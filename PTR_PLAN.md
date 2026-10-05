# PTR tag database — plan

Status: paused on 2026-10-05. Work continues later on branch `ptr-tags`.

## Handoff (read this first)

This section is for an agent with no memory of the earlier session.

### State

- Branch `ptr-tags` holds all PTR work. `main` has none of it. Do not merge
  without the user's request.
- Done: Phase 0 (spike), Phase 1 (`ptr.db` schema), the sync loop, and the
  `ptr-sync` CLI with retry rules and `--max-gb-per-day`.
- Not done: Phase 3 (settings pane and in-app refresh) and Phase 4
  (potential tags in the UI, import action).
- No full sync ran. The development machine has no disk space for it
  (about 21 GB download, about 100–125 GB `ptr.db`). Never run a full sync
  here. Test with `--stop-at 300` or less, and write to `/tmp`.

### Files

- `src/ptr/client.rs`: HTTP client, metadata parser, retry rules.
- `src/ptr/update.rs`: update file parser (types 34 and 36).
- `src/ptr/db.rs`: `ptr.db` schema, `apply_index`, `build_lookup_indexes`.
- `src/ptr/sync.rs`: sync loop. The app refresh must reuse it.
- `src/ptr/spike.rs`: ignored measuring test.
- `src/bin/ptr-sync.rs`: initial-sync CLI. It includes `src/ptr/` with
  `#[path]`.

### Commands

    cargo build --release
    PICHOUSE_PTR_KEY=<key> target/release/ptr-sync /tmp/t.db --stop-at 300
    PICHOUSE_PTR_KEY=<key> cargo test --release spike_ptr -- --ignored --nocapture

Get the key at https://hydrusnetwork.github.io/hydrus/access_keys.html.
Do not put the key in code or docs.

### Next steps

1. Ask the user for the results of a full `ptr-sync` on the desktop (time,
   size, errors). Update the numbers in this file.
2. Phase 3: settings pane "Tag Database (PTR)" with keys `ptr.enabled`,
   `ptr.path`, `ptr.url`, `ptr.key`. Add a "Refresh" action that runs
   `ptr::sync::sync` in the background (pattern: `src/ui/aitag.rs`).
3. Phase 4: show potential tags in a second text color. Add "Import PTR
   tags". Read the tagging code on `main` first. It can change before then.

### Open questions

- The early commits on `ptr-tags` contain the public PTR key. The user did
  not decide yet whether to remove it from the history (needs a
  force-push).
- `apply_index` does not use `hashes_sha256` during the sync. The refresh
  in the app runs after the index build, so the inserts are slower there.
  Measure before you change it.


## Goal

Give pichouse an optional local copy of the Hydrus Public Tag Repository
(PTR). The copy is a SQLite file. pichouse looks up tags by the file SHA-256.
pichouse updates the copy from time to time.

Most photos in the library are art (booru, anime, furry). The PTR tags are
useful for this type of image. A PTR match occurs only for byte-identical
files. A re-saved or resized copy does not match.

## Facts about the PTR

Source: https://hydrusnetwork.github.io/hydrus/access_keys.html and the Hydrus
source code (license WTFPL v3, so a port is permitted).

- Server: `https://ptr.hydrus.network:45871`. The certificate is self-signed.
- The public read-only access key is on
  https://hydrusnetwork.github.io/hydrus/access_keys.html. pichouse does not
  ship the key. The user gives it to `ptr-sync` and to the app settings.
- Send the key in the `Hydrus-Key` header. A session cookie is optional.
- Do not send a User-Agent that contains `hydrus/<N>`. Use `pichouse/<version>`.
- `GET /metadata?since=<index>` gives the update list. The body is zlib JSON:
  `{metadata_slice: Metadata (type 37)}`. Each entry is
  `(update_index, [hash_hex, ...], begin_ts, end_ts)`.
- `GET /update?update_hash=<hex>` gives one update file.
- The SHA-256 of the downloaded bytes must equal `update_hash`.
- An update file is zlib-compressed UTF-8 JSON. The envelope is
  `[type, version, info]`.
- DefinitionsUpdate (type 36): `[(0, [[hash_id, sha256_hex], ...]),
  (1, [[tag_id, "namespace:tag"], ...])]`.
- ContentUpdate (type 34): `[(content_type, [(action, [data, ...])])]`.
  - content_type: 0 = mappings, 1 = siblings, 2 = parents.
  - action: 0 = add, 1 = delete.
  - Mappings data: `(tag_id, [hash_id, ...])`.
  - Siblings data: `(bad_tag_id, good_tag_id)`.
  - Parents data: `(child_tag_id, parent_tag_id)`.
- The metadata does not show the update type. You must download a file to
  know its type.
- A content update holds only ids. You need the definitions updates to read
  the ids. Thus the first sync must download almost the full history.
- After an error, wait 4 hours. Do not check metadata more often than the
  next-update-due time.

Hydrus source anchors: `hydrus/core/networking/HydrusNetwork.py` (Metadata,
DefinitionsUpdate, ContentUpdate), `hydrus/core/HydrusSerialisable.py`,
`hydrus/server/networking/ServerServer.py`,
`hydrus/client/ClientServices.py` (`_SyncDownloadMetadata`,
`_SyncDownloadUpdates`).

## Size (estimates, not measured)

- Download: about 6 GB in 2021. It is larger now.
- Mappings: more than 2 billion.
- Local SQLite file: probably 30–50 GB. It must be on an SSD.
- The development machine does not have this space. Do not run a full sync
  on it.

## Decisions

- The full copy runs on the user's desktop, in the pichouse GUI.
- The PTR database is optional. It is off by default.
- The file is `ptr.db`, separate from `library.db`. The setting `ptr.path`
  gives its location, so it can go on any disk.

## Phase 0 results (measured 2026-10-05, 80 sample files)

- Metadata: 4681 indexes, 27041 update files. The metadata is 1.1 MB.
- About 28 % of the files are definitions updates. 72 % are content updates.
- A content file holds about 250 000 mappings. It is about 0.7 MB.
- A file download takes about 0.1 s. The server sent no bandwidth errors.
- Estimated full download: about 21 GB.
- Estimated rows: 150–205 M hashes, 18–45 M tags, about 4 000 M mappings.
- Estimated full `ptr.db`: about 100–125 GB. This is a rough number.
- The format agrees with the Hydrus source. The parser in `src/ptr/` reads
  all sampled files with no error.

## Phase 1 results (measured 2026-10-05, indexes 0–1200)

- `ptr-sync` applied 1201 indexes in about 106 s, in three runs. Each run
  continued from `last_index`.
- Rows: 1.4 M hashes, 0.32 M tags, 20 M mappings, 522 siblings, 1537 parents.
- The file was 324 MB with no lookup indexes.
- Rate: about 190 000 mappings per second. A full sync (about 4 000 M
  mappings) can take about 6 hours, plus the index build. Later indexes can
  be slower, because the tables grow.

## Phase 0 — Spike (small data only)

1. Add `src/ptr/` with a protocol client: HTTPS, the `Hydrus-Key` header,
   acceptance of the self-signed certificate, zlib, and the JSON parser.
2. Add an ignored test `spike_ptr`. It writes only to `/tmp`.
   - Fetch `/metadata?since=0`.
   - Download about 10 sample updates (less than 100 MB) from early, middle,
     and recent indexes.
   - Report the update count, the sizes, and the rows per update.
   - Estimate the full download size and the full database size.
   - Delete the samples.
3. Review the numbers with the user. Then fix the schema.

## Two-step sync (decided)

1. Initial sync: a second Rust binary `ptr-sync` in this crate
   (`src/bin/ptr-sync.rs`). It shares `src/ptr/` (client, parser, schema)
   with the app. Usage: `ptr-sync <path/to/ptr.db> --key <access key>` (or `PICHOUSE_PTR_KEY`). It can stop and
   continue from `last_index`. CI builds it next to `pichouse`.
2. Refresh: an in-app action applies only the new updates
   (`/metadata?since=<last_index + 1>`). It uses the same code.

All PTR work stays on the `ptr-tags` branch until the user decides.

## Rate limits and errors (decided)

- The official client sends `User-Agent: hydrus client/20`. It limits itself
  to 2 GB per day per hydrus service by default. pichouse sends
  `pichouse/<version>`. The server accepts it.
- 503 (busy): wait 5 min, double the wait on each retry, up to 6 tries.
- 502, 522, network error: retry after 10 s, doubled each time.
- 429, 509, 529 (bandwidth used up): stop with a message. The next run
  continues from `last_index`.
- `ptr-sync --max-gb-per-day <GB>` limits the average rate. The default is
  no limit.

## Phase 1 — Tag database (`ptr.db`)

- `hashes(id INTEGER PRIMARY KEY, sha256 BLOB)` with an index on `sha256`.
- `tags(id INTEGER PRIMARY KEY, text TEXT)`.
- `mappings(tag_id, hash_id)` as `WITHOUT ROWID`.
- `siblings(bad_tag_id, good_tag_id)` and `parents(child_tag_id,
  parent_tag_id)`.
- `sync_state(last_index, next_update_due, last_error_at)`.
- Apply each update in one transaction, in index order. Then the sync can
  stop and continue with no data loss.

## Phase 2 — Sync engine

- Use the background HTTP pattern of `src/ui/aitag.rs`: a `Msg` channel, a
  coordinator thread, and a `Controller` for cancel.
- First sync: apply all updates from index 0.
- Later sync: `/metadata?since=<last+1>`, then apply only the new updates.
- Follow the Hydrus etiquette (error wait, no fast polling).
- Show progress in the status bar.

## Phase 3 — Settings pane "Tag Database (PTR)"

- On/off switch, path, server, and access key (with defaults).
- "Sync now" and "Cancel" buttons.
- Last sync time and size on disk.

## Phase 4 — Potential tags

- PTR tags are "potential tags". They stay in `ptr.db`. pichouse does not
  copy them into `library.db` by itself.
- pichouse looks up the potential tags by the photo SHA-256. Siblings
  resolve the tags. All namespaces are kept.
- The tag views show two text colors: one for the photo tags, one for the
  potential tags. A potential tag that is already a photo tag shows once, as
  a photo tag.
- The action "Import PTR tags" copies the potential tags into the pichouse
  `tags` and `photo_tags` tables. It works on one photo, the selected photos,
  or an album. The user can also import one potential tag at a time.
- An imported tag is a normal, permanent photo tag. A later sync does not
  change or remove it.
- A later sync can add new potential tags for a photo. They show in the
  potential color until the user imports them.
- A sync never changes `library.db`.

## Open questions

None for now. Review again after the Phase 0 numbers.
