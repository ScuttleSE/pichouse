-- library.db schema for pichouse

CREATE TABLE IF NOT EXISTS library_folders (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    path     TEXT NOT NULL UNIQUE,
    added_at INTEGER NOT NULL,
    -- Unix time when this root's first full scan completed. 0 until then.
    -- A photo counts as "new" only if it was added after this moment, so the
    -- initial import of an existing library never floods the New Files view.
    first_scan_done_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS folders (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    path  TEXT NOT NULL UNIQUE,
    name  TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    year  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS photos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    folder_id   INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    path        TEXT NOT NULL UNIQUE,
    filename    TEXT NOT NULL,
    size        INTEGER NOT NULL,
    mod_time    INTEGER NOT NULL,
    taken_at    INTEGER NOT NULL DEFAULT 0,
    width       INTEGER NOT NULL DEFAULT 0,
    height      INTEGER NOT NULL DEFAULT 0,
    hash        TEXT NOT NULL DEFAULT '',
    thumb_ready INTEGER NOT NULL DEFAULT 0,
    orientation INTEGER NOT NULL DEFAULT 0,
    ai_status   INTEGER NOT NULL DEFAULT 0,
    -- Two-phase import state: 0=structured (cheap stat only), 1=enriching,
    -- 2=done (EXIF/dimensions/hash filled in).
    scan_state  INTEGER NOT NULL DEFAULT 0,
    -- 1 when the file is gone from disk but the row is kept (soft "missing")
    -- so tags/edits survive a temporary unmount, move, or delete.
    missing     INTEGER NOT NULL DEFAULT 0,
    -- Unix time when this photo row was first recorded in the library. Set on
    -- the Phase 1 structure insert. Used with the owning root's
    -- first_scan_done_at to decide whether the photo is "new".
    added_at    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder_id);
-- Fast selection of photos still needing Phase 2 enrichment.
CREATE INDEX IF NOT EXISTS idx_photos_scan_state ON photos(scan_state);
-- Fast selection of recently added photos for the New Files view.
CREATE INDEX IF NOT EXISTS idx_photos_added_at ON photos(added_at);

CREATE TABLE IF NOT EXISTS scan_state (
    folder_id    INTEGER PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE,
    last_scanned INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'pending'
);

-- Albums are a virtual organisation layer over folders. They do not affect
-- files on disk. An album may nest under a parent album (sub-albums).
CREATE TABLE IF NOT EXISTS albums (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL,
    parent_id INTEGER REFERENCES albums(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0
);

-- Membership of a scanned folder in an album. A folder in no album is shown at
-- the Library root under "New folders". position gives the virtual order.
CREATE TABLE IF NOT EXISTS album_folders (
    album_id  INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (album_id, folder_id)
);

CREATE INDEX IF NOT EXISTS idx_album_folders_folder ON album_folders(folder_id);

-- Application settings as key/value pairs. All preferences other than the data
-- directory location live here.
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Tags are a global vocabulary of keywords. Names are case-insensitively
-- unique. Both AI-generated and user-created tags share this table.
CREATE TABLE IF NOT EXISTS tags (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

-- Association between a photo and a tag. source distinguishes AI (0) from
-- user (1) tags; confirmed marks an AI tag the user has approved.
CREATE TABLE IF NOT EXISTS photo_tags (
    photo_id   INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
    source     INTEGER NOT NULL DEFAULT 0,   -- 0=ai, 1=user
    confirmed  INTEGER NOT NULL DEFAULT 0,   -- user confirmed an AI tag
    created_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (photo_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_phototags_tag ON photo_tags(tag_id);

-- Full-text index over the concatenated tag text per photo. rowid == photos.id.
-- Maintained explicitly by the tag write methods (contentless FTS5 table).
CREATE VIRTUAL TABLE IF NOT EXISTS photo_tags_fts
    USING fts5(tags, tokenize='unicode61');

-- Virtual albums group individual *photos* (not folders) drawn from anywhere in
-- the library. They may nest, a photo may belong to many, and membership mixes
-- manually pinned photos with rule-matched (smart) photos. They do not touch
-- files on disk.
CREATE TABLE IF NOT EXISTS virtual_albums (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    parent_id  INTEGER REFERENCES virtual_albums(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    -- How multiple rules combine: 0 = AND (match every rule), 1 = OR (any rule).
    rule_match INTEGER NOT NULL DEFAULT 1
);

-- Manual membership and manual exclusions for a virtual album. kind = 0 pins a
-- photo (always included); kind = 1 excludes a photo the rules would otherwise
-- match (hide it).
CREATE TABLE IF NOT EXISTS virtual_album_photos (
    album_id  INTEGER NOT NULL REFERENCES virtual_albums(id) ON DELETE CASCADE,
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0,
    kind      INTEGER NOT NULL DEFAULT 0,   -- 0 = pin, 1 = exclusion
    PRIMARY KEY (album_id, photo_id)
);

CREATE INDEX IF NOT EXISTS idx_vap_photo ON virtual_album_photos(photo_id);

-- Structured rules that drive smart membership. field/op/value describe one
-- condition; the owning album's rule_match combines them.
--   field: 'tag' | 'date_from' | 'date_to' | 'filename' | 'folder'
--   op:    'has' | 'gte' | 'lte' | 'contains' | 'eq'
CREATE TABLE IF NOT EXISTS virtual_album_rules (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id INTEGER NOT NULL REFERENCES virtual_albums(id) ON DELETE CASCADE,
    field    TEXT NOT NULL,
    op       TEXT NOT NULL,
    value    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_var_album ON virtual_album_rules(album_id);

-- Immich servers the user connects to. Each server is a remote Immich instance
-- reached over HTTP with an API key. pichouse supports more than one server.
-- The API key is stored in plain text, the same way the AI host and port are.
CREATE TABLE IF NOT EXISTS immich_servers (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    name     TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key  TEXT NOT NULL DEFAULT '',
    added_at INTEGER NOT NULL DEFAULT 0
);
