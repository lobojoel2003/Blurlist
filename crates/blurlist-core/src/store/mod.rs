//! SQLite persistence for the roster, scans, detected faces and decisions.
//!
//! Everything lives in one local database file. Nothing is synchronised
//! anywhere; deleting a scan removes its thumbnails and face crops.

mod schema;

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::geometry::{BBox, Landmarks};
use crate::matching::{Classification, Decision, FaceCategory, Pile};
use crate::settings::Settings;

pub struct Store {
    conn: Mutex<Connection>,
}

fn now() -> String {
    time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|j| j.as_str().map(str::to_string)).unwrap_or_default()
}

fn parse_enum<T: for<'de> Deserialize<'de>>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn f32s_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn blob_to_f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

// ---------------------------------------------------------------- roster types

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: i64,
    pub name: String,
    pub group: String,
    pub notes: String,
    pub created_at: String,
    pub references: Vec<ReferenceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSummary {
    pub id: i64,
    pub person_id: i64,
    pub source_name: String,
    pub det_score: f32,
    pub face_px: u32,
    pub created_at: String,
}

/// A reference face about to be stored.
pub struct NewReference {
    pub source_name: String,
    /// Square crop around the face with margin (JPEG).
    pub crop_jpeg: Vec<u8>,
    /// Landmarks in crop coordinates, for re-embedding with another model.
    pub crop_landmarks: Landmarks,
    pub det_score: f32,
    pub face_px: u32,
}

pub struct StoredReference {
    pub id: i64,
    pub person_id: i64,
    pub crop_jpeg: Vec<u8>,
    pub crop_landmarks: Landmarks,
}

// ------------------------------------------------------------------ scan types

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    Running,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhotoStatus {
    Ok,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCounts {
    pub photos: u32,
    pub photos_ok: u32,
    pub unsupported: u32,
    pub errors: u32,
    pub faces: u32,
    pub matches: u32,
    pub matches_reviewed: u32,
    pub check: u32,
    pub check_pending: u32,
    pub manual: u32,
    pub to_blur: u32,
    pub pending: u32,
    /// Photos that would be held back on export right now.
    pub photos_held: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub id: i64,
    pub name: String,
    pub input_dir: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub status: ScanStatus,
    pub detector_id: String,
    pub embedder_id: String,
    pub roster_size: u32,
    pub counts: ScanCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoRecord {
    pub id: i64,
    pub scan_id: i64,
    pub rel_path: String,
    pub width: u32,
    pub height: u32,
    pub status: PhotoStatus,
    pub error: Option<String>,
    pub file_size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRecord {
    pub id: i64,
    pub photo_id: i64,
    pub bbox: BBox,
    pub landmarks: Option<Landmarks>,
    pub det_score: Option<f32>,
    pub category: FaceCategory,
    pub pile: Pile,
    pub person_id: Option<i64>,
    pub similarity: Option<f32>,
    pub decision: Decision,
    pub reviewed: bool,
}

/// A face about to be stored.
pub struct NewFace {
    pub bbox: BBox,
    pub landmarks: Option<Landmarks>,
    pub det_score: Option<f32>,
    pub classification: Classification,
    pub embedding: Option<Vec<f32>>,
    pub crop_jpeg: Option<Vec<u8>>,
}

pub struct NewPhoto {
    pub rel_path: String,
    pub width: u32,
    pub height: u32,
    pub status: PhotoStatus,
    pub error: Option<String>,
    pub thumb_jpeg: Option<Vec<u8>>,
    pub file_size: u64,
    pub modified: i64,
    pub faces: Vec<NewFace>,
}

/// One row of the review screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub face: FaceRecord,
    pub photo: PhotoRecord,
    pub person: Option<PersonBrief>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonBrief {
    pub id: i64,
    pub name: String,
    pub group: String,
    pub reference_ids: Vec<i64>,
}

/// Faces that would be exported for one photo.
pub struct ExportPhoto {
    pub photo: PhotoRecord,
    pub faces: Vec<FaceRecord>,
}

impl Store {
    /// Open (creating and migrating if needed) a database file.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// In-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    // ------------------------------------------------------------- settings

    pub fn settings(&self) -> Result<Settings> {
        let conn = self.conn.lock();
        let v: Option<String> =
            conn.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()?;
        Ok(v.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        s.validate().map_err(Error::Invalid)?;
        let json = serde_json::to_string(s)?;
        self.conn.lock().execute(
            "INSERT INTO settings(key, value) VALUES('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    // --------------------------------------------------------------- roster

    pub fn add_person(&self, name: &str, group: &str, notes: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a name is required".into()));
        }
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO people(name, group_name, notes, created_at) VALUES(?1, ?2, ?3, ?4)",
            params![name, group.trim(), notes.trim(), now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_person(&self, id: i64, name: &str, group: &str, notes: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("a name is required".into()));
        }
        let n = self.conn.lock().execute(
            "UPDATE people SET name = ?2, group_name = ?3, notes = ?4 WHERE id = ?1",
            params![id, name, group.trim(), notes.trim()],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("person {id} not found")));
        }
        Ok(())
    }

    pub fn delete_person(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM people WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn find_person_by_name(&self, name: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .lock()
            .query_row(
                "SELECT id FROM people WHERE name = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
                params![name.trim()],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn list_people(&self) -> Result<Vec<Person>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, group_name, notes, created_at FROM people
             ORDER BY group_name COLLATE NOCASE, name COLLATE NOCASE, id",
        )?;
        let mut people: Vec<Person> = stmt
            .query_map([], |r| {
                Ok(Person {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    group: r.get(2)?,
                    notes: r.get(3)?,
                    created_at: r.get(4)?,
                    references: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare(
            "SELECT id, person_id, source_name, det_score, face_px, created_at
             FROM reference_faces ORDER BY id",
        )?;
        let refs = stmt.query_map([], |r| {
            Ok(ReferenceSummary {
                id: r.get(0)?,
                person_id: r.get(1)?,
                source_name: r.get(2)?,
                det_score: r.get(3)?,
                face_px: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        for r in refs {
            let r = r?;
            if let Some(p) = people.iter_mut().find(|p| p.id == r.person_id) {
                p.references.push(r);
            }
        }
        Ok(people)
    }

    pub fn person_count(&self) -> Result<u32> {
        Ok(self.conn.lock().query_row("SELECT COUNT(*) FROM people", [], |r| r.get(0))?)
    }

    pub fn add_reference(&self, person_id: i64, r: &NewReference) -> Result<i64> {
        let conn = self.conn.lock();
        let exists: Option<i64> =
            conn.query_row("SELECT id FROM people WHERE id = ?1", params![person_id], |r| r.get(0)).optional()?;
        if exists.is_none() {
            return Err(Error::NotFound(format!("person {person_id} not found")));
        }
        conn.execute(
            "INSERT INTO reference_faces(person_id, source_name, crop_jpeg, crop_landmarks, det_score, face_px, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                person_id,
                r.source_name,
                r.crop_jpeg,
                serde_json::to_string(&r.crop_landmarks)?,
                r.det_score,
                r.face_px,
                now()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete_reference(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM reference_faces WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reference_crop(&self, id: i64) -> Result<Vec<u8>> {
        self.conn
            .lock()
            .query_row("SELECT crop_jpeg FROM reference_faces WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("reference photo {id} not found")))
    }

    /// The stored crop of a reference face plus its landmarks in crop coordinates.
    pub fn reference(&self, id: i64) -> Result<StoredReference> {
        let (person_id, crop_jpeg, lm): (i64, Vec<u8>, String) = self
            .conn
            .lock()
            .query_row(
                "SELECT person_id, crop_jpeg, crop_landmarks FROM reference_faces WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("reference photo {id} not found")))?;
        Ok(StoredReference { id, person_id, crop_jpeg, crop_landmarks: serde_json::from_str(&lm)? })
    }

    /// Reference faces without an embedding for `model`.
    pub fn references_missing_embedding(&self, model: &str) -> Result<Vec<StoredReference>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.person_id, r.crop_jpeg, r.crop_landmarks FROM reference_faces r
             WHERE NOT EXISTS (SELECT 1 FROM reference_embeddings e WHERE e.reference_id = r.id AND e.model = ?1)",
        )?;
        let rows = stmt.query_map(params![model], |r| {
            let lm: String = r.get(3)?;
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Vec<u8>>(2)?, lm))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, person_id, crop_jpeg, lm) = row?;
            out.push(StoredReference { id, person_id, crop_jpeg, crop_landmarks: serde_json::from_str(&lm)? });
        }
        Ok(out)
    }

    pub fn put_reference_embedding(&self, reference_id: i64, model: &str, v: &[f32]) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO reference_embeddings(reference_id, model, vector) VALUES(?1, ?2, ?3)
             ON CONFLICT(reference_id, model) DO UPDATE SET vector = excluded.vector",
            params![reference_id, model, f32s_to_blob(v)],
        )?;
        Ok(())
    }

    /// `(person_id, embedding)` for every reference face embedded with `model`.
    pub fn roster_embeddings(&self, model: &str) -> Result<Vec<(i64, Vec<f32>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT r.person_id, e.vector FROM reference_embeddings e
             JOIN reference_faces r ON r.id = e.reference_id WHERE e.model = ?1 ORDER BY r.id",
        )?;
        let rows =
            stmt.query_map(params![model], |r| Ok((r.get::<_, i64>(0)?, blob_to_f32s(&r.get::<_, Vec<u8>>(1)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ----------------------------------------------------------------- scans

    pub fn create_scan(
        &self,
        name: &str,
        input_dir: &str,
        detector: &str,
        embedder: &str,
        config: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock();
        let roster: u32 = conn.query_row("SELECT COUNT(*) FROM people", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO scans(name, input_dir, created_at, status, detector_id, embedder_id, config, roster_size)
             VALUES(?1, ?2, ?3, 'running', ?4, ?5, ?6, ?7)",
            params![name, input_dir, now(), detector, embedder, config, roster],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finish_scan(&self, id: i64, status: ScanStatus) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE scans SET status = ?2, finished_at = ?3 WHERE id = ?1",
            params![id, enum_str(&status), now()],
        )?;
        Ok(())
    }

    /// Mark scans left "running" by a crash or forced quit as failed.
    pub fn recover_interrupted_scans(&self) -> Result<usize> {
        Ok(self.conn.lock().execute("UPDATE scans SET status = 'failed' WHERE status = 'running'", [])?)
    }

    pub fn update_roster_size(&self, scan_id: i64) -> Result<()> {
        self.conn
            .lock()
            .execute("UPDATE scans SET roster_size = (SELECT COUNT(*) FROM people) WHERE id = ?1", params![scan_id])?;
        Ok(())
    }

    pub fn delete_scan(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM scans WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Insert one processed photo and its faces in a single transaction.
    pub fn insert_photo(&self, scan_id: i64, p: &NewPhoto) -> Result<i64> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO photos(scan_id, rel_path, width, height, status, error, thumb_jpeg, file_size, modified)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                scan_id,
                p.rel_path,
                p.width,
                p.height,
                enum_str(&p.status),
                p.error,
                p.thumb_jpeg,
                p.file_size as i64,
                p.modified
            ],
        )?;
        let photo_id = tx.last_insert_rowid();
        for f in &p.faces {
            insert_face(&tx, photo_id, f, false)?;
        }
        tx.commit()?;
        Ok(photo_id)
    }

    pub fn list_scans(&self) -> Result<Vec<ScanSummary>> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare("SELECT id FROM scans ORDER BY id DESC")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        ids.into_iter().map(|id| self.scan(id)).collect()
    }

    pub fn scan(&self, id: i64) -> Result<ScanSummary> {
        let conn = self.conn.lock();
        let mut s = conn
            .query_row(
                "SELECT id, name, input_dir, created_at, finished_at, status, detector_id, embedder_id, roster_size
                 FROM scans WHERE id = ?1",
                params![id],
                |r| {
                    Ok(ScanSummary {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        input_dir: r.get(2)?,
                        created_at: r.get(3)?,
                        finished_at: r.get(4)?,
                        status: parse_enum(&r.get::<_, String>(5)?)?,
                        detector_id: r.get(6)?,
                        embedder_id: r.get(7)?,
                        roster_size: r.get(8)?,
                        counts: ScanCounts::default(),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("scan {id} not found")))?;

        let c = &mut s.counts;
        conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(status = 'ok'), 0),
                    COALESCE(SUM(status = 'unsupported'), 0),
                    COALESCE(SUM(status = 'error'), 0)
             FROM photos WHERE scan_id = ?1",
            params![id],
            |r| {
                c.photos = r.get(0)?;
                c.photos_ok = r.get(1)?;
                c.unsupported = r.get(2)?;
                c.errors = r.get(3)?;
                Ok(())
            },
        )?;
        conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(f.pile = 'matches'), 0),
                    COALESCE(SUM(f.pile = 'matches' AND f.reviewed = 1), 0),
                    COALESCE(SUM(f.pile = 'check'), 0),
                    COALESCE(SUM(f.pile = 'check' AND f.decision = 'pending'), 0),
                    COALESCE(SUM(f.category = 'manual'), 0),
                    COALESCE(SUM(f.decision = 'blur'), 0),
                    COALESCE(SUM(f.decision = 'pending'), 0),
                    COUNT(DISTINCT CASE WHEN f.decision = 'pending' THEN f.photo_id END)
             FROM faces f JOIN photos p ON p.id = f.photo_id WHERE p.scan_id = ?1",
            params![id],
            |r| {
                c.faces = r.get(0)?;
                c.matches = r.get(1)?;
                c.matches_reviewed = r.get(2)?;
                c.check = r.get(3)?;
                c.check_pending = r.get(4)?;
                c.manual = r.get(5)?;
                c.to_blur = r.get(6)?;
                c.pending = r.get(7)?;
                c.photos_held = r.get(8)?;
                Ok(())
            },
        )?;
        Ok(s)
    }

    pub fn scan_config(&self, id: i64) -> Result<String> {
        Ok(self.conn.lock().query_row("SELECT config FROM scans WHERE id = ?1", params![id], |r| r.get(0))?)
    }

    pub fn photos(&self, scan_id: i64) -> Result<Vec<PhotoRecord>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare(&format!("SELECT {PHOTO_COLS} FROM photos p WHERE p.scan_id = ?1 ORDER BY p.rel_path"))?;
        let rows = stmt.query_map(params![scan_id], |r| photo_from_row(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn photo(&self, photo_id: i64) -> Result<PhotoRecord> {
        self.conn
            .lock()
            .query_row(&format!("SELECT {PHOTO_COLS} FROM photos p WHERE p.id = ?1"), params![photo_id], |r| {
                photo_from_row(r, 0)
            })
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("photo {photo_id} not found")))
    }

    pub fn photo_thumb(&self, photo_id: i64) -> Result<Vec<u8>> {
        self.conn
            .lock()
            .query_row("SELECT thumb_jpeg FROM photos WHERE id = ?1", params![photo_id], |r| {
                r.get::<_, Option<Vec<u8>>>(0)
            })
            .optional()?
            .flatten()
            .ok_or_else(|| Error::NotFound(format!("no preview for photo {photo_id}")))
    }

    pub fn photo_faces(&self, photo_id: i64) -> Result<Vec<FaceRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("SELECT {FACE_COLS} FROM faces f WHERE f.photo_id = ?1 ORDER BY f.id"))?;
        let rows = stmt.query_map(params![photo_id], |r| face_from_row(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn face(&self, face_id: i64) -> Result<FaceRecord> {
        self.conn
            .lock()
            .query_row(&format!("SELECT {FACE_COLS} FROM faces f WHERE f.id = ?1"), params![face_id], |r| {
                face_from_row(r, 0)
            })
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("face {face_id} not found")))
    }

    pub fn face_crop(&self, face_id: i64) -> Result<Vec<u8>> {
        self.conn
            .lock()
            .query_row("SELECT crop_jpeg FROM faces WHERE id = ?1", params![face_id], |r| {
                r.get::<_, Option<Vec<u8>>>(0)
            })
            .optional()?
            .flatten()
            .ok_or_else(|| Error::NotFound(format!("no crop for face {face_id}")))
    }

    /// Review list for a pile, in photo order.
    pub fn review_items(&self, scan_id: i64, pile: Pile) -> Result<Vec<ReviewItem>> {
        let conn = self.conn.lock();
        let sql = format!(
            "SELECT {FACE_COLS}, {PHOTO_COLS}, pe.id, pe.name, pe.group_name
             FROM faces f
             JOIN photos p ON p.id = f.photo_id
             LEFT JOIN people pe ON pe.id = f.person_id
             WHERE p.scan_id = ?1 AND f.pile = ?2
             ORDER BY p.rel_path, f.bbox_y, f.bbox_x"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![scan_id, enum_str(&pile)], |r| {
            let face = face_from_row(r, 0)?;
            let photo = photo_from_row(r, FACE_COL_COUNT)?;
            let base = FACE_COL_COUNT + PHOTO_COL_COUNT;
            let person = match r.get::<_, Option<i64>>(base)? {
                Some(id) => {
                    Some(PersonBrief { id, name: r.get(base + 1)?, group: r.get(base + 2)?, reference_ids: Vec::new() })
                }
                None => None,
            };
            Ok(ReviewItem { face, photo, person })
        })?;
        let mut items: Vec<ReviewItem> = rows.collect::<rusqlite::Result<_>>()?;
        let mut ref_stmt = conn.prepare("SELECT id FROM reference_faces WHERE person_id = ?1 ORDER BY id")?;
        for it in &mut items {
            if let Some(p) = &mut it.person {
                p.reference_ids = ref_stmt.query_map(params![p.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            }
        }
        Ok(items)
    }

    /// Record a person's decision for one face.
    pub fn set_decision(&self, face_id: i64, decision: Decision) -> Result<FaceRecord> {
        let n = self.conn.lock().execute(
            "UPDATE faces SET decision = ?2, reviewed = ?3 WHERE id = ?1",
            params![face_id, enum_str(&decision), decision != Decision::Pending],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("face {face_id} not found")));
        }
        self.face(face_id)
    }

    /// Put a face back into an earlier state (used by undo on the review screen).
    pub fn restore_decision(&self, face_id: i64, decision: Decision, reviewed: bool) -> Result<FaceRecord> {
        let n = self.conn.lock().execute(
            "UPDATE faces SET decision = ?2, reviewed = ?3 WHERE id = ?1",
            params![face_id, enum_str(&decision), reviewed && decision != Decision::Pending],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("face {face_id} not found")));
        }
        self.face(face_id)
    }

    /// Apply one decision to every face in a pile (optionally only those still pending).
    pub fn bulk_decision(&self, scan_id: i64, pile: Pile, only_pending: bool, decision: Decision) -> Result<usize> {
        let sql = format!(
            "UPDATE faces SET decision = ?3, reviewed = 1
             WHERE pile = ?2 {} AND photo_id IN (SELECT id FROM photos WHERE scan_id = ?1)",
            if only_pending { "AND decision = 'pending'" } else { "" }
        );
        Ok(self.conn.lock().execute(&sql, params![scan_id, enum_str(&pile), enum_str(&decision)])?)
    }

    /// Apply one decision to every face of a photo in a pile.
    pub fn photo_decision(&self, photo_id: i64, pile: Pile, decision: Decision) -> Result<usize> {
        Ok(self.conn.lock().execute(
            "UPDATE faces SET decision = ?3, reviewed = 1 WHERE photo_id = ?1 AND pile = ?2",
            params![photo_id, enum_str(&pile), enum_str(&decision)],
        )?)
    }

    /// Add a hand-drawn blur region to a photo.
    pub fn add_manual_face(&self, photo_id: i64, bbox: BBox, crop_jpeg: Option<Vec<u8>>) -> Result<i64> {
        let conn = self.conn.lock();
        let face = NewFace {
            bbox,
            landmarks: None,
            det_score: None,
            classification: Classification {
                category: FaceCategory::Manual,
                pile: Pile::None,
                person_id: None,
                similarity: None,
                decision: Decision::Blur,
            },
            embedding: None,
            crop_jpeg,
        };
        insert_face(&conn, photo_id, &face, true)
    }

    /// Delete a hand-drawn region. Detected faces cannot be deleted, only kept.
    pub fn delete_manual_face(&self, face_id: i64) -> Result<()> {
        let n =
            self.conn.lock().execute("DELETE FROM faces WHERE id = ?1 AND category = 'manual'", params![face_id])?;
        if n == 0 {
            return Err(Error::Invalid("only hand-drawn regions can be removed".into()));
        }
        Ok(())
    }

    /// Stored embeddings of a scan's detected faces, for re-matching.
    pub fn face_embeddings(&self, scan_id: i64, model: &str) -> Result<Vec<(FaceRecord, Vec<f32>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {FACE_COLS}, f.embedding FROM faces f JOIN photos p ON p.id = f.photo_id
             WHERE p.scan_id = ?1 AND f.embedding IS NOT NULL AND f.embedding_model = ?2"
        ))?;
        let rows = stmt.query_map(params![scan_id, model], |r| {
            Ok((face_from_row(r, 0)?, blob_to_f32s(&r.get::<_, Vec<u8>>(FACE_COL_COUNT)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Faces of a scan that have no embedding (too small) and are not manual.
    pub fn faces_without_embedding(&self, scan_id: i64) -> Result<Vec<FaceRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {FACE_COLS} FROM faces f JOIN photos p ON p.id = f.photo_id
             WHERE p.scan_id = ?1 AND f.embedding IS NULL AND f.category != 'manual'"
        ))?;
        let rows = stmt.query_map(params![scan_id], |r| face_from_row(r, 0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Overwrite a face's classification (used by re-matching).
    pub fn update_classification(
        &self,
        face_id: i64,
        c: &Classification,
        decision: Decision,
        reviewed: bool,
    ) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE faces SET category = ?2, pile = ?3, person_id = ?4, similarity = ?5, decision = ?6, reviewed = ?7
             WHERE id = ?1",
            params![
                face_id,
                enum_str(&c.category),
                enum_str(&c.pile),
                c.person_id,
                c.similarity,
                enum_str(&decision),
                reviewed
            ],
        )?;
        Ok(())
    }

    /// Photos of a scan with their faces, for export.
    pub fn export_photos(&self, scan_id: i64) -> Result<Vec<ExportPhoto>> {
        let photos = self.photos(scan_id)?;
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("SELECT {FACE_COLS} FROM faces f WHERE f.photo_id = ?1"))?;
        let mut out = Vec::with_capacity(photos.len());
        for photo in photos {
            let faces = stmt.query_map(params![photo.id], |r| face_from_row(r, 0))?.collect::<rusqlite::Result<_>>()?;
            out.push(ExportPhoto { photo, faces });
        }
        Ok(out)
    }
}

const FACE_COLS: &str = "f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.landmarks, f.det_score, \
     f.category, f.pile, f.person_id, f.similarity, f.decision, f.reviewed";
const FACE_COL_COUNT: usize = 14;
const PHOTO_COLS: &str = "p.id, p.scan_id, p.rel_path, p.width, p.height, p.status, p.error, p.file_size, p.modified";
const PHOTO_COL_COUNT: usize = 9;

fn face_from_row(r: &Row, o: usize) -> rusqlite::Result<FaceRecord> {
    let lm: Option<String> = r.get(o + 6)?;
    Ok(FaceRecord {
        id: r.get(o)?,
        photo_id: r.get(o + 1)?,
        bbox: BBox::new(r.get(o + 2)?, r.get(o + 3)?, r.get(o + 4)?, r.get(o + 5)?),
        landmarks: lm.and_then(|s| serde_json::from_str(&s).ok()),
        det_score: r.get(o + 7)?,
        category: parse_enum(&r.get::<_, String>(o + 8)?)?,
        pile: parse_enum(&r.get::<_, String>(o + 9)?)?,
        person_id: r.get(o + 10)?,
        similarity: r.get(o + 11)?,
        decision: parse_enum(&r.get::<_, String>(o + 12)?)?,
        reviewed: r.get(o + 13)?,
    })
}

fn photo_from_row(r: &Row, o: usize) -> rusqlite::Result<PhotoRecord> {
    Ok(PhotoRecord {
        id: r.get(o)?,
        scan_id: r.get(o + 1)?,
        rel_path: r.get(o + 2)?,
        width: r.get(o + 3)?,
        height: r.get(o + 4)?,
        status: parse_enum(&r.get::<_, String>(o + 5)?)?,
        error: r.get(o + 6)?,
        file_size: r.get::<_, i64>(o + 7)? as u64,
        modified: r.get(o + 8)?,
    })
}

fn insert_face(conn: &Connection, photo_id: i64, f: &NewFace, reviewed: bool) -> Result<i64> {
    let c = &f.classification;
    let lm = match &f.landmarks {
        Some(l) => Some(serde_json::to_string(l)?),
        None => None,
    };
    let emb: Option<Vec<u8>> = f.embedding.as_deref().map(f32s_to_blob);
    conn.execute(
        "INSERT INTO faces(photo_id, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, det_score, category, pile,
                           person_id, similarity, decision, reviewed, embedding, embedding_model, crop_jpeg)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                CASE WHEN ?14 IS NULL THEN NULL
                     ELSE (SELECT s.embedder_id FROM scans s JOIN photos p ON p.scan_id = s.id WHERE p.id = ?1) END,
                ?15)",
        params![
            photo_id,
            f.bbox.x,
            f.bbox.y,
            f.bbox.w,
            f.bbox.h,
            lm,
            f.det_score,
            enum_str(&c.category),
            enum_str(&c.pile),
            c.person_id,
            c.similarity,
            enum_str(&c.decision),
            reviewed,
            emb,
            f.crop_jpeg
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests;
