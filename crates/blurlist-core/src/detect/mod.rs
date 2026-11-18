//! Face detection.
//!
//! Detectors run on a fixed 640x640 canvas. Event photos are often 20+
//! megapixels with small faces at the back of a group, so one downscaled pass
//! would miss them. [`detect_faces`] therefore combines a whole-image pass with
//! overlapping high-resolution tiles and merges the results.

mod scrfd;
mod yunet;

use std::path::Path;

use image::RgbImage;
use serde::{Deserialize, Serialize};

pub use scrfd::ScrfdDetector;
pub use yunet::YunetDetector;

use crate::error::Result;
use crate::geometry::{nms, BBox, Landmarks, Point, Scored};
use crate::models::ModelFamily;

/// One detected face, in the pixel coordinates of the image it was found in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub bbox: BBox,
    pub landmarks: Landmarks,
    pub score: f32,
}

impl Scored for Detection {
    fn bbox(&self) -> &BBox {
        &self.bbox
    }
    fn score(&self) -> f32 {
        self.score
    }
}

impl Detection {
    fn map(&self, scale: f32, dx: f32, dy: f32) -> Detection {
        let f = |p: Point| Point::new(p.x * scale + dx, p.y * scale + dy);
        Detection {
            bbox: self.bbox.scale(scale).translate(dx, dy),
            landmarks: self.landmarks.map(f),
            score: self.score,
        }
    }
}

/// A face detection network.
pub trait FaceDetector: Send {
    /// Canvas size the network expects (width, height).
    fn input_size(&self) -> (u32, u32);

    /// Run the network on an image exactly `input_size()` in size and return
    /// detections above `threshold`, before non-maximum suppression.
    fn run(&mut self, canvas: &RgbImage, threshold: f32) -> Result<Vec<Detection>>;
}

/// Load a detector of the given family.
pub fn load_detector(family: ModelFamily, path: &Path, threads: usize) -> Result<Box<dyn FaceDetector>> {
    match family {
        ModelFamily::Yunet => Ok(Box::new(YunetDetector::new(path, threads)?)),
        ModelFamily::Scrfd => Ok(Box::new(ScrfdDetector::new(path, threads)?)),
        other => Err(crate::Error::Model(format!("{other:?} is not a face detector"))),
    }
}

/// How hard to look for small faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Thoroughness {
    /// One pass over the downscaled photo. Finds faces larger than ~4% of the
    /// photo's long side.
    Fast,
    /// Adds a tiled pass at 1920 px. Finds faces down to ~1% of the long side.
    #[default]
    Thorough,
    /// Adds a tiled pass at 3200 px for large crowds and stage shots.
    Maximum,
}

impl Thoroughness {
    fn tile_long_side(self) -> Option<u32> {
        match self {
            Thoroughness::Fast => None,
            Thoroughness::Thorough => Some(1920),
            Thoroughness::Maximum => Some(3200),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DetectOptions {
    pub threshold: f32,
    pub thoroughness: Thoroughness,
    pub nms_iou: f32,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self { threshold: 0.5, thoroughness: Thoroughness::Thorough, nms_iou: 0.4 }
    }
}

const TILE_OVERLAP: u32 = 192;

/// Find every face in `img`, returned in `img`'s pixel coordinates, sorted
/// top-to-bottom then left-to-right.
pub fn detect_faces(detector: &mut dyn FaceDetector, img: &RgbImage, opts: &DetectOptions) -> Result<Vec<Detection>> {
    let (cw, ch) = detector.input_size();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();

    // 1. Whole-image pass, letterboxed into the canvas (top-left aligned).
    let scale = (cw as f32 / w as f32).min(ch as f32 / h as f32).min(4.0);
    let (rw, rh) = (((w as f32 * scale).round() as u32).clamp(1, cw), ((h as f32 * scale).round() as u32).clamp(1, ch));
    let resized = if (rw, rh) == (w, h) {
        img.clone()
    } else {
        image::imageops::resize(img, rw, rh, image::imageops::FilterType::Triangle)
    };
    let canvas = paste_on_canvas(&resized, 0, 0, rw, rh, cw, ch);
    for d in detector.run(&canvas, opts.threshold)? {
        all.push(d.map(w as f32 / rw as f32, 0.0, 0.0));
    }

    // 2. Tiled pass at higher resolution.
    if let Some(long) = opts.thoroughness.tile_long_side() {
        let native_long = w.max(h);
        let target_long = native_long.min(long);
        // Only worth it when tiles see the photo at clearly higher resolution.
        if target_long as f32 > cw.max(ch) as f32 * 1.3 {
            let s = target_long as f32 / native_long as f32;
            let (ww, wh) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
            let work = if (ww, wh) == (w, h) {
                img.clone()
            } else {
                image::imageops::resize(img, ww, wh, image::imageops::FilterType::Triangle)
            };
            let back = w as f32 / ww as f32;
            for ty in tile_starts(wh, ch) {
                for tx in tile_starts(ww, cw) {
                    let tw = cw.min(ww - tx);
                    let th = ch.min(wh - ty);
                    let canvas = paste_on_canvas(&work, tx, ty, tw, th, cw, ch);
                    for d in detector.run(&canvas, opts.threshold)? {
                        // Drop faces cut by an inner tile edge: the overlap
                        // guarantees a neighbouring tile sees them whole.
                        let b = d.bbox;
                        let cut_left = tx > 0 && b.x < 2.0;
                        let cut_top = ty > 0 && b.y < 2.0;
                        let cut_right = tx + tw < ww && b.x2() > tw as f32 - 2.0;
                        let cut_bottom = ty + th < wh && b.y2() > th as f32 - 2.0;
                        if cut_left || cut_top || cut_right || cut_bottom {
                            continue;
                        }
                        let m = d.map(1.0, tx as f32, ty as f32).map(back, 0.0, 0.0);
                        all.push(m);
                    }
                }
            }
        }
    }

    let mut kept = nms(&all, opts.nms_iou, 0.75);
    for d in &mut kept {
        d.bbox = d.bbox.clamp(w as f32, h as f32);
    }
    kept.retain(|d| d.bbox.w >= 4.0 && d.bbox.h >= 4.0);
    kept.sort_by(|a, b| {
        let ra = (a.bbox.y / 40.0).floor();
        let rb = (b.bbox.y / 40.0).floor();
        ra.partial_cmp(&rb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.bbox.x.partial_cmp(&b.bbox.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    Ok(kept)
}

/// Start offsets of overlapping tiles covering `len` pixels.
fn tile_starts(len: u32, tile: u32) -> Vec<u32> {
    if len <= tile {
        return vec![0];
    }
    let step = tile - TILE_OVERLAP;
    let mut v = Vec::new();
    let mut s = 0;
    while s + tile < len {
        v.push(s);
        s += step;
    }
    v.push(len - tile);
    v.dedup();
    v
}

/// Copy the `tw x th` region at (`x`, `y`) of `src` onto the top-left of a
/// black `cw x ch` canvas.
fn paste_on_canvas(src: &RgbImage, x: u32, y: u32, tw: u32, th: u32, cw: u32, ch: u32) -> RgbImage {
    if x == 0 && y == 0 && tw == cw && th == ch && src.dimensions() == (cw, ch) {
        return src.clone();
    }
    let mut canvas = RgbImage::new(cw, ch);
    let sw = src.width() as usize;
    let raw = src.as_raw();
    let out = canvas.as_mut();
    for row in 0..th as usize {
        let s = ((y as usize + row) * sw + x as usize) * 3;
        let d = row * cw as usize * 3;
        out[d..d + tw as usize * 3].copy_from_slice(&raw[s..s + tw as usize * 3]);
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_everything_with_overlap() {
        assert_eq!(tile_starts(500, 640), vec![0]);
        let t = tile_starts(1920, 640);
        assert_eq!(*t.first().unwrap(), 0);
        assert_eq!(*t.last().unwrap() + 640, 1920);
        for pair in t.windows(2) {
            assert!(pair[1] - pair[0] <= 640 - TILE_OVERLAP);
        }
    }

    #[test]
    fn canvas_paste() {
        let mut src = RgbImage::new(4, 4);
        src.put_pixel(2, 3, image::Rgb([9, 9, 9]));
        let c = paste_on_canvas(&src, 1, 2, 3, 2, 8, 8);
        assert_eq!(c.get_pixel(1, 1).0, [9, 9, 9]);
        assert_eq!(c.get_pixel(5, 5).0, [0, 0, 0]);
    }

    struct Fake;
    impl FaceDetector for Fake {
        fn input_size(&self) -> (u32, u32) {
            (64, 64)
        }
        fn run(&mut self, _canvas: &RgbImage, _t: f32) -> Result<Vec<Detection>> {
            Ok(vec![Detection {
                bbox: BBox::new(10.0, 10.0, 8.0, 8.0),
                landmarks: [Point::new(14.0, 14.0); 5],
                score: 0.9,
            }])
        }
    }

    #[test]
    fn global_pass_maps_back_to_image_coordinates() {
        let img = RgbImage::new(128, 64);
        let opts = DetectOptions { thoroughness: Thoroughness::Fast, ..Default::default() };
        let found = detect_faces(&mut Fake, &img, &opts).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bbox, BBox::new(20.0, 20.0, 16.0, 16.0));
        assert_eq!(found[0].landmarks[0], Point::new(28.0, 28.0));
    }
}
