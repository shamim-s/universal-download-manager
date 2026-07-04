-- UDM SQLite schema (Phase 1)

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY,
    url             TEXT NOT NULL,
    filename        TEXT NOT NULL,
    save_path       TEXT NOT NULL,
    file_size       INTEGER,
    downloaded      INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'queued',
    priority        INTEGER NOT NULL DEFAULT 128,
    created_at      TEXT NOT NULL,
    completed_at    TEXT,
    error           TEXT,
    referrer        TEXT,
    cookies         TEXT,
    user_agent      TEXT,
    headers         TEXT,   -- JSON object
    segments        TEXT,   -- JSON array
    checksum        TEXT,   -- JSON object
    source_browser  TEXT,
    tags            TEXT    -- JSON array
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
