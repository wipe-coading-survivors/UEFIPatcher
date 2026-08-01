pub mod artifact;
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

    pub fn delete_artifacts_for_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM artifacts WHERE session_id=?1",
            params![session_id],
        )?;
        Ok(())
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
}
