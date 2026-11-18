//! Building the opt-out roster from reference photos.

use std::path::{Path, PathBuf};

use image::RgbImage;
use serde::{Deserialize, Serialize};

use crate::detect::{Detection, Thoroughness};
use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::geometry::{BBox, Landmarks, Point};
use crate::imageio::{classify_path, encode_jpeg, fit_within, load_image, load_image_bytes, FileKind};
use crate::settings::Settings;
use crate::store::{NewReference, Store};

/// Reference faces smaller than this get a quality warning.
const REF_MIN_GOOD_PX: f32 = 80.0;
/// A face this many times larger than the next one is picked automatically.
const DOMINANT_RATIO: f32 = 2.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceCandidate {
    pub index: usize,
    pub bbox: BBox,
    pub score: f32,
    pub face_px: u32,
    /// JPEG preview of the face.
    #[serde(skip)]
    pub crop_jpeg: Vec<u8>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoInspection {
    pub width: u32,
    pub height: u32,
    pub faces: Vec<FaceCandidate>,
    /// Index of the face Blurlist would pick by itself, if any.
    pub suggested: Option<usize>,
}

fn reference_detections(engine: &Engine, img: &RgbImage, settings: &Settings) -> Result<Vec<Detection>> {
    let mut opts = settings.detect_options();
    opts.thoroughness = Thoroughness::Thorough;
    let mut faces = engine.detect(img, &opts)?;
    // Largest first: in a portrait the subject is the biggest face.
    faces.sort_by(|a, b| b.bbox.short_side().partial_cmp(&a.bbox.short_side()).unwrap_or(std::cmp::Ordering::Equal));
    Ok(faces)
}

fn suggested_index(faces: &[Detection]) -> Option<usize> {
    match faces {
        [] => None,
        [_] => Some(0),
        [a, b, ..] if a.bbox.short_side() >= b.bbox.short_side() * DOMINANT_RATIO => Some(0),
        _ => None,
    }
}

fn quality_warnings(d: &Detection) -> Vec<String> {
    let mut w = Vec::new();
    if d.bbox.short_side() < REF_MIN_GOOD_PX {
        w.push("The face is small in this photo; a closer photo will match more reliably.".into());
    }
    if d.score < 0.7 {
        w.push("The face may be turned, blurry or partly hidden.".into());
    }
    w
}

/// Square crop around a face with generous margin. Returns the crop and the
/// transform (scale, offset) from image to crop coordinates.
fn margin_crop(img: &RgbImage, b: &BBox, factor: f32, max_side: u32) -> (RgbImage, f32, Point) {
    let side = b.w.max(b.h) * factor;
    let c = b.center();
    let region = BBox::new(c.x - side / 2.0, c.y - side / 2.0, side, side);
    let (x, y, w, h) = region.to_pixel_rect(img.width(), img.height()).unwrap_or((0, 0, img.width(), img.height()));
    let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let s = (max_side as f32 / w.max(h) as f32).min(1.0);
    let crop = if s < 1.0 {
        let nw = ((w as f32 * s).round() as u32).max(1);
        let nh = ((h as f32 * s).round() as u32).max(1);
        image::imageops::resize(&crop, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        crop
    };
    (crop, s, Point::new(x as f32, y as f32))
}

/// Preview crop used by the review screen.
pub fn display_crop(img: &RgbImage, b: &BBox) -> Result<Vec<u8>> {
    let (crop, _, _) = margin_crop(img, b, 1.9, 240);
    encode_jpeg(&crop, 85)
}

/// Re-frame a stored reference crop so the face fills it the way
/// [`display_crop`] frames faces from event photos, for side-by-side review.
pub fn reference_face_view(crop_jpeg: &[u8], landmarks: &Landmarks) -> Result<Vec<u8>> {
    let img = load_image_bytes(crop_jpeg)?;
    let n = landmarks.len() as f32;
    let cx = landmarks.iter().map(|p| p.x).sum::<f32>() / n;
    let cy = landmarks.iter().map(|p| p.y).sum::<f32>() / n;
    // Eye-to-mouth distance is ~0.4 of the face box height.
    let eye_y = (landmarks[0].y + landmarks[1].y) / 2.0;
    let mouth_y = (landmarks[3].y + landmarks[4].y) / 2.0;
    let face_h = ((mouth_y - eye_y) / 0.4).max(8.0);
    let side = face_h * 1.9;
    let region = BBox::new(cx - side / 2.0, cy - side / 2.0 - face_h * 0.05, side, side);
    let Some((x, y, w, h)) = region.to_pixel_rect(img.width(), img.height()) else {
        return Ok(crop_jpeg.to_vec());
    };
    let view = image::imageops::crop_imm(&img, x, y, w, h).to_image();
    encode_jpeg(&fit_within(&view, 320), 88)
}

/// Detect faces in a candidate reference photo so the organiser can pick one.
pub fn inspect_reference_photo(engine: &Engine, path: &Path, settings: &Settings) -> Result<PhotoInspection> {
    let loaded = load_image(path)?;
    let faces = reference_detections(engine, &loaded.rgb, settings)?;
    let suggested = suggested_index(&faces);
    let mut out = Vec::with_capacity(faces.len());
    for (i, d) in faces.iter().enumerate() {
        out.push(FaceCandidate {
            index: i,
            bbox: d.bbox,
            score: d.score,
            face_px: d.bbox.short_side().round() as u32,
            crop_jpeg: display_crop(&loaded.rgb, &d.bbox)?,
            warnings: quality_warnings(d),
        });
    }
    Ok(PhotoInspection { width: loaded.rgb.width(), height: loaded.rgb.height(), faces: out, suggested })
}

/// Add a reference photo for a person. `face_index` refers to the order
/// returned by [`inspect_reference_photo`]; `None` picks automatically and
/// fails if the choice is ambiguous.
pub fn add_reference_photo(
    engine: &Engine,
    store: &Store,
    person_id: i64,
    path: &Path,
    face_index: Option<usize>,
    settings: &Settings,
) -> Result<i64> {
    let loaded = load_image(path)?;
    let faces = reference_detections(engine, &loaded.rgb, settings)?;
    let idx = match face_index {
        Some(i) if i < faces.len() => i,
        Some(_) => return Err(Error::Invalid("that face is no longer in the photo".into())),
        None => match (faces.len(), suggested_index(&faces)) {
            (0, _) => return Err(Error::Invalid("no face was found in this photo".into())),
            (_, Some(i)) => i,
            (n, None) => return Err(Error::Invalid(format!("{n} faces were found; choose which one is this person"))),
        },
    };
    let d = &faces[idx];
    let embedding = engine.embed(&loaded.rgb, &d.landmarks)?;
    let (crop, s, off) = margin_crop(&loaded.rgb, &d.bbox, 2.4, 448);
    let crop_landmarks = d.landmarks.map(|p| Point::new((p.x - off.x) * s, (p.y - off.y) * s));
    let source_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rid = store.add_reference(
        person_id,
        &NewReference {
            source_name,
            crop_jpeg: encode_jpeg(&crop, 90)?,
            crop_landmarks,
            det_score: d.score,
            face_px: d.bbox.short_side().round() as u32,
        },
    )?;
    store.put_reference_embedding(rid, engine.embedding_key(), &embedding)?;
    Ok(rid)
}

/// Embed reference faces that have no embedding for the active recogniser
/// (e.g. after switching models). Returns how many were added.
pub fn ensure_roster_embeddings(engine: &Engine, store: &Store) -> Result<usize> {
    let missing = store.references_missing_embedding(engine.embedding_key())?;
    for r in &missing {
        let crop = load_image_bytes(&r.crop_jpeg)?;
        let v = engine.embed(&crop, &r.crop_landmarks)?;
        store.put_reference_embedding(r.id, engine.embedding_key(), &v)?;
    }
    Ok(missing.len())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "status")]
pub enum ImportOutcome {
    Added { person_id: i64, created: bool },
    NeedsAttention { reason: String },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    pub file: String,
    pub path: PathBuf,
    pub name: String,
    pub group: String,
    #[serde(flatten)]
    pub outcome: ImportOutcome,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub people_created: usize,
    pub photos_added: usize,
    pub entries: Vec<ImportEntry>,
}

/// Turn a file name like `Ana_Perez (2).jpg` into a display name (`Ana Perez`).
pub fn name_from_file(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut s: String = stem.replace(['_', '.'], " ");
    // Strip a trailing copy number: "Ana (2)", "Ana-2", "Ana 2".
    let trimmed = s.trim_end();
    let without_paren = trimmed
        .strip_suffix(')')
        .and_then(|t| t.rsplit_once('('))
        .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map(|(head, _)| head.to_string());
    if let Some(h) = without_paren {
        s = h;
    } else {
        let digits = trimmed.trim_end_matches(|c: char| c.is_ascii_digit());
        if digits.len() < trimmed.len() && (digits.ends_with(' ') || digits.ends_with('-')) {
            s = digits.trim_end_matches(['-', ' ']).to_string();
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Import a folder of reference photos. Each file is one person, named after
/// the file; files in a sub-folder get the sub-folder's name as their group.
/// Several photos with the same name are attached to the same person.
pub fn import_roster_folder(
    engine: &Engine,
    store: &Store,
    dir: &Path,
    settings: &Settings,
    on_progress: &dyn Fn(usize, usize),
) -> Result<ImportReport> {
    if !dir.is_dir() {
        return Err(Error::NotFound(format!("{} is not a folder", dir.display())));
    }
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .min_depth(1)
        .max_depth(2)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .flatten()
    {
        if !entry.file_type().is_file() || classify_path(entry.path()) != FileKind::Supported {
            continue;
        }
        let group = if entry.depth() == 2 {
            entry
                .path()
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        files.push((entry.path().to_path_buf(), group));
    }

    let mut report = ImportReport::default();
    let existing = store.list_people()?;
    let total = files.len();
    for (i, (path, group)) in files.into_iter().enumerate() {
        on_progress(i, total);
        let name = name_from_file(&path);
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let outcome = if name.is_empty() {
            ImportOutcome::NeedsAttention { reason: "the file name does not contain a name".into() }
        } else {
            match inspect_reference_photo(engine, &path, settings) {
                Err(e) => ImportOutcome::Failed { error: e.to_string() },
                Ok(insp) if insp.faces.is_empty() => {
                    ImportOutcome::NeedsAttention { reason: "no face was found".into() }
                }
                Ok(insp) if insp.suggested.is_none() => ImportOutcome::NeedsAttention {
                    reason: format!("{} faces were found; add this photo by hand and pick one", insp.faces.len()),
                },
                Ok(insp) => {
                    let found = existing
                        .iter()
                        .find(|p| p.name.eq_ignore_ascii_case(&name) && p.group.eq_ignore_ascii_case(&group))
                        .map(|p| p.id)
                        .or_else(|| {
                            report.entries.iter().find_map(|e| match e.outcome {
                                ImportOutcome::Added { person_id, .. }
                                    if e.name.eq_ignore_ascii_case(&name) && e.group.eq_ignore_ascii_case(&group) =>
                                {
                                    Some(person_id)
                                }
                                _ => None,
                            })
                        });
                    let (pid, created) = match found {
                        Some(id) => (id, false),
                        None => (store.add_person(&name, &group, "")?, true),
                    };
                    match add_reference_photo(engine, store, pid, &path, insp.suggested, settings) {
                        Ok(_) => {
                            report.photos_added += 1;
                            if created {
                                report.people_created += 1;
                            }
                            ImportOutcome::Added { person_id: pid, created }
                        }
                        Err(e) => {
                            if created {
                                store.delete_person(pid)?;
                            }
                            ImportOutcome::Failed { error: e.to_string() }
                        }
                    }
                }
            }
        };
        report.entries.push(ImportEntry { file, path, name, group: group.clone(), outcome });
    }
    on_progress(total, total);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_files() {
        assert_eq!(name_from_file(Path::new("Ana_Perez.jpg")), "Ana Perez");
        assert_eq!(name_from_file(Path::new("Ana Perez (2).JPG")), "Ana Perez");
        assert_eq!(name_from_file(Path::new("ben-okafor-2.png")), "ben-okafor");
        assert_eq!(name_from_file(Path::new("Chloé  Martin 3.jpeg")), "Chloé Martin");
        assert_eq!(name_from_file(Path::new("R2D2.jpg")), "R2D2");
        assert_eq!(name_from_file(Path::new("O.Brien.jpg")), "O Brien");
    }
}
