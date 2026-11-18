//! SCRFD (InsightFace) decoder for the `*_bnkps` / buffalo models with five
//! keypoints. Mirrors `insightface.model_zoo.scrfd.SCRFD`: RGB input
//! normalised with mean 127.5 / std 128, anchor-free distance regression.
//!
//! The published SCRFD weights are for non-commercial research only, so
//! Blurlist never bundles them.

use std::path::Path;

use image::RgbImage;
use ort::{session::Session, value::Tensor};

use super::{Detection, FaceDetector};
use crate::error::{Error, Result};
use crate::geometry::{BBox, Point};
use crate::models::{new_session, to_nchw, validate_family, ModelFamily};

pub struct ScrfdDetector {
    session: Session,
    input_name: String,
    size: (u32, u32),
}

impl ScrfdDetector {
    pub fn new(path: &Path, threads: usize) -> Result<Self> {
        let session = new_session(path, threads)?;
        validate_family(&session, ModelFamily::Scrfd)?;
        let input_name = session.inputs()[0].name().to_string();
        let size = match session.inputs()[0].dtype().tensor_shape() {
            Some(shape) if shape.len() == 4 && shape[2] > 0 && shape[3] > 0 => (shape[3] as u32, shape[2] as u32),
            _ => (640, 640),
        };
        Ok(Self { session, input_name, size })
    }
}

struct Head<'a> {
    count: usize,
    data: &'a [f32],
}

impl FaceDetector for ScrfdDetector {
    fn input_size(&self) -> (u32, u32) {
        self.size
    }

    fn run(&mut self, canvas: &RgbImage, threshold: f32) -> Result<Vec<Detection>> {
        let (w, h) = self.size;
        let data = to_nchw(canvas, 127.5, 1.0 / 128.0, false);
        let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data))?;
        let outputs = self.session.run(ort::inputs![self.input_name.as_str() => input])?;

        // Group the nine outputs by head type (last dimension 1/4/10) and sort
        // each group from the finest stride (most anchors) to the coarsest.
        let mut scores: Vec<Head> = Vec::new();
        let mut boxes: Vec<Head> = Vec::new();
        let mut kps: Vec<Head> = Vec::new();
        for idx in 0..outputs.len() {
            let (shape, d) = outputs[idx].try_extract_tensor::<f32>()?;
            let last = *shape.last().unwrap_or(&0) as usize;
            if last == 0 {
                continue;
            }
            let head = Head { count: d.len() / last, data: d };
            match last {
                1 => scores.push(head),
                4 => boxes.push(head),
                10 => kps.push(head),
                _ => return Err(Error::Model(format!("unexpected SCRFD output width {last}"))),
            }
        }
        if scores.len() != 3 || boxes.len() != 3 || kps.len() != 3 {
            return Err(Error::Model("SCRFD model must have score, box and keypoint heads for 3 strides".into()));
        }
        for g in [&mut scores, &mut boxes, &mut kps] {
            g.sort_by_key(|h| std::cmp::Reverse(h.count));
        }

        let mut dets = Vec::new();
        for (level, stride) in [8u32, 16, 32].into_iter().enumerate() {
            let fw = (w / stride) as usize;
            let fh = (h / stride) as usize;
            let cells = fw * fh;
            let n = scores[level].count;
            if cells == 0 || !n.is_multiple_of(cells) || boxes[level].count != n || kps[level].count != n {
                return Err(Error::Model("SCRFD output sizes do not match the input size".into()));
            }
            let anchors = n / cells;
            let s = stride as f32;
            let (sc, bx, kp) = (scores[level].data, boxes[level].data, kps[level].data);
            for i in 0..n {
                let score = sc[i];
                if score < threshold {
                    continue;
                }
                let cell = i / anchors;
                let cx = (cell % fw) as f32 * s;
                let cy = (cell / fw) as f32 * s;
                let x1 = cx - bx[i * 4] * s;
                let y1 = cy - bx[i * 4 + 1] * s;
                let x2 = cx + bx[i * 4 + 2] * s;
                let y2 = cy + bx[i * 4 + 3] * s;
                let mut lm = [Point::default(); 5];
                for (k, p) in lm.iter_mut().enumerate() {
                    *p = Point::new(cx + kp[i * 10 + 2 * k] * s, cy + kp[i * 10 + 2 * k + 1] * s);
                }
                dets.push(Detection { bbox: BBox::from_corners(x1, y1, x2, y2), landmarks: lm, score });
            }
        }
        Ok(crate::geometry::nms(&dets, 0.4, 1.1))
    }
}
