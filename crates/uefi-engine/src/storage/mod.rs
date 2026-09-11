pub mod artifact;
pub mod image;
pub mod schema;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Db {
    pub conn: Connection,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub token: String,
    pub name: String,
    pub created_at: i64,
    pub last_activity: i64,
}

#[derive(Debug, Clone)]
pub struct ArtifactRow {
    pub id: String,
    pub session_id: String,
    pub kind: String,
    pub path: String,
    pub size: i64,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ImageRow {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub path: String,
    pub mode: i64,
    pub size: i64,
    pub created_at: i64,
    pub last_activity: i64,
}

#[derive(Debug, Clone)]
pub struct ImageSnapshotRow {
    pub id: String,
    pub image_id: String,
    pub name: String,
    pub size: i64,
    pub created_at: i64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn open_db(path: &Path) -> Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA)?;
    Ok(Db { conn })
}

impl Db {
    pub fn insert_session(&self, id: &str, token: &str, name: &str) -> Result<()> {
        let t = now();
        self.conn.execute(
            "INSERT INTO sessions (id, token, name, created_at, last_activity) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, token, name, t],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>> {
        self.conn
            .query_row(
                "SELECT id, token, name, created_at, last_activity FROM sessions WHERE id=?1",
                params![id],
                |r| {
                    Ok(SessionRow {
                        id: r.get(0)?,
                        token: r.get(1)?,
                        name: r.get(2)?,
                        created_at: r.get(3)?,
                        last_activity: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn touch_session(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET last_activity=?1 WHERE id=?2",
            params![now(), id],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM artifacts WHERE session_id=?1", params![id])?;
        self.conn
            .execute("DELETE FROM sessions WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn delete_session_metadata(&self, id: &str) -> Result<()> {
        self.delete_session(id)
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, token, name, created_at, last_activity FROM sessions")?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                token: r.get(1)?,
                name: r.get(2)?,
                created_at: r.get(3)?,
                last_activity: r.get(4)?,
            })
        })?;
        let mut v = vec![];
        for r in rows {
            v.push(r?);
        }
        Ok(v)
    }

    pub fn list_expired(&self, ttl_secs: i64) -> Result<Vec<String>> {
        let threshold = now() - ttl_secs;
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM sessions WHERE last_activity < ?1")?;
        let rows = stmt.query_map(params![threshold], |r| r.get::<_, String>(0))?;
        let mut v = vec![];
        for r in rows {
            v.push(r?);
        }
        Ok(v)
    }

    pub fn insert_artifact(
        &self,
        id: &str,
        session_id: &str,
        kind: &str,
        path: &str,
        size: i64,
        source: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artifacts (id, session_id, kind, path, size, source, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, session_id, kind, path, size, source, now()],
        )?;
        Ok(())
    }

    pub fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRow>> {
        self.conn
            .query_row(
                "SELECT id, session_id, kind, path, size, source, created_at FROM artifacts WHERE id=?1",
                params![id],
                |r| {
                    Ok(ArtifactRow {
                        id: r.get(0)?,
                        session_id: r.get(1)?,
                        kind: r.get(2)?,
                        path: r.get(3)?,
                        size: r.get(4)?,
                        source: r.get(5)?,
                        created_at: r.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, kind, path, size, source, created_at FROM artifacts WHERE session_id=?1",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(ArtifactRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                kind: r.get(2)?,
                path: r.get(3)?,
                size: r.get(4)?,
                source: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        let mut v = vec![];
        for r in rows {
            v.push(r?);
        }
        Ok(v)
    }

    pub fn insert_image(
        &self,
        id: &str,
        session_id: &str,
        name: &str,
        path: &str,
        mode: i64,
        size: i64,
    ) -> Result<()> {
        let t = now();
        self.conn.execute(
            "INSERT INTO images (id, session_id, name, path, mode, size, created_at, last_activity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![id, session_id, name, path, mode, size, t],
        )?;
        Ok(())
    }

    pub fn get_image(&self, id: &str) -> Result<Option<ImageRow>> {
        self.conn
            .query_row(
                "SELECT id, session_id, name, path, mode, size, created_at, last_activity FROM images WHERE id=?1",
                params![id],
                |r| {
                    Ok(ImageRow {
                        id: r.get(0)?,
                        session_id: r.get(1)?,
                        name: r.get(2)?,
                        path: r.get(3)?,
                        mode: r.get(4)?,
                        size: r.get(5)?,
                        created_at: r.get(6)?,
                        last_activity: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_images(&self, session_id: &str) -> Result<Vec<ImageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, name, path, mode, size, created_at, last_activity FROM images WHERE session_id=?1",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(ImageRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                name: r.get(2)?,
                path: r.get(3)?,
                mode: r.get(4)?,
                size: r.get(5)?,
                created_at: r.get(6)?,
                last_activity: r.get(7)?,
            })
        })?;
        let mut v = vec![];
        for r in rows {
            v.push(r?);
        }
        Ok(v)
    }

    pub fn delete_artifacts_for_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM artifacts WHERE session_id=?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn touch_image(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET last_activity=?1 WHERE id=?2",
            params![now(), id],
        )?;
        Ok(())
    }

    pub fn delete_image(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM images WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn delete_images_for_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM images WHERE session_id=?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn insert_image_snapshot(
        &self,
        id: &str,
        image_id: &str,
        name: &str,
        size: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO image_snapshots (id, image_id, name, size, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, image_id, name, size, now()],
        )?;
        Ok(())
    }

    pub fn get_image_snapshot(&self, id: &str) -> Result<Option<ImageSnapshotRow>> {
        self.conn
            .query_row(
                "SELECT id, image_id, name, size, created_at FROM image_snapshots WHERE id=?1",
                params![id],
                |r| {
                    Ok(ImageSnapshotRow {
                        id: r.get(0)?,
                        image_id: r.get(1)?,
                        name: r.get(2)?,
                        size: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_image_snapshots(&self, image_id: &str) -> Result<Vec<ImageSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, image_id, name, size, created_at FROM image_snapshots WHERE image_id=?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![image_id], |r| {
            Ok(ImageSnapshotRow {
                id: r.get(0)?,
                image_id: r.get(1)?,
                name: r.get(2)?,
                size: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        let mut v = vec![];
        for r in rows {
            v.push(r?);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, Db) {
        let td = TempDir::new().unwrap();
        let db = open_db(&td.path().join("test.db")).unwrap();
        (td, db)
    }

    #[test]
    fn insert_and_get_session_with_name() {
        let (_td, db) = test_db();
        db.insert_session("s1", "tok1", "/home/user/work").unwrap();
        let row = db.get_session("s1").unwrap().unwrap();
        assert_eq!(row.token, "tok1");
        assert_eq!(row.name, "/home/user/work");
    }

    #[test]
    fn delete_session_metadata_keeps_no_trace() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "name1").unwrap();
        db.insert_artifact("a1", "s1", "extracted", "/x.bin", 100, "GUID:0x10")
            .unwrap();
        db.delete_session_metadata("s1").unwrap();
        assert!(db.get_session("s1").unwrap().is_none());
        assert!(db.list_artifacts("s1").unwrap().is_empty());
    }

    #[test]
    fn list_artifacts_for_session() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_artifact("a1", "s1", "extracted", "/x", 100, "target1")
            .unwrap();
        db.insert_artifact("a2", "s1", "imported", "/y", 200, "file.bin")
            .unwrap();
        let arts = db.list_artifacts("s1").unwrap();
        assert_eq!(arts.len(), 2);
    }

    #[test]
    fn list_expired() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.conn
            .execute("UPDATE sessions SET last_activity = 0", [])
            .unwrap();
        let expired = db.list_expired(3600).unwrap();
        assert!(expired.contains(&"s1".into()));
    }

    #[test]
    fn insert_and_get_image() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "BIOS.bin", "/path/bios.bin", 1, 16_000_000)
            .unwrap();
        let row = db.get_image("img1").unwrap().unwrap();
        assert_eq!(row.name, "BIOS.bin");
        assert_eq!(row.path, "/path/bios.bin");
        assert_eq!(row.mode, 1);
        assert_eq!(row.size, 16_000_000);
    }

    #[test]
    fn get_image_missing_returns_none() {
        let (_td, db) = test_db();
        assert!(db.get_image("nope").unwrap().is_none());
    }

    #[test]
    fn list_images_for_session() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        db.insert_image("img2", "s1", "B.bin", "/b", 1, 200)
            .unwrap();
        let imgs = db.list_images("s1").unwrap();
        assert_eq!(imgs.len(), 2);
    }

    #[test]
    fn list_images_empty_for_other_session() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_session("s2", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        assert!(db.list_images("s2").unwrap().is_empty());
    }

    #[test]
    fn touch_image_updates_last_activity() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        let before = db.get_image("img1").unwrap().unwrap().last_activity;
        std::thread::sleep(std::time::Duration::from_secs(2));
        db.touch_image("img1").unwrap();
        let after = db.get_image("img1").unwrap().unwrap().last_activity;
        assert!(
            after > before,
            "last_activity must advance: {before} -> {after}"
        );
    }

    #[test]
    fn delete_image_removes_row() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        db.delete_image("img1").unwrap();
        assert!(db.get_image("img1").unwrap().is_none());
    }

    #[test]
    fn delete_images_for_session_removes_all() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        db.insert_image("img2", "s1", "B.bin", "/b", 1, 200)
            .unwrap();
        db.delete_images_for_session("s1").unwrap();
        assert!(db.list_images("s1").unwrap().is_empty());
    }

    #[test]
    fn delete_session_cascades_to_images() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_image("img1", "s1", "A.bin", "/a", 0, 100)
            .unwrap();
        db.delete_session("s1").unwrap();
        assert!(db.get_image("img1").unwrap().is_none());
    }

    #[test]
    fn image_snapshots_roundtrip_and_cascade() {
        let (_td, db) = test_db();
        db.insert_session("s1", "tok", "n").unwrap();
        db.insert_image("i1", "s1", "n", "p", 1, 16).unwrap();
        db.insert_image_snapshot("sn1", "i1", "before", 16).unwrap();
        let rows = db.list_image_snapshots("i1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "before");
        assert_eq!(
            db.get_image_snapshot("sn1").unwrap().unwrap().image_id,
            "i1"
        );
        db.delete_session_metadata("s1").unwrap();
        assert!(db.list_image_snapshots("i1").unwrap().is_empty());
    }
}
