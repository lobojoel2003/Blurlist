//! Irreversible face anonymisation.
//!
//! A light Gaussian blur can be partly undone, and even a strong one leaves
//! the face's structure visible. Blurlist first averages the face region down
//! to a handful of colour cells, which throws the identity away, then smooths
//! the result so it reads as a soft blur instead of a mosaic. The effect is
//! blended in with an elliptical feathered mask that fully covers the
//! detected face box plus hair, ears and chin.

use image::{imageops, imageops::FilterType, Rgb, RgbImage};
use serde::{Deserialize, Serialize};

use crate::geometry::BBox;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BlurStyle {
    /// Soft, heavy blur (default).
    #[default]
    Blur,
    /// Visible mosaic blocks.
    Pixelate,
    /// Flat fill with the region's average colour.
    Solid,
}

/// Horizontal scale of the anonymised area relative to the detected box.
const EXPAND_X: f32 = 1.6;
/// Vertical scale (taller, to cover hair and chin).
const EXPAND_Y: f32 = 1.75;
/// Normalised ellipse radius where feathering starts. With the factors above
/// the corners of the detected box sit at radius ~0.85, inside the solid core.
const FEATHER_START: f32 = 0.88;
/// Colour cells across the face; identity cannot survive this.
const CELLS_ACROSS: u32 = 7;

/// The region that will be modified for a face box.
pub fn blur_region(face: &BBox) -> BBox {
    face.expand(EXPAND_X, EXPAND_Y)
}

/// Anonymise every box in `faces` in place.
pub fn anonymize(img: &mut RgbImage, faces: &[BBox], style: BlurStyle) {
    for f in faces {
        anonymize_one(img, f, style);
    }
}

fn anonymize_one(img: &mut RgbImage, face: &BBox, style: BlurStyle) {
    let (iw, ih) = img.dimensions();
    let region = blur_region(face);
    let Some((rx, ry, rw, rh)) = region.to_pixel_rect(iw, ih) else {
        return;
    };
    let patch = imageops::crop_imm(img, rx, ry, rw, rh).to_image();
    let anon = anonymized_patch(&patch, face, style);

    // Elliptical mask in the coordinates of the *unclipped* region so faces at
    // the photo edge keep their full coverage.
    let c = region.center();
    let (a, b) = (region.w / 2.0, region.h / 2.0);
    for py in 0..rh {
        for px in 0..rw {
            let gx = (rx + px) as f32 + 0.5;
            let gy = (ry + py) as f32 + 0.5;
            let nx = (gx - c.x) / a;
            let ny = (gy - c.y) / b;
            let r = (nx * nx + ny * ny).sqrt();
            let alpha = if r <= FEATHER_START {
                1.0
            } else if r >= 1.0 {
                0.0
            } else {
                let t = (1.0 - r) / (1.0 - FEATHER_START);
                t * t * (3.0 - 2.0 * t)
            };
            if alpha <= 0.0 {
                continue;
            }
            let src = anon.get_pixel(px, py).0;
            let dst = img.get_pixel_mut(rx + px, ry + py);
            for (d, s) in dst.0.iter_mut().zip(src) {
                let v = s as f32 * alpha + *d as f32 * (1.0 - alpha);
                *d = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn anonymized_patch(patch: &RgbImage, face: &BBox, style: BlurStyle) -> RgbImage {
    let (w, h) = patch.dimensions();
    match style {
        BlurStyle::Solid => {
            let avg = average(patch);
            RgbImage::from_pixel(w, h, avg)
        }
        BlurStyle::Pixelate | BlurStyle::Blur => {
            // Cell size derived from the face (not the region) so coverage is
            // equally destructive for every face size.
            let cell = (face.w.max(face.h) / CELLS_ACROSS as f32).max(2.0);
            let cw = ((w as f32 / cell).round() as u32).max(1);
            let chh = ((h as f32 / cell).round() as u32).max(1);
            let small = area_downsample(patch, cw, chh);
            if style == BlurStyle::Pixelate {
                imageops::resize(&small, w, h, FilterType::Nearest)
            } else {
                let up = imageops::resize(&small, w, h, FilterType::Triangle);
                let sigma = (cell * 0.6).max(1.0);
                imageops::blur(&up, sigma)
            }
        }
    }
}

fn average(img: &RgbImage) -> Rgb<u8> {
    let mut sum = [0u64; 3];
    for p in img.pixels() {
        for (acc, v) in sum.iter_mut().zip(p.0) {
            *acc += v as u64;
        }
    }
    let n = (img.width() as u64 * img.height() as u64).max(1);
    Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
}

/// Box-filter downsample: every output cell is the exact mean of its source pixels.
fn area_downsample(img: &RgbImage, ow: u32, oh: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let mut out = RgbImage::new(ow, oh);
    for oy in 0..oh {
        let y0 = (oy as u64 * h as u64 / oh as u64) as u32;
        let y1 = (((oy + 1) as u64 * h as u64 / oh as u64) as u32).max(y0 + 1).min(h);
        for ox in 0..ow {
            let x0 = (ox as u64 * w as u64 / ow as u64) as u32;
            let x1 = (((ox + 1) as u64 * w as u64 / ow as u64) as u32).max(x0 + 1).min(w);
            let mut sum = [0u64; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    for (acc, v) in sum.iter_mut().zip(img.get_pixel(x, y).0) {
                        *acc += v as u64;
                    }
                }
            }
            let n = ((y1 - y0) as u64 * (x1 - x0) as u64).max(1);
            out.put_pixel(ox, oy, Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| if (x / 2 + y / 2) % 2 == 0 { Rgb([255, 255, 255]) } else { Rgb([0, 0, 0]) })
    }

    fn variance(img: &RgbImage, b: &BBox) -> f32 {
        let (x, y, w, h) = b.to_pixel_rect(img.width(), img.height()).unwrap();
        let mut vals = Vec::new();
        for yy in y..y + h {
            for xx in x..x + w {
                vals.push(img.get_pixel(xx, yy).0[0] as f32);
            }
        }
        let m = vals.iter().sum::<f32>() / vals.len() as f32;
        vals.iter().map(|v| (v - m).powi(2)).sum::<f32>() / vals.len() as f32
    }

    #[test]
    fn destroys_detail_inside_the_face_box() {
        for style in [BlurStyle::Blur, BlurStyle::Pixelate, BlurStyle::Solid] {
            let mut img = checkerboard(200, 200);
            let face = BBox::new(60.0, 60.0, 60.0, 70.0);
            let before = variance(&img, &face);
            anonymize(&mut img, &[face], style);
            let after = variance(&img, &face);
            assert!(after < before * 0.05, "{style:?}: {after} vs {before}");
        }
    }

    #[test]
    fn leaves_far_pixels_untouched() {
        let orig = checkerboard(300, 300);
        let mut img = orig.clone();
        anonymize(&mut img, &[BBox::new(20.0, 20.0, 40.0, 40.0)], BlurStyle::Blur);
        assert_eq!(img.get_pixel(250, 250), orig.get_pixel(250, 250));
        assert_eq!(img.get_pixel(150, 20), orig.get_pixel(150, 20));
    }

    #[test]
    fn handles_faces_at_the_edge() {
        let mut img = checkerboard(100, 100);
        let face = BBox::new(-10.0, 70.0, 40.0, 40.0);
        anonymize(&mut img, &[face], BlurStyle::Blur);
        let visible = face.clamp(100.0, 100.0);
        assert!(variance(&img, &visible) < 500.0);
    }

    #[test]
    fn box_corners_are_inside_the_solid_core() {
        let r = ((1.0 / EXPAND_X).powi(2) + (1.0 / EXPAND_Y).powi(2)).sqrt();
        assert!(r < FEATHER_START, "corner radius {r}");
    }
}
