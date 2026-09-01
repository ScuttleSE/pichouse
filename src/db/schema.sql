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
    -- 64-bit perceptual hash (dHash) of the oriented image, stored as a signed
    -- INTEGER (bit-cast from u64). 0 means not yet computed. Used by the
    -- duplicate finder for near-duplicate matching.
    phash       INTEGER NOT NULL DEFAULT 0,
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
    added_at    INTEGER NOT NULL DEFAULT 0,
    -- 1 when the user marks this photo unimportant. A skipped photo is excluded
    -- from every future face scan (human faces and stylised faces). Setting this
    -- also removes the photo from all face groups.
    skip_face_scan INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder_id);
-- Fast selection of photos still needing Phase 2 enrichment.
CREATE INDEX IF NOT EXISTS idx_photos_scan_state ON photos(scan_state);
-- Fast selection of recently added photos for the New Files view.
CREATE INDEX IF NOT EXISTS idx_photos_added_at ON photos(added_at);
-- Fast bucketing of byte-identical photos for the duplicate finder.
CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(hash);

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
    position  INTEGER NOT NULL DEFAULT 0,
    -- Face-recognition kind: 0 = inherit (root default Photo), 1 = Photo,
    -- 2 = Art. Controls which face method scans this album's photos.
    kind      INTEGER NOT NULL DEFAULT 0
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

-- Groups a subset of an album's rules so they combine with their own AND/OR
-- mode into a single term of the album's top-level rule_match. One level
-- only — a group cannot contain another group.
CREATE TABLE IF NOT EXISTS virtual_album_rule_groups (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id   INTEGER NOT NULL REFERENCES virtual_albums(id) ON DELETE CASCADE,
    rule_match INTEGER NOT NULL DEFAULT 1   -- 0 = AND, 1 = OR
);

CREATE INDEX IF NOT EXISTS idx_varg_album ON virtual_album_rule_groups(album_id);

-- Structured rules that drive smart membership. field/op/value describe one
-- condition; a NULL group_id means the rule is a top-level term combined by
-- the owning album's rule_match, otherwise it's a member of that rule group.
--   field: 'tag' | 'date_from' | 'date_to' | 'filename' | 'path' | 'folder' | 'person' | 'character'
--   op:    'has' | 'gte' | 'lte' | 'contains' | 'eq'
CREATE TABLE IF NOT EXISTS virtual_album_rules (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    album_id INTEGER NOT NULL REFERENCES virtual_albums(id) ON DELETE CASCADE,
    group_id INTEGER REFERENCES virtual_album_rule_groups(id) ON DELETE CASCADE,
    field    TEXT NOT NULL,
    op       TEXT NOT NULL,
    value    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_var_album ON virtual_album_rules(album_id);
-- idx_var_group is created in migrate() instead of here: on a pre-existing
-- database this table exists without group_id until migrate() adds it, and
-- an index on that column can't be created before the column exists.

-- Non-destructive edits for a photo. One row per edited photo. Edits are never
-- written to the original file on disk; they are applied at view time and when
-- generating thumbnails. All values are integer-scaled so the row stays cheap
-- to compare and copy. A missing row means "no edits" (identity).
--   flip_h/flip_v : 0 or 1, applied after the stored photos.orientation rotate.
--   straighten_mdeg : arbitrary rotation in milli-degrees (1000 = 1 degree),
--                     positive = clockwise, followed by an auto-crop.
--   crop_*        : crop rectangle in per-mille of the post-orient/straighten
--                   image (0..1000). crop_w/h = 0 means "no crop".
--   brightness/contrast : -100..100, 0 = neutral. Applied after levels.
--   lv_<c>_black/white  : per-channel input range, 0..255.
--   lv_<c>_gamma_mille  : per-channel gamma * 1000 (1000 = 1.0).
--   edit_rev      : bumped on every change; forms part of the thumbnail cache
--                   key so edited thumbnails never collide with originals.
CREATE TABLE IF NOT EXISTS photo_edits (
    photo_id        INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    flip_h          INTEGER NOT NULL DEFAULT 0,
    flip_v          INTEGER NOT NULL DEFAULT 0,
    straighten_mdeg INTEGER NOT NULL DEFAULT 0,
    crop_x          INTEGER NOT NULL DEFAULT 0,
    crop_y          INTEGER NOT NULL DEFAULT 0,
    crop_w          INTEGER NOT NULL DEFAULT 0,
    crop_h          INTEGER NOT NULL DEFAULT 0,
    brightness      INTEGER NOT NULL DEFAULT 0,
    contrast        INTEGER NOT NULL DEFAULT 0,
    lv_r_black      INTEGER NOT NULL DEFAULT 0,
    lv_r_white      INTEGER NOT NULL DEFAULT 255,
    lv_r_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    lv_g_black      INTEGER NOT NULL DEFAULT 0,
    lv_g_white      INTEGER NOT NULL DEFAULT 255,
    lv_g_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    lv_b_black      INTEGER NOT NULL DEFAULT 0,
    lv_b_white      INTEGER NOT NULL DEFAULT 255,
    lv_b_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    edit_rev        INTEGER NOT NULL DEFAULT 0
);

-- Saved, reusable color-levels presets. Independent of any photo. Used for
-- negative scans with known color casts. A preset stores only per-channel
-- black/white/gamma levels (the same fields as photo_edits lv_*).
CREATE TABLE IF NOT EXISTS level_presets (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
    lv_r_black INTEGER NOT NULL DEFAULT 0,
    lv_r_white INTEGER NOT NULL DEFAULT 255,
    lv_r_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    lv_g_black INTEGER NOT NULL DEFAULT 0,
    lv_g_white INTEGER NOT NULL DEFAULT 255,
    lv_g_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    lv_b_black INTEGER NOT NULL DEFAULT 0,
    lv_b_white INTEGER NOT NULL DEFAULT 255,
    lv_b_gamma_mille INTEGER NOT NULL DEFAULT 1000,
    created_at INTEGER NOT NULL DEFAULT 0
);

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

-- Links a local scanned folder to an album on an Immich server. When a folder
-- is linked, new photos scanned into it are uploaded to that album in the
-- background. A folder can link to at most one Immich album (folder_id is the
-- primary key).
CREATE TABLE IF NOT EXISTS immich_folder_links (
    folder_id       INTEGER PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE,
    server_id       INTEGER NOT NULL REFERENCES immich_servers(id) ON DELETE CASCADE,
    immich_album_id TEXT NOT NULL,
    created_at      INTEGER NOT NULL DEFAULT 0
);

-- A named person for facial recognition. A cluster of similar faces becomes a
-- person once the user names it. cover_face_id points at a representative face
-- for the person's icon (nullable until a face is chosen).
CREATE TABLE IF NOT EXISTS persons (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    cover_face_id INTEGER,
    created_at    INTEGER NOT NULL DEFAULT 0
);

-- One detected face in one photo. The bounding box and the 5 landmark points
-- are in the coordinate space of the photo AFTER photos.orientation rotation
-- and BEFORE any non-destructive edit. The box is stored in per-mille of that
-- oriented image (0..1000), the same convention as photo_edits crop_*.
--   person_id  : the assigned person, or NULL when unassigned.
--   cluster_id : the automatic similarity cluster, or NULL before clustering.
--   landmarks  : 10 little-endian f32 values (x,y for 5 points), per-mille.
--   embedding  : embedding_dim little-endian f32 values (the face vector).
--   embedding_dim : the vector length, so a model change is detectable.
--   det_score  : detector confidence, 0..1 scaled to 0..1000.
--   confirmed  : 1 when the user approved the person assignment.
--   source     : 0 = detector, 1 = user (a hand-added face box, future use).
CREATE TABLE IF NOT EXISTS faces (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id      INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    person_id     INTEGER REFERENCES persons(id) ON DELETE SET NULL,
    cluster_id    INTEGER,
    bbox_x        INTEGER NOT NULL DEFAULT 0,
    bbox_y        INTEGER NOT NULL DEFAULT 0,
    bbox_w        INTEGER NOT NULL DEFAULT 0,
    bbox_h        INTEGER NOT NULL DEFAULT 0,
    landmarks     BLOB,
    embedding     BLOB,
    embedding_dim INTEGER NOT NULL DEFAULT 0,
    det_score     INTEGER NOT NULL DEFAULT 0,
    confirmed     INTEGER NOT NULL DEFAULT 0,
    source        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);
CREATE INDEX IF NOT EXISTS idx_faces_cluster ON faces(cluster_id);

-- Per-photo face-scan state, mirroring the two-phase scan_state idea. A photo
-- with no row here has not had a detection pass. state: 0 = pending,
-- 1 = scanning, 2 = done.
CREATE TABLE IF NOT EXISTS face_scan (
    photo_id   INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    state      INTEGER NOT NULL DEFAULT 0,
    scanned_at INTEGER NOT NULL DEFAULT 0
);

-- A rejection: the user said this face is NOT this person. Clustering never
-- attaches a rejected face to that person's cluster again, so a correction
-- sticks across re-scans. A face can be rejected from several people.
CREATE TABLE IF NOT EXISTS face_rejections (
    face_id   INTEGER NOT NULL REFERENCES faces(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    PRIMARY KEY (face_id, person_id)
);

CREATE INDEX IF NOT EXISTS idx_face_rejections_face ON face_rejections(face_id);

-- ---------------------------------------------------------------------------
-- Stylised faces (anime, cartoon, furry). A parallel system to the human face
-- tables above. It uses different models and a separate clustering pass. A
-- named group is a "character". The tables mirror persons/faces/face_scan/
-- face_rejections but hold no landmarks (the stylised embedder uses the box).
-- ---------------------------------------------------------------------------

-- A named stylised character. cover_face_id points at a representative face.
CREATE TABLE IF NOT EXISTS characters (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    cover_face_id INTEGER,
    created_at    INTEGER NOT NULL DEFAULT 0
);

-- One detected stylised face in one photo. The bounding box is in per-mille of
-- the oriented image (0..1000), the same convention as faces.
--   character_id  : the assigned character, or NULL when unassigned.
--   cluster_id    : the automatic cluster, or NULL before clustering. -1 means
--                   HDBSCAN noise (an unclear or unmatched face).
--   embedding     : embedding_dim little-endian f32 values (768 for CCIP).
--   embedding_dim : the vector length, so a model change is detectable.
--   det_score     : detector confidence, 0..1 scaled to 0..1000.
--   confirmed     : 1 when the user approved the character assignment.
--   source        : 0 = detector, 1 = user.
CREATE TABLE IF NOT EXISTS style_faces (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    photo_id      INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    character_id  INTEGER REFERENCES characters(id) ON DELETE SET NULL,
    cluster_id    INTEGER,
    bbox_x        INTEGER NOT NULL DEFAULT 0,
    bbox_y        INTEGER NOT NULL DEFAULT 0,
    bbox_w        INTEGER NOT NULL DEFAULT 0,
    bbox_h        INTEGER NOT NULL DEFAULT 0,
    embedding     BLOB,
    embedding_dim INTEGER NOT NULL DEFAULT 0,
    det_score     INTEGER NOT NULL DEFAULT 0,
    confirmed     INTEGER NOT NULL DEFAULT 0,
    source        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_style_faces_photo ON style_faces(photo_id);
CREATE INDEX IF NOT EXISTS idx_style_faces_character ON style_faces(character_id);
CREATE INDEX IF NOT EXISTS idx_style_faces_cluster ON style_faces(cluster_id);

-- Per-photo stylised-face-scan state. state: 0 = pending, 1 = scanning,
-- 2 = done, 3 = error.
CREATE TABLE IF NOT EXISTS style_face_scan (
    photo_id   INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    state      INTEGER NOT NULL DEFAULT 0,
    scanned_at INTEGER NOT NULL DEFAULT 0
);

-- A rejection: the user said this stylised face is NOT this character.
CREATE TABLE IF NOT EXISTS style_face_rejections (
    face_id      INTEGER NOT NULL REFERENCES style_faces(id) ON DELETE CASCADE,
    character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    PRIMARY KEY (face_id, character_id)
);

CREATE INDEX IF NOT EXISTS idx_style_face_rejections_face ON style_face_rejections(face_id);

-- ---------------------------------------------------------------------------
-- Groups organise named people/characters into a nestable tree (e.g. "Disney",
-- "Furry"), separately from persons/characters themselves. A group may nest
-- under a parent group (sub-groups). Unlike album_folders, membership here is
-- NOT exclusive: a person/character may belong to any number of groups at
-- once, so add/remove operations never evict other memberships.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS person_groups (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    parent_id     INTEGER REFERENCES person_groups(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    -- A representative face for the group's tile/icon, chosen by the user via
    -- "Set face as thumbnail" on a member's tile. NULL until chosen. Not a
    -- foreign key, matching persons.cover_face_id/characters.cover_face_id.
    cover_face_id INTEGER
);

CREATE TABLE IF NOT EXISTS person_group_members (
    group_id  INTEGER NOT NULL REFERENCES person_groups(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, person_id)
);

CREATE INDEX IF NOT EXISTS idx_person_group_members_person ON person_group_members(person_id);

-- Parallel group tree for stylised characters. Mirrors person_groups exactly.
CREATE TABLE IF NOT EXISTS character_groups (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    parent_id     INTEGER REFERENCES character_groups(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    cover_face_id INTEGER
);

CREATE TABLE IF NOT EXISTS character_group_members (
    group_id     INTEGER NOT NULL REFERENCES character_groups(id) ON DELETE CASCADE,
    character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, character_id)
);

CREATE INDEX IF NOT EXISTS idx_character_group_members_character ON character_group_members(character_id);

-- ---------------------------------------------------------------------------
-- Duplicate-finder bans. The user marked two photos as "not a duplicate" so the
-- duplicate finder never groups that pair again. The pair is stored normalised
-- (photo_a < photo_b). Either photo's deletion cascades the ban away.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS dup_bans (
    photo_a  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    photo_b  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    banned_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (photo_a, photo_b)
);

CREATE INDEX IF NOT EXISTS idx_dup_bans_b ON dup_bans(photo_b);
