//! Scanning an event folder and (re-)matching faces against the roster.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::embed::l2_normalize;
use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::imageio::{classify_path, encode_jpeg, fit_within, load_image, FileKind};
use crate::matching::{classify, Decision, FaceCategory, MatchConfig, Pile, RosterIndex, MIN_EMBED_PX};
use crate::roster::{display_crop, ensure_roster_embeddings};
use crate::settings::Settings;
use crate::store::{NewFace, NewPhoto, PhotoStatus, ScanStatus, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Preparing,
    Scanning,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: i64,
    pub phase: ScanPhase,
    pub total: u32,
    pub done: u32,
    pub faces: u32,
    pub matches: u32,
    pub check: u32,
    pub current: Option<String>,
}

/// Snapshot of the settings a scan ran with, stored alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanConfig {
    pub settings: Settings,
    pub review_threshold: f32,
    pub match_threshold: f32,
}

/// Files found in an event folder.
#[derive(Debug, Default)]
pub struct FolderListing {
    pub supported: Vec<PathBuf>,
    pub unsupported: Vec<PathBuf>,
    pub other: usize,
}

/// Recursively list photos, skipping hidden files and system folders.
pub fn list_folder(dir: &Path) -> Result<FolderListing> {
    if !dir.is_dir() {
        return Err(Error::NotFound(format!("{} is not a folder", dir.display())));
    }
    let mut out = FolderListing::default();
    let skip = |name: &str| name.starts_with('.') || name == "@eaDir" || name == "__MACOSX" || name == "Thumbs.db";
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !skip(&e.file_name().to_string_lossy()))
    {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        match classify_path(entry.path()) {
            FileKind::Supported => out.supported.push(entry.into_path()),
            FileKind::UnsupportedMedia => out.unsupported.push(entry.into_path()),
            FileKind::Other => out.other += 1,
        }
    }
    Ok(out)
}

/// Path relative to the event folder with `/` separators.
pub fn rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

pub(crate) fn file_stamp(path: &Path) -> (u64, i64) {
    match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            (m.len(), modified)
        }
        Err(_) => (0, 0),
    }
}

fn process_photo(
    engine: &Engine,
    path: &Path,
    rel: String,
    roster: &RosterIndex,
    settings: &Settings,
    mc: &MatchConfig,
) -> NewPhoto {
    let (file_size, modified) = file_stamp(path);
    let loaded = match load_image(path) {
        Ok(l) => l,
        Err(e) => {
            return NewPhoto {
                rel_path: rel,
                width: 0,
                height: 0,
                status: PhotoStatus::Error,
                error: Some(e.to_string()),
                thumb_jpeg: None,
                file_size,
                modified,
                faces: Vec::new(),
            }
        }
    };
    let img = &loaded.rgb;
    let result = (|| -> Result<Vec<NewFace>> {
        let dets = engine.detect(img, &settings.detect_options())?;
        let mut faces = Vec::with_capacity(dets.len());
        for d in dets {
            let px = d.bbox.short_side();
            let embedding =
                if px >= MIN_EMBED_PX { Some(l2_normalize(engine.embed(img, &d.landmarks)?)) } else { None };
            let classification = classify(px, embedding.as_deref(), roster, mc);
            faces.push(NewFace {
                bbox: d.bbox,
                landmarks: Some(d.landmarks),
                det_score: Some(d.score),
                classification,
                embedding,
                crop_jpeg: Some(display_crop(img, &d.bbox)?),
            });
        }
        Ok(faces)
    })();
    let thumb = encode_jpeg(&fit_within(img, 480), 80).ok();
    match result {
        Ok(faces) => NewPhoto {
            rel_path: rel,
            width: img.width(),
            height: img.height(),
            status: PhotoStatus::Ok,
            error: None,
            thumb_jpeg: thumb,
            file_size,
            modified,
            faces,
        },
        Err(e) => NewPhoto {
            rel_path: rel,
            width: img.width(),
            height: img.height(),
            status: PhotoStatus::Error,
            error: Some(e.to_string()),
            thumb_jpeg: thumb,
            file_size,
            modified,
            faces: Vec::new(),
        },
    }
}

/// Scan an event folder. Returns the new scan's id. Progress is reported
/// after every photo; setting `cancel` stops the scan early (it is then
/// marked cancelled and cannot be exported).
pub fn run_scan(
    engine: &Engine,
    store: &Store,
    name: &str,
    input_dir: &Path,
    settings: &Settings,
    on_progress: &(dyn Fn(&ScanProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<i64> {
    let input_dir = input_dir.canonicalize().map_err(|e| Error::io(input_dir, e))?;
    ensure_roster_embeddings(engine, store)?;
    let roster = RosterIndex::from_pairs(store.roster_embeddings(engine.embedding_key())?);
    let mc = settings.match_config(engine.default_thresholds());
    let config = ScanConfig {
        settings: settings.clone(),
        review_threshold: mc.thresholds.review,
        match_threshold: mc.thresholds.matched,
    };
    let scan_id = store.create_scan(
        name,
        &input_dir.to_string_lossy(),
        &engine.detector_info.id,
        engine.embedding_key(),
        &serde_json::to_string(&config)?,
    )?;

    let mut progress = ScanProgress {
        scan_id,
        phase: ScanPhase::Preparing,
        total: 0,
        done: 0,
        faces: 0,
        matches: 0,
        check: 0,
        current: None,
    };
    on_progress(&progress);

    let outcome = (|| -> Result<()> {
        let listing = list_folder(&input_dir)?;
        for path in &listing.unsupported {
            let (file_size, modified) = file_stamp(path);
            store.insert_photo(
                scan_id,
                &NewPhoto {
                    rel_path: rel_path(&input_dir, path),
                    width: 0,
                    height: 0,
                    status: PhotoStatus::Unsupported,
                    error: Some("this file type cannot be checked by Blurlist".into()),
                    thumb_jpeg: None,
                    file_size,
                    modified,
                    faces: Vec::new(),
                },
            )?;
        }
        progress.phase = ScanPhase::Scanning;
        progress.total = listing.supported.len() as u32;
        on_progress(&progress);

        let done = AtomicU32::new(0);
        let faces = AtomicU32::new(0);
        let matches = AtomicU32::new(0);
        let check = AtomicU32::new(0);
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 4);
        let pool =
            rayon::ThreadPoolBuilder::new().num_threads(workers).build().map_err(|e| Error::Invalid(e.to_string()))?;
        let base = progress.clone();
        pool.install(|| {
            listing.supported.par_iter().try_for_each(|path| -> Result<()> {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Cancelled);
                }
                let rel = rel_path(&input_dir, path);
                let photo = process_photo(engine, path, rel.clone(), &roster, settings, &mc);
                let n_faces = photo.faces.len() as u32;
                let n_match = photo.faces.iter().filter(|f| f.classification.pile == Pile::Matches).count() as u32;
                let n_check = photo.faces.iter().filter(|f| f.classification.pile == Pile::Check).count() as u32;
                store.insert_photo(scan_id, &photo)?;
                let mut p = base.clone();
                p.done = done.fetch_add(1, Ordering::Relaxed) + 1;
                p.faces = faces.fetch_add(n_faces, Ordering::Relaxed) + n_faces;
                p.matches = matches.fetch_add(n_match, Ordering::Relaxed) + n_match;
                p.check = check.fetch_add(n_check, Ordering::Relaxed) + n_check;
                p.current = Some(rel);
                on_progress(&p);
                Ok(())
            })
        })?;
        progress.done = done.load(Ordering::Relaxed);
        progress.faces = faces.load(Ordering::Relaxed);
        progress.matches = matches.load(Ordering::Relaxed);
        progress.check = check.load(Ordering::Relaxed);
        Ok(())
    })();

    let status = match &outcome {
        Ok(()) => ScanStatus::Complete,
        Err(Error::Cancelled) => ScanStatus::Cancelled,
        Err(_) => ScanStatus::Failed,
    };
    store.finish_scan(scan_id, status)?;
    progress.phase = ScanPhase::Finished;
    progress.current = None;
    on_progress(&progress);
    match outcome {
        Ok(()) | Err(Error::Cancelled) => Ok(scan_id),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RematchReport {
    pub faces_checked: usize,
    pub changed: usize,
}

/// Re-run matching for a finished scan using its stored face embeddings, for
/// example after people were added to the roster or thresholds changed.
///
/// Decisions are kept where the outcome did not change, and faces a person
/// already confirmed for blurring stay blurred. Any other face whose match
/// changed goes back to its default decision so it is reviewed again.
pub fn rematch_scan(engine: &Engine, store: &Store, scan_id: i64, settings: &Settings) -> Result<RematchReport> {
    let scan = store.scan(scan_id)?;
    if scan.embedder_id != engine.embedding_key() {
        return Err(Error::Invalid(format!(
            "this event was scanned with {}; scan it again to use {}",
            scan.embedder_id,
            engine.embedding_key()
        )));
    }
    ensure_roster_embeddings(engine, store)?;
    let roster = RosterIndex::from_pairs(store.roster_embeddings(engine.embedding_key())?);
    let mc = settings.match_config(engine.default_thresholds());
    let mut report = RematchReport::default();

    let with_emb = store.face_embeddings(scan_id, engine.embedding_key())?;
    let without = store.faces_without_embedding(scan_id)?;
    let all = with_emb.into_iter().map(|(f, e)| (f, Some(e))).chain(without.into_iter().map(|f| (f, None)));
    for (face, emb) in all {
        if face.category == FaceCategory::Manual {
            continue;
        }
        report.faces_checked += 1;
        let c = classify(face.bbox.short_side(), emb.as_deref(), &roster, &mc);
        let same = c.category == face.category && c.person_id == face.person_id && c.pile == face.pile;
        let (decision, reviewed) = if same {
            (face.decision, face.reviewed)
        } else if face.reviewed && face.decision == Decision::Blur {
            (Decision::Blur, true)
        } else {
            report.changed += 1;
            (c.decision, false)
        };
        store.update_classification(face.id, &c, decision, reviewed)?;
    }
    store.update_roster_size(scan_id)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_paths_use_forward_slashes() {
        let root = Path::new("/events/day1");
        assert_eq!(rel_path(root, &root.join("a").join("b.jpg")), "a/b.jpg");
    }

    #[test]
    fn listing_skips_hidden_and_reports_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::create_dir_all(d.join(".hidden")).unwrap();
        for f in ["a.jpg", "sub/b.PNG", "c.heic", "notes.txt", ".DS_Store", ".hidden/x.jpg", "clip.mov"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let l = list_folder(d).unwrap();
        assert_eq!(l.supported.len(), 2);
        assert_eq!(l.unsupported.len(), 2);
        assert_eq!(l.other, 1);
    }
}
