pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    token         TEXT NOT NULL,
    name          TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS artifacts (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    source     TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity);
"#;
