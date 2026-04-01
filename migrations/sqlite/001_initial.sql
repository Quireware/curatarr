-- curatarr initial schema (SQLite)

CREATE TABLE works (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    original_language TEXT,
    original_pub_date TEXT,
    description TEXT,
    description_html TEXT,
    content_type TEXT NOT NULL,
    age_rating TEXT,
    content_warnings TEXT NOT NULL DEFAULT '[]',
    average_rating REAL,
    user_rating REAL,
    user_review TEXT,
    read_status TEXT NOT NULL DEFAULT 'unread',
    user_notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_works_title ON works(title);
CREATE INDEX idx_works_sort_title ON works(sort_title);
CREATE INDEX idx_works_content_type ON works(content_type);
CREATE INDEX idx_works_read_status ON works(read_status);
CREATE INDEX idx_works_created_at ON works(created_at);

CREATE TABLE authors (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    birth_date TEXT,
    death_date TEXT,
    nationality TEXT,
    biography TEXT,
    biography_html TEXT,
    photo_path TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_authors_name ON authors(name);
CREATE INDEX idx_authors_sort_name ON authors(sort_name);

CREATE TABLE work_authors (
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'author',
    PRIMARY KEY (work_id, author_id, role)
);

CREATE TABLE publishers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    imprint TEXT,
    parent_publisher_id TEXT REFERENCES publishers(id) ON DELETE SET NULL,
    country TEXT,
    founding_year INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_publishers_name ON publishers(name);

CREATE TABLE editions (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    isbn13 TEXT,
    isbn10 TEXT,
    asin TEXT,
    publisher_id TEXT REFERENCES publishers(id) ON DELETE SET NULL,
    imprint TEXT,
    publication_date TEXT,
    edition_number INTEGER,
    format TEXT NOT NULL,
    page_count INTEGER,
    word_count INTEGER,
    language TEXT,
    translator TEXT,
    cover_path TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_editions_work_id ON editions(work_id);
CREATE INDEX idx_editions_isbn13 ON editions(isbn13);
CREATE INDEX idx_editions_isbn10 ON editions(isbn10);
CREATE INDEX idx_editions_format ON editions(format);

CREATE TABLE series (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    description TEXT,
    series_type TEXT NOT NULL DEFAULT 'ongoing',
    reading_order TEXT NOT NULL DEFAULT 'publication',
    volume_count INTEGER,
    expected_volume_count INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_series_title ON series(title);
CREATE INDEX idx_series_sort_title ON series(sort_title);

CREATE TABLE series_entries (
    id TEXT PRIMARY KEY NOT NULL,
    series_id TEXT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    position REAL NOT NULL,
    arc TEXT,
    UNIQUE (series_id, work_id)
);

CREATE INDEX idx_series_entries_series_id ON series_entries(series_id);

CREATE TABLE collections (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE collection_works (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, work_id)
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE work_tags (
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (work_id, tag_id)
);

CREATE TABLE files (
    id TEXT PRIMARY KEY NOT NULL,
    edition_id TEXT NOT NULL REFERENCES editions(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    format TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    import_date TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_files_edition_id ON files(edition_id);
CREATE INDEX idx_files_sha256 ON files(sha256);
CREATE INDEX idx_files_format ON files(format);

CREATE TABLE external_ids (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    external_id TEXT NOT NULL,
    UNIQUE (entity_type, entity_id, provider)
);

CREATE INDEX idx_external_ids_entity ON external_ids(entity_type, entity_id);

CREATE TABLE root_folders (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    name TEXT,
    content_types TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE recycle_bin (
    id TEXT PRIMARY KEY NOT NULL,
    original_file_id TEXT NOT NULL,
    original_path TEXT NOT NULL,
    recycle_path TEXT NOT NULL,
    deleted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_recycle_bin_deleted_at ON recycle_bin(deleted_at);
