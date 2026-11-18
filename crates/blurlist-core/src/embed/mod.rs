//! Face alignment and identity embeddings.

use std::path::Path;

use image::{Rgb, RgbImage};
use ort::{session::Session, value::Tensor};

use crate::error::{Error, Result};
use crate::geometry::{estimate_similarity, Affine, Landmarks, Point};
use crate::models::{new_session, to_nchw, validate_family, ModelFamily};

/// The five-point template InsightFace and OpenCV align faces to (112x112).
pub const ARCFACE_TEMPLATE: [Point; 5] = [
    Point::new(38.2946, 51.6963),
    Point::new(73.5318, 51.5014),
    Point::new(56.0252, 71.7366),
    Point::new(41.5493, 92.3655),
    Point::new(70.7299, 92.2041),
];

pub const ALIGNED_SIZE: u32 = 112;

/// Warp the face described by `landmarks` onto the 112x112 template.
pub fn align_face(img: &RgbImage, landmarks: &Landmarks) -> Result<RgbImage> {
    let m = estimate_similarity(landmarks, &ARCFACE_TEMPLATE)
        .ok_or_else(|| Error::Invalid("degenerate face landmarks".into()))?;
    warp_affine(img, &m, ALIGNED_SIZE, ALIGNED_SIZE)
}

/// Bilinear `warpAffine` (OpenCV semantics, constant black border).
pub fn warp_affine(img: &RgbImage, m: &Affine, out_w: u32, out_h: u32) -> Result<RgbImage> {
    let inv = m.invert().ok_or_else(|| Error::Invalid("non-invertible transform".into()))?;
    let (w, h) = img.dimensions();
    let (wi, hi) = (w as i64, h as i64);
    let raw = img.as_raw();
    let sample = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= wi || y >= hi {
            0.0
        } else {
            raw[((y * wi + x) * 3) as usize + c] as f32
        }
    };
    let mut out = RgbImage::new(out_w, out_h);
    for v in 0..out_h {
        for u in 0..out_w {
            let p = inv.apply(Point::new(u as f32, v as f32));
            let x0 = p.x.floor();
            let y0 = p.y.floor();
            let fx = p.x - x0;
            let fy = p.y - y0;
            let (x0, y0) = (x0 as i64, y0 as i64);
            let mut px = [0u8; 3];
            for (c, val) in px.iter_mut().enumerate() {
                let a = sample(x0, y0, c);
                let b = sample(x0 + 1, y0, c);
                let cc = sample(x0, y0 + 1, c);
                let d = sample(x0 + 1, y0 + 1, c);
                let top = a + (b - a) * fx;
                let bot = cc + (d - cc) * fx;
                *val = (top + (bot - top) * fy).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(u, v, Rgb(px));
        }
    }
    Ok(out)
}

/// A face recognition network producing L2-normalised identity vectors.
pub trait FaceEmbedder: Send {
    fn dim(&self) -> usize;
    /// Embed an aligned 112x112 face.
    fn embed(&mut self, aligned: &RgbImage) -> Result<Vec<f32>>;
}

/// SFace / ArcFace style embedder. The two differ only in input scaling.
pub struct OnnxEmbedder {
    session: Session,
    input_name: String,
    mean: f32,
    inv_std: f32,
    dim: usize,
}

impl OnnxEmbedder {
    pub fn new(path: &Path, family: ModelFamily, threads: usize) -> Result<Self> {
        let session = new_session(path, threads)?;
        validate_family(&session, family)?;
        let (mean, inv_std) = match family {
            // OpenCV FaceRecognizerSF: blobFromImage(scale=1, mean=0, swapRB=true).
            ModelFamily::Sface => (0.0, 1.0),
            // InsightFace ArcFaceONNX: (x - 127.5) / 127.5, RGB.
            ModelFamily::Arcface => (127.5, 1.0 / 127.5),
            other => return Err(Error::Model(format!("{other:?} is not a face recogniser"))),
        };
        let input_name = session.inputs()[0].name().to_string();
        if let Some(shape) = session.inputs()[0].dtype().tensor_shape() {
            if shape.len() != 4 || (shape[2] > 0 && shape[2] != 112) || (shape[3] > 0 && shape[3] != 112) {
                return Err(Error::Model("recogniser must take 112x112 input".into()));
            }
        }
        let dim = session.outputs()[0]
            .dtype()
            .tensor_shape()
            .and_then(|s| s.last().copied())
            .filter(|&d| d > 0)
            .unwrap_or(512) as usize;
        Ok(Self { session, input_name, mean, inv_std, dim })
    }
}

impl FaceEmbedder for OnnxEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&mut self, aligned: &RgbImage) -> Result<Vec<f32>> {
        if aligned.dimensions() != (ALIGNED_SIZE, ALIGNED_SIZE) {
            return Err(Error::Invalid("face must be aligned to 112x112".into()));
        }
        let data = to_nchw(aligned, self.mean, self.inv_std, false);
        let input = Tensor::from_array(([1usize, 3, 112, 112], data))?;
        let outputs = self.session.run(ort::inputs![self.input_name.as_str() => input])?;
        let (_, v) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(l2_normalize(v.to_vec()))
    }
}

pub fn load_embedder(family: ModelFamily, path: &Path, threads: usize) -> Result<Box<dyn FaceEmbedder>> {
    Ok(Box::new(OnnxEmbedder::new(path, family, threads)?))
}

pub fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-12 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

/// Cosine similarity of two L2-normalised vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_alignment_is_lossless() {
        let mut img = RgbImage::new(112, 112);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([x as u8, y as u8, (x ^ y) as u8]);
        }
        let out = align_face(&img, &ARCFACE_TEMPLATE).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn alignment_undoes_scaling() {
        // A face twice as large, offset by (10, 20).
        let lm = ARCFACE_TEMPLATE.map(|p| Point::new(p.x * 2.0 + 10.0, p.y * 2.0 + 20.0));
        let mut img = RgbImage::new(300, 300);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([((x as i64 - 10) / 2).clamp(0, 255) as u8, ((y as i64 - 20) / 2).clamp(0, 255) as u8, 0]);
        }
        let out = align_face(&img, &lm).unwrap();
        let px = out.get_pixel(56, 70).0;
        assert!((px[0] as i32 - 56).abs() <= 1 && (px[1] as i32 - 70).abs() <= 1);
    }

    #[test]
    fn cosine_of_normalised() {
        let a = l2_normalize(vec![3.0, 4.0]);
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
        let b = l2_normalize(vec![-4.0, 3.0]);
        assert!(cosine(&a, &b).abs() < 1e-6);
    }
}
