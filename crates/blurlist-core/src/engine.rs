//! A loaded detector + recogniser pair, safe to share between threads.

use image::RgbImage;
use parking_lot::Mutex;

use crate::detect::{detect_faces, load_detector, DetectOptions, Detection, FaceDetector};
use crate::embed::{align_face, load_embedder, FaceEmbedder};
use crate::error::{Error, Result};
use crate::geometry::Landmarks;
use crate::models::{ModelInfo, ModelKind, Thresholds};

pub struct Engine {
    detector: Mutex<Box<dyn FaceDetector>>,
    embedder: Mutex<Box<dyn FaceEmbedder>>,
    pub detector_info: ModelInfo,
    pub embedder_info: ModelInfo,
}

impl Engine {
    /// Load both models. `threads` is the ONNX Runtime intra-op thread count.
    pub fn load(detector: &ModelInfo, embedder: &ModelInfo, threads: usize) -> Result<Self> {
        if detector.kind != ModelKind::Detector || embedder.kind != ModelKind::Embedder {
            return Err(Error::Invalid("need one face detector and one face recogniser".into()));
        }
        let dpath = detector
            .path
            .as_ref()
            .ok_or_else(|| Error::NotFound(format!("model file {} was not found", detector.file)))?;
        let epath = embedder
            .path
            .as_ref()
            .ok_or_else(|| Error::NotFound(format!("model file {} was not found", embedder.file)))?;
        Ok(Self {
            detector: Mutex::new(load_detector(detector.family, dpath, threads)?),
            embedder: Mutex::new(load_embedder(embedder.family, epath, threads)?),
            detector_info: detector.clone(),
            embedder_info: embedder.clone(),
        })
    }

    pub fn detect(&self, img: &RgbImage, opts: &DetectOptions) -> Result<Vec<Detection>> {
        let mut det = self.detector.lock();
        detect_faces(det.as_mut(), img, opts)
    }

    /// Align and embed one face.
    pub fn embed(&self, img: &RgbImage, landmarks: &Landmarks) -> Result<Vec<f32>> {
        let aligned = align_face(img, landmarks)?;
        self.embedder.lock().embed(&aligned)
    }

    /// Key under which embeddings from this recogniser are stored.
    pub fn embedding_key(&self) -> &str {
        &self.embedder_info.id
    }

    pub fn default_thresholds(&self) -> Thresholds {
        self.embedder_info.family.default_thresholds()
    }
}
