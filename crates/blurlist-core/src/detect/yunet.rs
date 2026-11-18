//! YuNet (OpenCV Zoo, MIT licence) decoder.
//!
//! Mirrors `cv::FaceDetectorYN` for the 2023mar model: BGR input in 0..255,
//! three strides with separate class / objectness / box / keypoint heads.

use std::path::Path;

use image::RgbImage;
use ort::{session::Session, value::Tensor};

use super::{Detection, FaceDetector};
use crate::error::{Error, Result};
use crate::geometry::{BBox, Point};
use crate::models::{new_session, to_nchw, validate_family, ModelFamily};

const STRIDES: [u32; 3] = [8, 16, 32];

pub struct YunetDetector {
    session: Session,
    input_name: String,
    size: (u32, u32),
}

impl YunetDetector {
    pub fn new(path: &Path, threads: usize) -> Result<Self> {
        let session = new_session(path, threads)?;
        validate_family(&session, ModelFamily::Yunet)?;
        let input_name = session.inputs()[0].name().to_string();
        let size = match session.inputs()[0].dtype().tensor_shape() {
            Some(shape) if shape.len() == 4 && shape[2] > 0 && shape[3] > 0 => (shape[3] as u32, shape[2] as u32),
            _ => (640, 640),
        };
        for s in STRIDES {
            for head in ["cls", "obj", "bbox", "kps"] {
                let name = format!("{head}_{s}");
                if !session.outputs().iter().any(|o| o.name() == name) {
                    return Err(Error::Model(format!("YuNet model is missing output {name}")));
                }
            }
        }
        Ok(Self { session, input_name, size })
    }
}

impl FaceDetector for YunetDetector {
    fn input_size(&self) -> (u32, u32) {
        self.size
    }

    fn run(&mut self, canvas: &RgbImage, threshold: f32) -> Result<Vec<Detection>> {
        let (w, h) = self.size;
        debug_assert_eq!(canvas.dimensions(), (w, h));
        let data = to_nchw(canvas, 0.0, 1.0, true);
        let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data))?;
        let outputs = self.session.run(ort::inputs![self.input_name.as_str() => input])?;

        let mut dets = Vec::new();
        for stride in STRIDES {
            let cols = (w / stride) as usize;
            let rows = (h / stride) as usize;
            let get = |head: &str| -> Result<&[f32]> {
                let (_, d) = outputs[format!("{head}_{stride}").as_str()].try_extract_tensor::<f32>()?;
                Ok(d)
            };
            let cls = get("cls")?;
            let obj = get("obj")?;
            let bbox = get("bbox")?;
            let kps = get("kps")?;
            let n = rows * cols;
            if cls.len() < n || obj.len() < n || bbox.len() < n * 4 || kps.len() < n * 10 {
                return Err(Error::Model("unexpected YuNet output size".into()));
            }
            let s = stride as f32;
            for r in 0..rows {
                for c in 0..cols {
                    let i = r * cols + c;
                    let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                    if score < threshold {
                        continue;
                    }
                    let cx = (c as f32 + bbox[i * 4]) * s;
                    let cy = (r as f32 + bbox[i * 4 + 1]) * s;
                    let bw = bbox[i * 4 + 2].exp() * s;
                    let bh = bbox[i * 4 + 3].exp() * s;
                    let mut lm = [Point::default(); 5];
                    for (k, p) in lm.iter_mut().enumerate() {
                        *p = Point::new((kps[i * 10 + 2 * k] + c as f32) * s, (kps[i * 10 + 2 * k + 1] + r as f32) * s);
                    }
                    dets.push(Detection {
                        bbox: BBox::new(cx - bw / 2.0, cy - bh / 2.0, bw, bh),
                        landmarks: lm,
                        score,
                    });
                }
            }
        }
        Ok(crate::geometry::nms(&dets, 0.3, 1.1))
    }
}
