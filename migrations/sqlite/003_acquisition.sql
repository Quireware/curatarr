ALTER TABLE works ADD COLUMN monitored INTEGER NOT NULL DEFAULT 0;
ALTER TABLE series ADD COLUMN monitored INTEGER NOT NULL DEFAULT 0;
ALTER TABLE works ADD COLUMN quality_profile_id TEXT;

CREATE TABLE quality_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    format_order TEXT NOT NULL,
    max_size_bytes INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO quality_profiles (id, name, format_order)
VALUES (
    '00000000-0000-7000-8000-000000000001',
    'Default',
    '["epub","azw3","mobi","pdf","cbz","cb7","cbr"]'
);

CREATE TABLE download_queue (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    protocol TEXT,
    indexer TEXT,
    title TEXT NOT NULL,
    guid TEXT,
    download_url TEXT,
    client TEXT,
    client_ref TEXT,
    output_path TEXT,
    error TEXT,
    idempotency_key TEXT NOT NULL UNIQUE,
    revision INTEGER NOT NULL DEFAULT 0,
    grabbed_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_download_queue_work_id ON download_queue(work_id);
CREATE INDEX idx_download_queue_state ON download_queue(state);
