//! End-to-end tests with the real models.
//!
//! The models are not committed; fetch them with `node scripts/fetch-models.mjs`.
//! Without them these tests print a notice and pass, unless
//! `BLURLIST_REQUIRE_MODELS=1` is set (as it is in CI).
//!
//! Expected values were produced with OpenCV 5.1's reference
//! `FaceDetectorYN` / `FaceRecognizerSF` on the same fixtures.
//! Fixtures are official White House portraits (U.S. federal government works,
//! public domain), downscaled with metadata removed; see fixtures/README.md.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use blurlist_core::detect::{DetectOptions, Thoroughness};
use blurlist_core::embed::cosine;
use blurlist_core::export::{export_scan, ExportOptions};
use blurlist_core::geometry::BBox;
use blurlist_core::matching::{Decision, FaceCategory, Pile};
use blurlist_core::models::{discover_models, Thresholds};
use blurlist_core::roster::{add_reference_photo, inspect_reference_photo};
use blurlist_core::scan::{rematch_scan, run_scan};
use blurlist_core::store::{ScanStatus, Store};
use blurlist_core::{Engine, Settings};
use image::{Rgb, RgbImage};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn model_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("BLURLIST_MODELS") {
        v.push(PathBuf::from(d));
    }
    v.push(root().join("models"));
    v.push(root().join("models/research"));
    v
}

fn engine_for(det_id: &str, emb_id: &str) -> Option<Engine> {
    let models = discover_models(&model_dirs());
    let det = models.iter().find(|m| m.id == det_id && m.path.is_some());
    let emb = models.iter().find(|m| m.id == emb_id && m.path.is_some());
    match (det, emb) {
        (Some(d), Some(e)) => Some(Engine::load(d, e, 2).expect("models should load")),
        _ => None,
    }
}

fn default_engine() -> Option<Engine> {
    let e = engine_for("yunet-2023mar", "sface-2021dec");
    if e.is_none() {
        if std::env::var("BLURLIST_REQUIRE_MODELS").is_ok_and(|v| v == "1") {
            panic!("models are required but missing; run `node scripts/fetch-models.mjs`");
        }
        eprintln!("SKIPPED: bundled models not found; run `node scripts/fetch-models.mjs`");
    }
    e
}

fn load(path: &Path) -> RgbImage {
    blurlist_core::imageio::load_image(path).unwrap().rgb
}

fn assert_close(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} ± {tol}");
}

/// Mean absolute difference over a box.
fn mad(a: &RgbImage, b: &RgbImage, bx: &BBox) -> f32 {
    let (x, y, w, h) = bx.to_pixel_rect(a.width(), a.height()).unwrap();
    let mut sum = 0u64;
    for yy in y..y + h {
        for xx in x..x + w {
            let p = a.get_pixel(xx, yy).0;
            let q = b.get_pixel(xx, yy).0;
            for c in 0..3 {
                sum += (p[c] as i32 - q[c] as i32).unsigned_abs() as u64;
            }
        }
    }
    sum as f32 / (w * h * 3) as f32
}

/// Insert an EXIF APP1 segment with the given orientation right after SOI.
fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut app1 = vec![0xFF, 0xE1, 0x00, 0x22];
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&[b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
    app1.extend_from_slice(&[0x01, 0x00]);
    app1.extend_from_slice(&[0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00]);
    app1.extend_from_slice(&orientation.to_le_bytes());
    app1.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(app1.len(), 2 + 0x22);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn yunet_matches_opencv_reference() {
    let Some(engine) = default_engine() else { return };
    let img = load(&fixture("reference-a.jpg"));
    let opts = DetectOptions { thoroughness: Thoroughness::Fast, ..Default::default() };
    let faces = engine.detect(&img, &opts).unwrap();
    assert_eq!(faces.len(), 1, "{faces:?}");
    let f = &faces[0];
    // OpenCV: [260.7, 63.3, 169.9, 231.7], eyes (308.6,150.2) (390.1,150.1)
    let expected = BBox::new(260.7, 63.3, 169.9, 231.7);
    assert!(f.bbox.iou(&expected) > 0.9, "box {:?}", f.bbox);
    assert_close(f.landmarks[0].x, 308.6, 4.0, "left eye x");
    assert_close(f.landmarks[0].y, 150.2, 4.0, "left eye y");
    assert_close(f.landmarks[1].x, 390.1, 4.0, "right eye x");
    assert_close(f.landmarks[4].y, 224.7, 4.0, "mouth y");
    assert!(f.score > 0.85);
}

#[test]
fn sface_similarities_match_opencv_reference() {
    let Some(engine) = default_engine() else { return };
    let opts = DetectOptions::default();
    let emb = |name: &str| {
        let img = load(&fixture(name));
        let faces = engine.detect(&img, &opts).unwrap();
        assert_eq!(faces.len(), 1, "{name}");
        engine.embed(&img, &faces[0].landmarks).unwrap()
    };
    let r = emb("reference-a.jpg");
    let a = emb("event-a.jpg");
    let b = emb("event-b.jpg");
    assert_eq!(r.len(), 128);
    assert_close(cosine(&r, &r), 1.0, 1e-4, "self");
    assert_close(cosine(&r, &a), 0.748, 0.04, "same person");
    assert_close(cosine(&r, &b), 0.127, 0.04, "different people");
    assert_close(cosine(&a, &b), 0.273, 0.04, "different people (2)");
}

#[test]
fn research_models_work_when_present() {
    // InsightFace SCRFD + ArcFace weights are research-only and never shipped;
    // this runs only on machines that have them in models/research.
    let Some(engine) = engine_for("scrfd-500m", "arcface-w600k-mbf") else {
        eprintln!("SKIPPED: research models not present");
        return;
    };
    let opts = DetectOptions::default();
    let emb = |name: &str| {
        let img = load(&fixture(name));
        let faces = engine.detect(&img, &opts).unwrap();
        assert_eq!(faces.len(), 1, "{name}: {faces:?}");
        engine.embed(&img, &faces[0].landmarks).unwrap()
    };
    let r = emb("reference-a.jpg");
    let a = emb("event-a.jpg");
    let b = emb("event-b.jpg");
    assert_eq!(r.len(), 512);
    assert!(cosine(&r, &a) > 0.45, "same: {}", cosine(&r, &a));
    assert!(cosine(&r, &b) < 0.2, "different: {}", cosine(&r, &b));
}

#[test]
fn tiling_finds_small_faces_in_large_photos() {
    let Some(engine) = default_engine() else { return };
    let face = load(&fixture("event-a.jpg"));
    let small = image::imageops::resize(&face, 84, 160, image::imageops::FilterType::Triangle);
    let mut big = RgbImage::from_pixel(4000, 3000, Rgb([96, 120, 96]));
    image::imageops::overlay(&mut big, &small, 2900, 2100);
    let fast = engine.detect(&big, &DetectOptions { thoroughness: Thoroughness::Fast, ..Default::default() }).unwrap();
    let thorough = engine.detect(&big, &DetectOptions::default()).unwrap();
    eprintln!("fast: {fast:?}\nthorough: {thorough:?}");
    assert_eq!(thorough.len(), 1, "{thorough:?}");
    let b = thorough[0].bbox;
    assert!(b.x > 2900.0 && b.x2() < 2984.0 && b.y > 2100.0 && b.y2() < 2260.0, "{b:?}");
}

#[test]
fn full_pipeline_scan_review_export() {
    let Some(engine) = default_engine() else { return };
    let store = Store::open_in_memory().unwrap();
    let mut settings = Settings::default();

    // Roster: one person, one reference photo.
    let insp = inspect_reference_photo(&engine, &fixture("reference-a.jpg"), &settings).unwrap();
    assert_eq!(insp.faces.len(), 1);
    assert_eq!(insp.suggested, Some(0));
    let ana = store.add_person("Ana Example", "Cabin 4", "").unwrap();
    let rid = add_reference_photo(&engine, &store, ana, &fixture("reference-a.jpg"), None, &settings).unwrap();
    let stored = store.reference(rid).unwrap();
    let view = blurlist_core::roster::reference_face_view(&stored.crop_jpeg, &stored.crop_landmarks).unwrap();
    let view = image::load_from_memory(&view).unwrap();
    assert!(view.width() > 100 && (view.width() as i64 - view.height() as i64).abs() <= 2, "{view:?}");

    // Event folder.
    let tmp = tempfile::tempdir().unwrap();
    let event = tmp.path().join("Field Day");
    std::fs::create_dir_all(event.join("day1")).unwrap();
    std::fs::copy(fixture("event-a.jpg"), event.join("day1/portrait.jpg")).unwrap();
    std::fs::copy(fixture("event-b.jpg"), event.join("day1/stranger.jpg")).unwrap();
    let a = load(&fixture("event-a.jpg"));
    let b = load(&fixture("event-b.jpg"));
    let mut group = RgbImage::from_pixel(1000, 900, Rgb([150, 150, 160]));
    image::imageops::overlay(&mut group, &a, 40, 50);
    image::imageops::overlay(&mut group, &b, 560, 50);
    group.save(event.join("group.png")).unwrap();
    // Sideways pixels + EXIF orientation 6 ("rotate 90° clockwise to view").
    let sideways = image::imageops::rotate270(&a);
    let jpeg = blurlist_core::imageio::encode_jpeg(&sideways, 92).unwrap();
    std::fs::write(event.join("rotated.jpg"), with_exif_orientation(&jpeg, 6)).unwrap();
    std::fs::write(event.join("clip.heic"), b"not really heic").unwrap();
    std::fs::write(event.join("notes.txt"), b"hello").unwrap();
    std::fs::write(event.join("broken.jpg"), b"not a jpeg").unwrap();

    let cancel = AtomicBool::new(false);
    let scan_id = run_scan(&engine, &store, "Field Day", &event, &settings, &|_| {}, &cancel).unwrap();
    let scan = store.scan(scan_id).unwrap();
    assert_eq!(scan.status, ScanStatus::Complete);
    let c = &scan.counts;
    assert_eq!((c.photos, c.photos_ok, c.unsupported, c.errors), (6, 4, 1, 1), "{c:?}");
    assert_eq!(c.faces, 5, "{c:?}");
    assert_eq!(c.matches, 3, "{c:?}");
    assert_eq!(c.check, 0, "{c:?}");

    let matches = store.review_items(scan_id, Pile::Matches).unwrap();
    let mut paths: Vec<_> = matches.iter().map(|m| m.photo.rel_path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["day1/portrait.jpg", "group.png", "rotated.jpg"]);
    assert!(matches.iter().all(|m| m.person.as_ref().unwrap().id == ana));
    assert!(matches.iter().all(|m| m.face.decision == Decision::Blur && !m.face.reviewed));
    // The rotated photo was handled upright.
    let rotated = matches.iter().find(|m| m.photo.rel_path == "rotated.jpg").unwrap();
    assert_eq!((rotated.photo.width, rotated.photo.height), (a.width(), a.height()));

    // Confirm one match with the keyboard flow.
    store.set_decision(matches[0].face.id, Decision::Blur).unwrap();

    // Export.
    let out_parent = tmp.path().join("exports");
    std::fs::create_dir_all(&out_parent).unwrap();
    let opts = ExportOptions { destination: out_parent.clone(), blur_style: settings.blur_style, jpeg_quality: 92 };
    let report = export_scan(&store, scan_id, &opts, &|_| {}, &cancel).unwrap();
    assert_eq!(report.exported, 4, "{report:?}");
    assert_eq!(report.faces_blurred, 3);
    assert_eq!(report.photos_blurred, 3);
    assert!(report.held_back.is_empty());
    assert_eq!(report.not_supported, vec!["clip.heic".to_string()]);
    assert_eq!(report.failed.len(), 1);
    // Compare canonical paths: on macOS the temp dir is behind a /private symlink.
    assert!(report.output_dir.starts_with(out_parent.canonicalize().unwrap()));
    assert!(!out_parent.join("names.txt").exists());

    let out = |rel: &str| report.output_dir.join(rel);
    for rel in ["day1/portrait.jpg", "day1/stranger.jpg", "rotated.jpg"] {
        let bytes = std::fs::read(out(rel)).unwrap();
        assert!(!bytes.windows(4).any(|w| w == b"Exif"), "{rel} still has EXIF");
    }
    assert!(!out("clip.heic").exists() && !out("notes.txt").exists() && !out("broken.jpg").exists());

    // Blurred where it should be, untouched elsewhere.
    let portrait_out = load(&out("day1/portrait.jpg"));
    let face_box = matches.iter().find(|m| m.photo.rel_path == "day1/portrait.jpg").unwrap().face.bbox;
    assert!(mad(&a, &portrait_out, &face_box) > 12.0, "face not blurred");
    assert!(mad(&a, &portrait_out, &BBox::new(0.0, 740.0, 417.0, 60.0)) < 4.0, "background changed");
    let stranger_out = load(&out("day1/stranger.jpg"));
    assert!(mad(&b, &stranger_out, &BBox::new(155.0, 64.0, 104.0, 145.0)) < 4.0, "stranger was blurred");
    let rotated_out = load(&out("rotated.jpg"));
    assert_eq!(rotated_out.dimensions(), a.dimensions(), "orientation not baked in");

    // Lowering the review threshold sends the strangers to "Check these";
    // their photos are held back until someone decides.
    settings.thresholds = Some(Thresholds { review: 0.05, matched: 0.42 });
    let rm = rematch_scan(&engine, &store, scan_id, &settings).unwrap();
    assert_eq!(rm.changed, 2, "{rm:?}");
    let check = store.review_items(scan_id, Pile::Check).unwrap();
    assert_eq!(check.len(), 2);
    assert!(check.iter().all(|i| i.face.category == FaceCategory::Uncertain && i.face.decision == Decision::Pending));
    let report = export_scan(&store, scan_id, &opts, &|_| {}, &cancel).unwrap();
    let mut held: Vec<_> = report.held_back.iter().map(|h| h.rel_path.as_str()).collect();
    held.sort();
    assert_eq!(held, ["day1/stranger.jpg", "group.png"]);
    assert_eq!(report.exported, 2);

    // Resolving them releases the photos.
    for item in &check {
        store.set_decision(item.face.id, Decision::Keep).unwrap();
    }
    let report = export_scan(&store, scan_id, &opts, &|_| {}, &cancel).unwrap();
    assert!(report.held_back.is_empty());
    assert_eq!(report.exported, 4);

    // Adding the stranger to the roster turns them into a match on re-match,
    // even though they were previously kept.
    settings.thresholds = None;
    let ben = store.add_person("Ben Example", "", "").unwrap();
    add_reference_photo(&engine, &store, ben, &fixture("event-b.jpg"), None, &settings).unwrap();
    rematch_scan(&engine, &store, scan_id, &settings).unwrap();
    let matches = store.review_items(scan_id, Pile::Matches).unwrap();
    assert_eq!(matches.len(), 5);
    assert_eq!(matches.iter().filter(|m| m.person.as_ref().unwrap().id == ben).count(), 2);
    assert_eq!(store.scan(scan_id).unwrap().roster_size, 2);

    // Changing a file after the scan holds it back.
    std::fs::write(event.join("day1/portrait.jpg"), std::fs::read(fixture("event-b.jpg")).unwrap()).unwrap();
    let report = export_scan(&store, scan_id, &opts, &|_| {}, &cancel).unwrap();
    assert!(report.held_back.iter().any(|h| h.rel_path == "day1/portrait.jpg"), "{report:?}");

    // Exporting into the event folder is refused.
    let inside = ExportOptions { destination: event.clone(), ..opts.clone() };
    assert!(export_scan(&store, scan_id, &inside, &|_| {}, &cancel).is_err());
}

#[test]
fn reference_photo_with_several_faces_needs_a_choice() {
    let Some(engine) = default_engine() else { return };
    let settings = Settings::default();
    let a = load(&fixture("event-a.jpg"));
    let b = load(&fixture("event-b.jpg"));
    let mut two = RgbImage::from_pixel(900, 820, Rgb([200, 200, 200]));
    image::imageops::overlay(&mut two, &a, 0, 10);
    image::imageops::overlay(&mut two, &b, 500, 10);
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("two.png");
    two.save(&path).unwrap();

    let insp = inspect_reference_photo(&engine, &path, &settings).unwrap();
    assert_eq!(insp.faces.len(), 2);
    assert_eq!(insp.suggested, None);

    let store = Store::open_in_memory().unwrap();
    let p = store.add_person("Someone", "", "").unwrap();
    assert!(add_reference_photo(&engine, &store, p, &path, None, &settings).is_err());
    add_reference_photo(&engine, &store, p, &path, Some(1), &settings).unwrap();
    assert_eq!(store.list_people().unwrap()[0].references.len(), 1);
}
