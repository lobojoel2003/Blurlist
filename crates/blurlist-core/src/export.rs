//! Writing the upload-ready copy of an event folder.
//!
//! * Originals are never modified; output goes to a new folder.
//! * Photos with any face still waiting for a decision are held back.
//! * Photos that changed on disk since they were scanned are held back.
//! * Every exported file is re-encoded from upright pixels with no EXIF, GPS,
//!   XMP, IPTC, maker notes or embedded thumbnails (which could otherwise
//!   still show the unblurred face). Only the ICC colour profile is kept.
//! * Nothing identifying (such as a list of names) is written into the
//!   export folder.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use image::{ImageEncoder, ImageFormat, RgbImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::blur::{anonymize, BlurStyle};
use crate::error::{Error, Result};
use crate::imageio::load_image;
use crate::matching::Decision;
use crate::scan::file_stamp;
use crate::store::{PhotoStatus, ScanStatus, Store};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Folder in which the export folder is created.
    pub destination: PathBuf,
    pub blur_style: BlurStyle,
    pub jpeg_quality: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPhoto {
    pub rel_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub output_dir: PathBuf,
    pub exported: usize,
    pub photos_blurred: usize,
    pub faces_blurred: usize,
    pub held_back: Vec<SkippedPhoto>,
    pub failed: Vec<SkippedPhoto>,
    pub not_supported: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub total: u32,
    pub done: u32,
    pub current: Option<String>,
}

/// Safe folder name derived from an event name.
pub fn export_folder_name(event: &str, date: &str) -> String {
    let cleaned: String =
        event.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { ' ' } else { c }).collect();
    let mut name = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    name = name.trim_matches('.').trim().to_string();
    if name.is_empty() {
        name = "Event".into();
    }
    if name.chars().count() > 60 {
        name = name.chars().take(60).collect::<String>().trim_end().to_string();
    }
    format!("{name} (blurred {date})")
}

fn today() -> String {
    let d = time::OffsetDateTime::now_utc().date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let mut p = parent.join(name);
    let mut n = 2;
    while p.exists() {
        p = parent.join(format!("{name} {n}"));
        n += 1;
    }
    p
}

/// Write `img` to `path` in `format`, with no metadata except the ICC profile.
pub fn write_clean(path: &Path, img: &RgbImage, format: ImageFormat, icc: Option<&[u8]>, quality: u8) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    let tmp = parent.join(format!(
        ".{}.blurlist-tmp",
        path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    ));
    let res = (|| -> Result<()> {
        let file = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        let mut w = BufWriter::new(file);
        let (wd, ht) = img.dimensions();
        let color = image::ExtendedColorType::Rgb8;
        let err = |e: image::ImageError| Error::Image(e.to_string());
        match format {
            ImageFormat::Png => {
                let mut enc = image::codecs::png::PngEncoder::new(&mut w);
                if let Some(p) = icc {
                    let _ = enc.set_icc_profile(p.to_vec());
                }
                enc.write_image(img.as_raw(), wd, ht, color).map_err(err)?;
            }
            ImageFormat::WebP => {
                let enc = image::codecs::webp::WebPEncoder::new_lossless(&mut w);
                enc.write_image(img.as_raw(), wd, ht, color).map_err(err)?;
            }
            ImageFormat::Tiff => {
                let mut buf = std::io::Cursor::new(Vec::new());
                image::codecs::tiff::TiffEncoder::new(&mut buf)
                    .write_image(img.as_raw(), wd, ht, color)
                    .map_err(err)?;
                w.write_all(buf.get_ref()).map_err(|e| Error::io(&tmp, e))?;
            }
            ImageFormat::Bmp => {
                image::codecs::bmp::BmpEncoder::new(&mut w).write_image(img.as_raw(), wd, ht, color).map_err(err)?;
            }
            _ => {
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, quality.clamp(50, 100));
                if let Some(p) = icc {
                    let _ = enc.set_icc_profile(p.to_vec());
                }
                enc.write_image(img.as_raw(), wd, ht, color).map_err(err)?;
            }
        }
        w.flush().map_err(|e| Error::io(&tmp, e))?;
        drop(w);
        std::fs::rename(&tmp, path).map_err(|e| Error::io(path, e))?;
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Export a finished scan. See the module docs for the guarantees.
pub fn export_scan(
    store: &Store,
    scan_id: i64,
    opts: &ExportOptions,
    on_progress: &(dyn Fn(&ExportProgress) + Sync),
    cancel: &AtomicBool,
) -> Result<ExportReport> {
    let started = Instant::now();
    let scan = store.scan(scan_id)?;
    if scan.status != ScanStatus::Complete {
        return Err(Error::Invalid(
            "this scan did not finish, so some photos were never checked; scan the folder again".into(),
        ));
    }
    let input = PathBuf::from(&scan.input_dir);
    let dest_parent = opts.destination.canonicalize().map_err(|e| Error::io(&opts.destination, e))?;
    if let Ok(input_c) = input.canonicalize() {
        if dest_parent.starts_with(&input_c) {
            return Err(Error::Invalid(
                "choose an export location outside the event folder, so originals and blurred copies never mix".into(),
            ));
        }
    }
    let out_dir = unique_dir(&dest_parent, &export_folder_name(&scan.name, &today()));
    std::fs::create_dir_all(&out_dir).map_err(|e| Error::io(&out_dir, e))?;

    let photos = store.export_photos(scan_id)?;
    let mut report = ExportReport { output_dir: out_dir.clone(), ..Default::default() };
    let mut work = Vec::new();
    for p in photos {
        match p.photo.status {
            PhotoStatus::Unsupported => report.not_supported.push(p.photo.rel_path.clone()),
            PhotoStatus::Error => report.failed.push(SkippedPhoto {
                rel_path: p.photo.rel_path.clone(),
                reason: p.photo.error.clone().unwrap_or_else(|| "could not be read".into()),
            }),
            PhotoStatus::Ok => {
                let pending = p.faces.iter().filter(|f| f.decision == Decision::Pending).count();
                if pending > 0 {
                    report.held_back.push(SkippedPhoto {
                        rel_path: p.photo.rel_path.clone(),
                        reason: format!(
                            "{pending} face{} still need{} a decision",
                            if pending == 1 { "" } else { "s" },
                            if pending == 1 { "s" } else { "" }
                        ),
                    });
                } else {
                    work.push(p);
                }
            }
        }
    }

    let total = work.len() as u32;
    let done = AtomicU32::new(0);
    on_progress(&ExportProgress { total, done: 0, current: None });

    enum Outcome {
        Written { faces: usize },
        Held(SkippedPhoto),
        Failed(SkippedPhoto),
    }

    let outcomes: Vec<Outcome> = work
        .par_iter()
        .map(|p| {
            if cancel.load(Ordering::Relaxed) {
                return Outcome::Failed(SkippedPhoto {
                    rel_path: p.photo.rel_path.clone(),
                    reason: "export was cancelled".into(),
                });
            }
            let rel = &p.photo.rel_path;
            let src = rel.split('/').fold(input.clone(), |acc, part| acc.join(part));
            let result = (|| -> std::result::Result<usize, Outcome> {
                let stamp = file_stamp(&src);
                if stamp != (p.photo.file_size, p.photo.modified) {
                    return Err(Outcome::Held(SkippedPhoto {
                        rel_path: rel.clone(),
                        reason: "the file changed after it was scanned; scan the folder again".into(),
                    }));
                }
                let fail = |e: Error| Outcome::Failed(SkippedPhoto { rel_path: rel.clone(), reason: e.to_string() });
                let mut loaded = load_image(&src).map_err(fail)?;
                if loaded.rgb.dimensions() != (p.photo.width, p.photo.height) {
                    return Err(Outcome::Held(SkippedPhoto {
                        rel_path: rel.clone(),
                        reason: "the photo's size changed after it was scanned; scan the folder again".into(),
                    }));
                }
                let boxes: Vec<_> = p.faces.iter().filter(|f| f.decision == Decision::Blur).map(|f| f.bbox).collect();
                anonymize(&mut loaded.rgb, &boxes, opts.blur_style);
                let dest = rel.split('/').fold(out_dir.clone(), |acc, part| acc.join(part));
                write_clean(&dest, &loaded.rgb, loaded.format, loaded.icc_profile.as_deref(), opts.jpeg_quality)
                    .map_err(fail)?;
                Ok(boxes.len())
            })();
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(&ExportProgress { total, done: n, current: Some(rel.clone()) });
            match result {
                Ok(faces) => Outcome::Written { faces },
                Err(o) => o,
            }
        })
        .collect();

    for o in outcomes {
        match o {
            Outcome::Written { faces } => {
                report.exported += 1;
                if faces > 0 {
                    report.photos_blurred += 1;
                    report.faces_blurred += faces;
                }
            }
            Outcome::Held(s) => report.held_back.push(s),
            Outcome::Failed(s) => report.failed.push(s),
        }
    }
    report.held_back.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    report.failed.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    report.duration_ms = started.elapsed().as_millis() as u64;
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_safe() {
        assert_eq!(export_folder_name("Field Day: 5/12", "2026-05-12"), "Field Day 5 12 (blurred 2026-05-12)");
        assert_eq!(export_folder_name("  ..  ", "d"), "Event (blurred d)");
        assert!(export_folder_name(&"x".repeat(200), "d").len() < 80);
    }

    #[test]
    fn clean_jpeg_has_no_metadata_and_keeps_icc() {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbImage::from_pixel(40, 30, image::Rgb([10, 200, 30]));
        let path = dir.path().join("sub/out.jpg");
        let icc = b"fake-icc-profile-bytes".to_vec();
        write_clean(&path, &img, ImageFormat::Jpeg, Some(&icc), 90).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(6).any(|w| w == b"Exif\0\0"));
        assert!(bytes.windows(11).any(|w| w == b"ICC_PROFILE"));
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (40, 30));
        // No temp file left behind.
        assert_eq!(std::fs::read_dir(dir.path().join("sub")).unwrap().count(), 1);
    }

    #[test]
    fn writes_every_supported_format() {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]));
        for (f, ext) in [
            (ImageFormat::Png, "png"),
            (ImageFormat::WebP, "webp"),
            (ImageFormat::Tiff, "tif"),
            (ImageFormat::Bmp, "bmp"),
            (ImageFormat::Jpeg, "jpg"),
        ] {
            let p = dir.path().join(format!("x.{ext}"));
            write_clean(&p, &img, f, None, 90).unwrap();
            let back = image::open(&p).unwrap();
            assert_eq!(back.width(), 8, "{ext}");
        }
    }
}
