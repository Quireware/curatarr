ALTER TABLE works ADD COLUMN monitored BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE series ADD COLUMN monitored BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE works ADD COLUMN quality_profile_id UUID;

CREATE TABLE quality_profiles (
    id UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    format_order JSONB NOT NULL,
    max_size_bytes BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO quality_profiles (id, name, format_order)
VALUES (
    '00000000-0000-7000-8000-000000000001',
    'Default',
    '["epub","azw3","mobi","pdf","cbz","cb7","cbr"]'::jsonb
);

CREATE TABLE download_queue (
    id UUID PRIMARY KEY NOT NULL,
    work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
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
    grabbed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_download_queue_work_id ON download_queue(work_id);
CREATE INDEX idx_download_queue_state ON download_queue(state);
