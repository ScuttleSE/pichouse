-- library.db schema for pichouse

CREATE TABLE IF NOT EXISTS library_folders (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    path     TEXT NOT NULL UNIQUE,
    added_at INTEGER NOT NULL
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
    ai_status   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder_id);

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
