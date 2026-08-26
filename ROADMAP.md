# pichouse — Feature Roadmap

Running list of features to add down the road. This is a capture document for
ideas as they come up, not a committed plan or schedule. Items here are
unordered by priority unless noted.

## Immich integration

Interface with an [Immich](https://immich.app/) server so the library can work
alongside a self-hosted Immich instance.

### Browse
- Browse Immich albums from within pichouse.
- View the contents of an Immich album (photos/assets) inside the app.

### Upload
- Upload a pichouse album to Immich as a **new** album (created on the Immich
  server).
- Upload a pichouse album into an **existing** Immich album (add to it rather
  than create).

### Sync
- Tag a local album as "synced" with an Immich album.
- When a synced album is tagged, adding pictures to it automatically uploads
  them to Immich in the background.
- Sync tags back and forth between pichouse and Immich (two-way tag sync).

### Open questions / to decide
- Authentication: API key vs. user login; where credentials are stored.
- Server configuration: single server or multiple; where the URL is set
  (settings UI).
- Mapping between pichouse folders/albums and Immich albums.
- Deduplication: how to avoid re-uploading assets already on the server.
- Read-only browse vs. two-way sync (out of scope for now unless stated).
- Background upload: queue, retry, and status reporting for automatic uploads.
- Tag sync semantics: conflict resolution when a tag changes on both sides;
  how pichouse tag sources (AI vs. user) map to Immich tags.
- How "synced" state is persisted (per-album flag in library.db) and what
  happens when the link is broken or the remote album is deleted.

## Non-destructive image editing

Basic image editing, Picasa-style. Edits never modify the original file on
disk; they are stored in the database and applied at view time.

### Edits
- Basic operations: crop, rotate, and similar (e.g. straighten, flip; more
  adjustments like brightness/contrast/color TBD).

### Behaviour
- Edits are stored in the database, not written to the original file
  (non-destructive).
- When an image is viewed, the **edited** version is the default view.
- Option to view the **original** (before edits).
- Option to **revert** the edits (discard, restoring the original as the view).

### Open questions / to decide
- Edit storage format: per-photo edit record / edit stack (ordered list of
  operations) vs. a single derived-state blob; where it lives in library.db.
- How edits interact with thumbnails (regenerate edited thumbnails? cache
  both?) and with the thumb cache keyed by original hash.
- How edits interact with Immich upload/sync (upload original, edited, or
  both; do edits sync?).
- Whether an "export edited copy" (bake edits into a new file) is offered.

## RAW + JPEG pairing

Many libraries contain a RAW file alongside a JPEG of the same shot. These
should be treated as one photo, not two.

### Behaviour
- When a RAW and a JPEG represent the same image, pair them so the photo
  appears **only once** in the thumbnail grid.
- The JPEG is the visible/front image; the RAW sits "behind" it (associated,
  but not shown separately).

### Open questions / to decide
- Pairing rule: match by basename (e.g. `IMG_1234.jpg` + `IMG_1234.cr2`) in the
  same folder; how to handle multiple RAW extensions and case.
- Which RAW formats to recognize (cr2, cr3, nef, arw, dng, raf, orf, rw2, ...).
- What happens if only a RAW exists (no JPEG sidecar) — is it shown directly,
  and how is a thumbnail generated (RAW decode/embedded preview)?
- How the pairing is stored in library.db (a link/reference on the photo row).
- Interaction with edits (edits apply to the JPEG view) and with Immich
  upload/sync (upload JPEG, RAW, or both).
- Behaviour when the pair is broken (one file deleted or moved).
