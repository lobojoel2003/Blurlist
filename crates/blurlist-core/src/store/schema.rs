//! Database schema and forward-only migrations (tracked with `PRAGMA user_version`).

use rusqlite::Connection;

use crate::error::{Error, Result};

const MIGRATIONS: &[&str] = &[
    // v1 — initial schema.
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE people (
        id          INTEGER PRIMARY KEY,
        name        TEXT NOT NULL,
        group_name  TEXT NOT NULL DEFAULT '',
        notes       TEXT NOT NULL DEFAULT '',
        created_at  TEXT NOT NULL
    );

    CREATE TABLE reference_faces (
        id              INTEGER PRIMARY KEY,
        person_id       INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
        source_name     TEXT NOT NULL,
        crop_jpeg       BLOB NOT NULL,
        crop_landmarks  TEXT NOT NULL,
        det_score       REAL NOT NULL,
        face_px         INTEGER NOT NULL,
        created_at      TEXT NOT NULL
    );
    CREATE INDEX reference_faces_person ON reference_faces(person_id);

    CREATE TABLE reference_embeddings (
        reference_id  INTEGER NOT NULL REFERENCES reference_faces(id) ON DELETE CASCADE,
        model         TEXT NOT NULL,
        vector        BLOB NOT NULL,
        PRIMARY KEY (reference_id, model)
    );

    CREATE TABLE scans (
        id           INTEGER PRIMARY KEY,
        name         TEXT NOT NULL,
        input_dir    TEXT NOT NULL,
        created_at   TEXT NOT NULL,
        finished_at  TEXT,
        status       TEXT NOT NULL,
        detector_id  TEXT NOT NULL,
        embedder_id  TEXT NOT NULL,
        config       TEXT NOT NULL,
        roster_size  INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE photos (
        id          INTEGER PRIMARY KEY,
        scan_id     INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
        rel_path    TEXT NOT NULL,
        width       INTEGER NOT NULL DEFAULT 0,
        height      INTEGER NOT NULL DEFAULT 0,
        status      TEXT NOT NULL,
        error       TEXT,
        thumb_jpeg  BLOB,
        file_size   INTEGER NOT NULL DEFAULT 0,
        modified    INTEGER NOT NULL DEFAULT 0,
        UNIQUE (scan_id, rel_path)
    );

    CREATE TABLE faces (
        id               INTEGER PRIMARY KEY,
        photo_id         INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
        bbox_x           REAL NOT NULL,
        bbox_y           REAL NOT NULL,
        bbox_w           REAL NOT NULL,
        bbox_h           REAL NOT NULL,
        landmarks        TEXT,
        det_score        REAL,
        category         TEXT NOT NULL,
        pile             TEXT NOT NULL,
        person_id        INTEGER REFERENCES people(id) ON DELETE SET NULL,
        similarity       REAL,
        decision         TEXT NOT NULL,
        reviewed         INTEGER NOT NULL DEFAULT 0,
        embedding        BLOB,
        embedding_model  TEXT,
        crop_jpeg        BLOB
    );
    CREATE INDEX faces_photo ON faces(photo_id);
    CREATE INDEX faces_pile ON faces(pile, decision);
    "#,
];

pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let version = version.max(0) as usize;
    if version > MIGRATIONS.len() {
        return Err(Error::Invalid(format!(
            "this database was created by a newer version of Blurlist (schema v{version}); please update the app"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
