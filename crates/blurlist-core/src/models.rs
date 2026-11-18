//! Model catalogue, discovery on disk and ONNX Runtime session management.
//!
//! Blurlist ships with two permissively licensed models (YuNet, MIT; SFace,
//! Apache-2.0). It can also run InsightFace's SCRFD detectors and ArcFace
//! recognisers, whose published weights are licensed for non-commercial
//! research only, so those are never bundled: users who are entitled to use
//! them can drop the files into their models folder or import them.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::session::{builder::GraphOptimizationLevel, Session};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// What a model does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Detector,
    Embedder,
}

/// The network architecture family. Decides pre- and post-processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFamily {
    /// YuNet (OpenCV Zoo). Fixed 640x640 BGR input, 12 outputs.
    Yunet,
    /// SCRFD (InsightFace) with 5-point keypoints. Dynamic RGB input, 9 outputs.
    Scrfd,
    /// SFace (OpenCV Zoo). 112x112 RGB in 0..255, 128-d output.
    Sface,
    /// ArcFace-style recogniser (InsightFace layout). 112x112 RGB in -1..1.
    Arcface,
}

impl ModelFamily {
    pub fn kind(self) -> ModelKind {
        match self {
            ModelFamily::Yunet | ModelFamily::Scrfd => ModelKind::Detector,
            ModelFamily::Sface | ModelFamily::Arcface => ModelKind::Embedder,
        }
    }

    /// Default similarity thresholds `(review, match)` for embedders. Faces
    /// scoring at least `match` are treated as a roster match; scores between
    /// `review` and `match` go to the "Check these" pile.
    pub fn default_thresholds(self) -> Thresholds {
        match self {
            // OpenCV's published same-identity cosine threshold for SFace is 0.363.
            ModelFamily::Sface => Thresholds { review: 0.28, matched: 0.42 },
            // InsightFace recognisers (w600k / glint) separate identities at ~0.28-0.35.
            ModelFamily::Arcface => Thresholds { review: 0.20, matched: 0.36 },
            _ => Thresholds { review: 0.3, matched: 0.45 },
        }
    }
}

/// Cosine-similarity thresholds for an embedder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// At or above this, a face goes to the "Check these" pile.
    pub review: f32,
    /// At or above this, a face counts as a roster match.
    #[serde(rename = "match")]
    pub matched: f32,
}

/// Static description of a model Blurlist knows about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub family: ModelFamily,
    pub kind: ModelKind,
    pub file: String,
    pub license: String,
    pub license_url: String,
    pub source_url: String,
    /// `true` if the weights' licence allows use outside non-commercial research.
    pub permissive: bool,
    /// Shipped inside the Blurlist installers.
    pub bundled: bool,
    /// Full path, filled in when the file is found on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

pub const DEFAULT_DETECTOR: &str = "yunet-2023mar";
pub const DEFAULT_EMBEDDER: &str = "sface-2021dec";

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    family: ModelFamily,
    file: &'static str,
    license: &'static str,
    license_url: &'static str,
    source_url: &'static str,
    permissive: bool,
    bundled: bool,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "yunet-2023mar",
        name: "YuNet face detector (2023mar)",
        family: ModelFamily::Yunet,
        file: "face_detection_yunet_2023mar.onnx",
        license: "MIT",
        license_url: "https://github.com/opencv/opencv_zoo/blob/main/models/face_detection_yunet/LICENSE",
        source_url: "https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet",
        permissive: true,
        bundled: true,
    },
    CatalogEntry {
        id: "sface-2021dec",
        name: "SFace recogniser (2021dec)",
        family: ModelFamily::Sface,
        file: "face_recognition_sface_2021dec.onnx",
        license: "Apache-2.0",
        license_url: "https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/LICENSE",
        source_url: "https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface",
        permissive: true,
        bundled: true,
    },
    CatalogEntry {
        id: "scrfd-10g",
        name: "SCRFD-10G detector (InsightFace buffalo_l)",
        family: ModelFamily::Scrfd,
        file: "det_10g.onnx",
        license: "Non-commercial research only",
        license_url: "https://github.com/deepinsight/insightface#license",
        source_url: "https://github.com/deepinsight/insightface/tree/master/model_zoo",
        permissive: false,
        bundled: false,
    },
    CatalogEntry {
        id: "scrfd-500m",
        name: "SCRFD-500M detector (InsightFace buffalo_sc)",
        family: ModelFamily::Scrfd,
        file: "det_500m.onnx",
        license: "Non-commercial research only",
        license_url: "https://github.com/deepinsight/insightface#license",
        source_url: "https://github.com/deepinsight/insightface/tree/master/model_zoo",
        permissive: false,
        bundled: false,
    },
    CatalogEntry {
        id: "arcface-w600k-r50",
        name: "ArcFace R50 WebFace600K (InsightFace buffalo_l)",
        family: ModelFamily::Arcface,
        file: "w600k_r50.onnx",
        license: "Non-commercial research only",
        license_url: "https://github.com/deepinsight/insightface#license",
        source_url: "https://github.com/deepinsight/insightface/tree/master/model_zoo",
        permissive: false,
        bundled: false,
    },
    CatalogEntry {
        id: "arcface-w600k-mbf",
        name: "ArcFace MobileFaceNet WebFace600K (InsightFace buffalo_sc)",
        family: ModelFamily::Arcface,
        file: "w600k_mbf.onnx",
        license: "Non-commercial research only",
        license_url: "https://github.com/deepinsight/insightface#license",
        source_url: "https://github.com/deepinsight/insightface/tree/master/model_zoo",
        permissive: false,
        bundled: false,
    },
];

/// Sidecar metadata for a model the user imported (`<file>.onnx.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomModelMeta {
    pub name: String,
    pub family: ModelFamily,
    #[serde(default)]
    pub license: String,
}

/// Every model Blurlist knows about, with `path` set for those found on disk.
/// Catalogue entries are listed even when missing so the UI can explain them.
pub fn discover_models(search_dirs: &[PathBuf]) -> Vec<ModelInfo> {
    let mut out: Vec<ModelInfo> = CATALOG
        .iter()
        .map(|c| ModelInfo {
            id: c.id.into(),
            name: c.name.into(),
            family: c.family,
            kind: c.family.kind(),
            file: c.file.into(),
            license: c.license.into(),
            license_url: c.license_url.into(),
            source_url: c.source_url.into(),
            permissive: c.permissive,
            bundled: c.bundled,
            path: search_dirs.iter().map(|d| d.join(c.file)).find(|p| p.is_file()),
        })
        .collect();

    for dir in search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            if path.extension().and_then(|e| e.to_str()) != Some("onnx") {
                continue;
            }
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            if CATALOG.iter().any(|c| c.file == file) {
                continue;
            }
            let sidecar = PathBuf::from(format!("{}.json", path.display()));
            let Some(meta) =
                std::fs::read(&sidecar).ok().and_then(|b| serde_json::from_slice::<CustomModelMeta>(&b).ok())
            else {
                continue;
            };
            let id = format!("custom-{}", file.trim_end_matches(".onnx"));
            if out.iter().any(|m| m.id == id) {
                continue;
            }
            out.push(ModelInfo {
                id,
                name: meta.name,
                family: meta.family,
                kind: meta.family.kind(),
                file,
                license: if meta.license.is_empty() { "Unknown — check the model card".into() } else { meta.license },
                license_url: String::new(),
                source_url: String::new(),
                permissive: false,
                bundled: false,
                path: Some(path),
            });
        }
    }
    out
}

/// Copy an ONNX file into `dest_dir` and register it with a sidecar.
pub fn import_model(src: &Path, dest_dir: &Path, name: &str, family: ModelFamily, license: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir).map_err(|e| Error::io(dest_dir, e))?;
    let stem: String = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let mut dest = dest_dir.join(format!("{stem}.onnx"));
    let mut n = 2;
    while dest.exists() || CATALOG.iter().any(|c| dest.file_name().is_some_and(|f| f == c.file)) {
        dest = dest_dir.join(format!("{stem}-{n}.onnx"));
        n += 1;
    }
    // Validate before copying: it must load and look like the chosen family.
    let session = new_session(src, 1)?;
    validate_family(&session, family)?;
    drop(session);
    std::fs::copy(src, &dest).map_err(|e| Error::io(&dest, e))?;
    let meta = CustomModelMeta { name: name.to_string(), family, license: license.to_string() };
    let sidecar = PathBuf::from(format!("{}.json", dest.display()));
    std::fs::write(&sidecar, serde_json::to_vec_pretty(&meta)?).map_err(|e| Error::io(&sidecar, e))?;
    Ok(dest)
}

fn environment() -> Result<&'static ort::environment::Environment> {
    static ENV: OnceLock<std::result::Result<ort::environment::Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| ort::init().with_name("blurlist").with_telemetry(false).build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Model(format!("could not start ONNX Runtime: {e}")))
}

/// Create an ONNX Runtime session for a model file.
pub fn new_session(path: &Path, threads: usize) -> Result<Session> {
    let env = environment()?;
    let map = |e: ort::Error<ort::session::builder::SessionBuilder>| Error::Model(e.to_string());
    let mut builder = Session::builder(env)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(map)?
        .with_intra_threads(threads.max(1))
        .map_err(map)?;
    builder.commit_from_file(path).map_err(|e| Error::Model(format!("{}: {e}", path.display())))
}

/// Check a session's inputs/outputs match what a family's decoder expects.
pub fn validate_family(session: &Session, family: ModelFamily) -> Result<()> {
    let n_in = session.inputs().len();
    let n_out = session.outputs().len();
    let ok = match family {
        ModelFamily::Yunet => n_in == 1 && n_out == 12,
        ModelFamily::Scrfd => n_in == 1 && n_out == 9,
        ModelFamily::Sface | ModelFamily::Arcface => n_in == 1 && n_out == 1,
    };
    if ok {
        Ok(())
    } else {
        Err(Error::Model(format!("this file does not look like a {family:?} model ({n_in} inputs, {n_out} outputs)")))
    }
}

/// Convert an RGB image into a flat NCHW float tensor buffer.
pub(crate) fn to_nchw(img: &image::RgbImage, mean: f32, inv_std: f32, bgr: bool) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let plane = (w * h) as usize;
    let mut out = vec![0f32; plane * 3];
    let raw = img.as_raw();
    for i in 0..plane {
        let (r, g, b) = (raw[i * 3] as f32, raw[i * 3 + 1] as f32, raw[i * 3 + 2] as f32);
        let (c0, c1, c2) = if bgr { (b, g, r) } else { (r, g, b) };
        out[i] = (c0 - mean) * inv_std;
        out[plane + i] = (c1 - mean) * inv_std;
        out[2 * plane + i] = (c2 - mean) * inv_std;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let models = discover_models(&[]);
        let mut ids: Vec<_> = models.iter().map(|m| m.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), models.len());
        assert!(models.iter().any(|m| m.id == DEFAULT_DETECTOR && m.bundled && m.permissive));
        assert!(models.iter().any(|m| m.id == DEFAULT_EMBEDDER && m.bundled && m.permissive));
        // Research-only weights must never be marked as bundled.
        assert!(models.iter().all(|m| m.permissive || !m.bundled));
    }

    #[test]
    fn nchw_layout() {
        let mut img = image::RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([10, 20, 30]));
        img.put_pixel(1, 0, image::Rgb([40, 50, 60]));
        assert_eq!(to_nchw(&img, 0.0, 1.0, false), vec![10., 40., 20., 50., 30., 60.]);
        assert_eq!(to_nchw(&img, 0.0, 1.0, true), vec![30., 60., 20., 50., 10., 40.]);
    }
}
