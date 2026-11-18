use super::*;
use crate::geometry::Point;
use crate::matching::{Classification, Decision, FaceCategory, Pile};

fn reference() -> NewReference {
    NewReference {
        source_name: "ana.jpg".into(),
        crop_jpeg: vec![1, 2, 3],
        crop_landmarks: [Point::new(1.0, 2.0); 5],
        det_score: 0.9,
        face_px: 120,
    }
}

fn face(category: FaceCategory, pile: Pile, decision: Decision, person: Option<i64>) -> NewFace {
    NewFace {
        bbox: BBox::new(10.0, 10.0, 50.0, 60.0),
        landmarks: Some([Point::new(20.0, 30.0); 5]),
        det_score: Some(0.9),
        classification: Classification { category, pile, person_id: person, similarity: Some(0.5), decision },
        embedding: Some(vec![0.6, 0.8]),
        crop_jpeg: Some(vec![9]),
    }
}

fn photo(name: &str, faces: Vec<NewFace>) -> NewPhoto {
    NewPhoto {
        rel_path: name.into(),
        width: 100,
        height: 80,
        status: PhotoStatus::Ok,
        error: None,
        thumb_jpeg: Some(vec![7]),
        file_size: 1234,
        modified: 42,
        faces,
    }
}

#[test]
fn roster_crud_and_embeddings() {
    let s = Store::open_in_memory().unwrap();
    assert!(s.add_person("  ", "", "").is_err());
    let ana = s.add_person("Ana Pérez", "Cabin 4", "").unwrap();
    let ben = s.add_person("Ben Okafor", "Cabin 2", "allergies").unwrap();
    let r1 = s.add_reference(ana, &reference()).unwrap();
    let _r2 = s.add_reference(ben, &reference()).unwrap();
    assert!(s.add_reference(999, &reference()).is_err());

    let people = s.list_people().unwrap();
    assert_eq!(people.len(), 2);
    // Sorted by group then name.
    assert_eq!(people[0].name, "Ben Okafor");
    assert_eq!(people[1].references.len(), 1);
    assert_eq!(s.find_person_by_name("ana pérez").unwrap(), Some(ana));

    assert_eq!(s.references_missing_embedding("sface").unwrap().len(), 2);
    s.put_reference_embedding(r1, "sface", &[0.25, -1.5]).unwrap();
    assert_eq!(s.references_missing_embedding("sface").unwrap().len(), 1);
    assert_eq!(s.roster_embeddings("sface").unwrap(), vec![(ana, vec![0.25, -1.5])]);
    assert!(s.roster_embeddings("arcface").unwrap().is_empty());

    s.update_person(ana, "Ana P.", "Cabin 5", "").unwrap();
    s.delete_person(ana).unwrap();
    assert!(s.roster_embeddings("sface").unwrap().is_empty());
    assert_eq!(s.person_count().unwrap(), 1);
}

#[test]
fn scan_counts_review_and_decisions() {
    let s = Store::open_in_memory().unwrap();
    let ana = s.add_person("Ana", "", "").unwrap();
    s.add_reference(ana, &reference()).unwrap();
    let scan = s.create_scan("Field day", "/photos", "yunet", "sface", "{}").unwrap();
    s.insert_photo(
        scan,
        &photo(
            "a.jpg",
            vec![
                face(FaceCategory::Match, Pile::Matches, Decision::Blur, Some(ana)),
                face(FaceCategory::None, Pile::None, Decision::Keep, None),
            ],
        ),
    )
    .unwrap();
    let p2 = s
        .insert_photo(
            scan,
            &photo("b.jpg", vec![face(FaceCategory::Uncertain, Pile::Check, Decision::Pending, Some(ana))]),
        )
        .unwrap();
    s.insert_photo(scan, &NewPhoto { status: PhotoStatus::Unsupported, thumb_jpeg: None, ..photo("c.heic", vec![]) })
        .unwrap();
    s.finish_scan(scan, ScanStatus::Complete).unwrap();

    let sum = s.scan(scan).unwrap();
    assert_eq!(sum.status, ScanStatus::Complete);
    assert_eq!(sum.roster_size, 1);
    let c = &sum.counts;
    assert_eq!((c.photos, c.photos_ok, c.unsupported), (3, 2, 1));
    assert_eq!((c.faces, c.matches, c.check, c.check_pending), (3, 1, 1, 1));
    assert_eq!((c.to_blur, c.pending, c.photos_held), (1, 1, 1));

    let matches = s.review_items(scan, Pile::Matches).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].person.as_ref().unwrap().name, "Ana");
    assert_eq!(matches[0].person.as_ref().unwrap().reference_ids.len(), 1);
    assert_eq!(matches[0].photo.rel_path, "a.jpg");

    let check = s.review_items(scan, Pile::Check).unwrap();
    let f = s.set_decision(check[0].face.id, Decision::Keep).unwrap();
    assert!(f.reviewed);
    let undone = s.restore_decision(check[0].face.id, Decision::Pending, false).unwrap();
    assert_eq!((undone.decision, undone.reviewed), (Decision::Pending, false));
    s.set_decision(check[0].face.id, Decision::Keep).unwrap();
    assert_eq!(s.scan(scan).unwrap().counts.photos_held, 0);

    // Embeddings carry the scan's model key.
    assert_eq!(s.face_embeddings(scan, "sface").unwrap().len(), 3);
    assert!(s.face_embeddings(scan, "arcface").unwrap().is_empty());

    // Manual regions.
    let m = s.add_manual_face(p2, BBox::new(1.0, 1.0, 5.0, 5.0), None).unwrap();
    assert_eq!(s.scan(scan).unwrap().counts.manual, 1);
    assert!(s.delete_manual_face(check[0].face.id).is_err());
    s.delete_manual_face(m).unwrap();

    assert_eq!(s.bulk_decision(scan, Pile::Matches, false, Decision::Keep).unwrap(), 1);
    assert_eq!(s.export_photos(scan).unwrap().len(), 3);

    s.delete_scan(scan).unwrap();
    assert!(s.scan(scan).is_err());
    assert!(s.list_scans().unwrap().is_empty());
}

#[test]
fn settings_roundtrip_and_validation() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(s.settings().unwrap(), Settings::default());
    let mut custom = Settings { jpeg_quality: 80, ..Default::default() };
    s.save_settings(&custom).unwrap();
    assert_eq!(s.settings().unwrap().jpeg_quality, 80);
    custom.jpeg_quality = 5;
    assert!(s.save_settings(&custom).is_err());
}

#[test]
fn reopening_a_file_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    {
        let s = Store::open(&path).unwrap();
        s.add_person("Ana", "", "").unwrap();
        s.create_scan("x", "/p", "d", "e", "{}").unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.person_count().unwrap(), 1);
    assert_eq!(s.recover_interrupted_scans().unwrap(), 1);
    assert_eq!(s.list_scans().unwrap()[0].status, ScanStatus::Failed);
}
