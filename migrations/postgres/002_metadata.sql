-- Field locks, provenance, and audit for metadata enrichment.

CREATE TABLE metadata_field_locks (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    field TEXT NOT NULL,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (entity_type, entity_id, field)
);

CREATE TABLE metadata_field_sources (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    field TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (entity_type, entity_id, field)
);

CREATE TABLE metadata_audit_log (
    id UUID PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    field TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_metadata_audit_entity ON metadata_audit_log(entity_type, entity_id, created_at);
