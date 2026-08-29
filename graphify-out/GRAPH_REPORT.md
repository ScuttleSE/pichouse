# Graph Report - pichouse  (2026-08-29)

## Corpus Check
- 99 files · ~134,485 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1812 nodes · 4465 edges · 115 communities (77 shown, 38 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 123 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3fd7d165`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Sidebar
- Generator
- Library
- EditPanel
- Viewer
- stylefacescan.rs
- PhotoEdit
- scan.rs
- Library
- Result
- db/mod.rs
- AppState
- facescan.rs
- Client
- CharactersView
- reconcile.rs
- Properties
- settings.rs
- Grid
- face/cluster.rs
- NewFilesView
- grid.rs
- Manager
- dedup.rs
- Library
- model.rs
- aitag.rs
- Library
- ui/immich.rs
- Result
- Rc
- styleface/models.rs
- app.rs
- Client
- Prefs
- face/models.rs
- .show_duplicates
- Photo
- FolderTree
- vmenu.rs
- .new
- vrules.rs
- opencode.json
- Result
- enrich.rs
- graphify.js
- StatusBar
- Embedder
- TextureCache
- PhotoObject
- Embedder
- name_style_clusters_dialog
- RULE THREE: versioning and named release process
- db/config.rs
- actions.rs
- CLAUDE.md
- immich_thumbs.rs
- name_cluster_dialog
- Controller
- parse_log_level
- toolbar.rs
- inference_test.rs
- Result
- HANDOFF.md
- Human facial detection and recognition system
- parse_tags
- FacesView
- state.rs
- StyleFaceConfig
- Duplicate image finder (HANDOFF)
- Immich integration (HANDOFF)
- Non-destructive editing and color levels (HANDOFF)
- Timeline, copy, crop overlay, slideshows, logging, freeze fixes
- pichouse tech stack (AGENTS.md)
- License switch from MIT to the Unlicense
- Virtual albums, drag-drop, tree persistence (HANDOFF)
- Color levels for negative scans (README)
- build_factory
- Background HTTP work pattern
- CI description (build.yaml on debian-go runner)
- DB schema and additive migration pattern
- Grid entry points pattern (Source enum)
- pichouse project description
- RULE FOUR: clean the debug build cache
- RULE ONE: always commit and push
- RULE ZERO: do not speculate in a loop
- RULE ZERO-A: ASD-STE100 strict communication
- RULE ZERO-B: answer first, do not vomit text
- RULE ZERO-C: diagnose with data, not assumptions
- Settings key/value pattern (library.db)
- Sidebar sections architecture pattern
- Publish rolling pre-release
- Publish Gitea release step
- Sync version into Cargo.toml step
- Parse version from tag step
- Context-menu and character-view fixes
- Four fixes: sidebar crash, album drag, scan freeze, smaller binary
- GitHub release prep, Unlicense switch, and v0.1.0
- pichouse
- AI-based tagging via Ollama (README)
- Baked export (README)
- New Files view (README)
- Adult / mature content tagging (planned)
- Geolocation & maps (planned)
- Picasa-style sidebar tree (planned)
- RAW + JPEG pairing (planned)
- VirtualRule
- LevelPreset
- .set_sort_order
- Config
- spin_row
- settings_faces.rs

## God Nodes (most connected - your core abstractions)
1. `AppState` - 209 edges
2. `Grid` - 100 edges
3. `Sidebar` - 71 edges
4. `Photo` - 67 edges
5. `show_error()` - 57 edges
6. `Viewer` - 53 edges
7. `Library` - 51 edges
8. `EditPanel` - 50 edges
9. `Library` - 36 edges
10. `Library` - 32 edges

## Surprising Connections (you probably didn't know these)
- `Releases section (README)` --semantically_similar_to--> `Named release workflow`  [INFERRED] [semantically similar]
  README.md → .gitea/workflows/release.yaml
- `Fast two-phase import (README)` --semantically_similar_to--> `Fast two-phase import and library freshness (HANDOFF)`  [INFERRED] [semantically similar]
  README.md → HANDOFF.md
- `Library freshness (README)` --semantically_similar_to--> `Fast two-phase import and library freshness (HANDOFF)`  [INFERRED] [semantically similar]
  README.md → HANDOFF.md
- `Facial recognition, local and optional (README)` --semantically_similar_to--> `Human facial detection and recognition system`  [INFERRED] [semantically similar]
  README.md → HANDOFF.md
- `Facial recognition, local and optional (README)` --semantically_similar_to--> `Facial detection & recognition (ROADMAP)`  [INFERRED] [semantically similar]
  README.md → ROADMAP.md

## Import Cycles
- 2-file cycle: `src/ui/properties.rs -> src/ui/state.rs -> src/ui/properties.rs`
- 2-file cycle: `src/ui/state.rs -> src/ui/status.rs -> src/ui/state.rs`
- 2-file cycle: `src/ui/state.rs -> src/ui/viewer.rs -> src/ui/state.rs`
- 3-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/properties.rs -> src/ui/editor.rs`
- 4-file cycle: `src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/state.rs -> src/ui/grid.rs`
- 5-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/editor.rs`

## Hyperedges (group relationships)
- **Non-destructive editing feature documented across README, ROADMAP, and HANDOFF** — readme_non_destructive_editing, roadmap_non_destructive_editing, handoff_non_destructive_editing [INFERRED 0.80]
- **CI release pipeline: rolling build, named release, versioning rule** — gitea_workflows_build_build_and_release_workflow, gitea_workflows_release_named_release_workflow, agents_rule_three [INFERRED 0.85]
- **Dual face-recognition pipelines: human People and stylised Characters** — handoff_stylised_face_recognition, handoff_facial_detection_recognition, readme_people_vs_characters, roadmap_facial_detection_recognition [INFERRED 0.85]

## Communities (115 total, 38 thin omitted)

### Community 0 - "Sidebar"
Cohesion: 0.08
Nodes (42): ListItem, ListView, Propagation, confirm(), prompt_text(), F, Option, Rc (+34 more)

### Community 1 - "Generator"
Cohesion: 0.06
Nodes (44): ImageError, remove_all_thumb_databases(), Connection, Mutex, Option, P, PathBuf, Result (+36 more)

### Community 2 - "Library"
Cohesion: 0.08
Nodes (26): MutexGuard, CountCache, enrichment_under_root_is_scoped_by_prefix(), Library, map_photo(), migrate(), new_files_respects_first_scan_boundary(), now() (+18 more)

### Community 3 - "EditPanel"
Cohesion: 0.12
Nodes (30): CheckButton, Context, Scale, SpinButton, channel_vals(), ChannelWidgets, Controls, draw_triangle() (+22 more)

### Community 4 - "Viewer"
Cohesion: 0.11
Nodes (25): CropPermille, Picture, Pixbuf, SourceId, decode_edited(), decode_pixbuf(), immich_server_for(), pixbuf_to_rgba() (+17 more)

### Community 5 - "stylefacescan.rs"
Cohesion: 0.07
Nodes (47): download_and_extract(), ensure_runtime(), ensure_runtime_progress(), extract_so_from_tgz(), init_runtime(), Fn, Path, PathBuf (+39 more)

### Community 6 - "PhotoEdit"
Cohesion: 0.09
Nodes (41): Rgba, Library, map_edit(), Result, Row, String, Vec, apply_brightness_contrast() (+33 more)

### Community 7 - "scan.rs"
Cohesion: 0.09
Nodes (42): Error, Exif, FnMut, G, civil_to_unix(), civil_unix_year_roundtrip(), dimensions(), enrich_file() (+34 more)

### Community 8 - "Library"
Cohesion: 0.10
Nodes (19): add_photo(), blob_to_floats(), delete_all_clears_everything(), face_roundtrip_preserves_embedding(), face_scan_state_gates_needing_list(), floats_to_blob(), Library, map_face() (+11 more)

### Community 9 - "Result"
Cohesion: 0.09
Nodes (11): blob_to_floats(), floats_to_blob(), Library, map_character(), map_style_face(), HashMap, Option, Result (+3 more)

### Community 10 - "db/mod.rs"
Cohesion: 0.08
Nodes (35): face_thumbs_path(), FaceThumbs, remove_face_thumbs_database(), Connection, Mutex, Option, P, PathBuf (+27 more)

### Community 11 - "AppState"
Cohesion: 0.10
Nodes (20): ApplicationWindow, SimpleAction, characters_pane(), delete_all(), GtkBox, Rc, AppState, Arc (+12 more)

### Community 12 - "facescan.rs"
Cohesion: 0.07
Nodes (37): FaceConfig, Default, Self, String, Cand, Detector, iou(), nms() (+29 more)

### Community 13 - "Client"
Cohesion: 0.11
Nodes (19): civil_from_days(), civil_to_unix(), Client, Error, parse_rfc3339_seconds(), parse_taken_at(), Display, Formatter (+11 more)

### Community 14 - "CharactersView"
Cohesion: 0.16
Nodes (22): Condvar, CharactersView, crop_pool(), CropJob, CropPool, queue_crop_job(), Arc, FlowBox (+14 more)

### Community 15 - "reconcile.rs"
Cohesion: 0.15
Nodes (30): collect(), DbSnapshot, dir_has_images(), mtime_secs(), PhotoInsert, PhotoMove, plan_dir(), plan_vanished_dirs() (+22 more)

### Community 16 - "Properties"
Cohesion: 0.12
Nodes (21): Notebook, bold_label(), field(), Properties, Button, Entry, GtkBox, Label (+13 more)

### Community 17 - "settings.rs"
Cohesion: 0.15
Nodes (23): action_key(), appearance_pane(), capture_shortcut(), folder_pane(), pane_box(), GtkBox, Label, Rc (+15 more)

### Community 18 - "Grid"
Cohesion: 0.08
Nodes (12): GridView, ListStore, SignalHandlerId, Grid, Box, Button, DropDown, Fn (+4 more)

### Community 19 - "face/cluster.rs"
Cohesion: 0.12
Nodes (23): Center, accumulate(), cluster(), ClusterAssignment, ClusterItem, cosine_similarity(), mean(), named_person_anchors_a_stable_cluster() (+15 more)

### Community 20 - "NewFilesView"
Cohesion: 0.11
Nodes (21): decode_texture(), Done, Job, NewFilesView, Arc, AtomicU64, Box, Cell (+13 more)

### Community 21 - "grid.rs"
Cohesion: 0.21
Nodes (14): cell_key(), Done, DupCellUi, DupGroupUi, human_size(), immich_path(), ImmichJob, Job (+6 more)

### Community 22 - "Manager"
Cohesion: 0.21
Nodes (7): Child, Manager, Client, Drop, Option, Result, String

### Community 23 - "dedup.rs"
Cohesion: 0.14
Nodes (22): banned_pair_is_not_grouped(), choose_keep(), DupGroup, exact_hash_groups(), find_duplicates(), format_rank(), keep_prefers_larger_then_lossless(), norm_pair() (+14 more)

### Community 24 - "Library"
Cohesion: 0.26
Nodes (13): cleanup(), crud_nesting_and_cycle(), Library, manual_membership_roundtrip(), mk_photo(), person_rule_matches_photos_of_that_person(), pin_and_exclusion_over_rules(), PathBuf (+5 more)

### Community 25 - "model.rs"
Cohesion: 0.08
Nodes (17): AiStatus, Album, AlbumKind, Character, ImmichAlbum, ImmichFolderLink, Person, PhotoScanState (+9 more)

### Community 26 - "aitag.rs"
Cohesion: 0.27
Nodes (12): ai_tag_folder(), ai_tag_library(), Msg, Arc, AtomicBool, Client, Library, Rc (+4 more)

### Community 27 - "Library"
Cohesion: 0.20
Nodes (9): album_kind_inherits_down_the_tree(), folders_under_album_covers_subtree(), Library, remove_library_folder_then_use_albums(), HashMap, PathBuf, Result, Vec (+1 more)

### Community 28 - "ui/immich.rs"
Cohesion: 0.21
Nodes (22): autoupload_added(), link_and_sync(), refresh_albums(), PathBuf, Rc, String, Vec, sanitize_folder_name() (+14 more)

### Community 29 - "Result"
Cohesion: 0.22
Nodes (10): affected_photo_ids(), fts_query(), Library, merge_tag_into(), rebuild_photo_fts(), HashSet, Result, String (+2 more)

### Community 31 - "styleface/models.rs"
Cohesion: 0.24
Nodes (19): Response, catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry (+11 more)

### Community 32 - "app.rs"
Cohesion: 0.24
Nodes (14): Application, apply_theme(), build_ui(), install_css(), load_folder_into_grid(), load_raw_folder_into_grid(), populate(), populate_deferred() (+6 more)

### Community 33 - "Client"
Cohesion: 0.18
Nodes (11): Client, Error, GenOptions, GenResult, Display, Formatter, From, Result (+3 more)

### Community 34 - "Prefs"
Cohesion: 0.21
Nodes (12): bool_setting(), format_sizes(), load_ai_config(), load_face_config(), load_styleface_config(), parse_sizes(), Prefs, Default (+4 more)

### Community 35 - "face/models.rs"
Cohesion: 0.25
Nodes (17): catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry, ModelKind (+9 more)

### Community 38 - "FolderTree"
Cohesion: 0.24
Nodes (10): base_name(), FolderTree, GtkBox, HashSet, Rc, RefCell, String, StringList (+2 more)

### Community 39 - "vmenu.rs"
Cohesion: 0.23
Nodes (16): album_depth(), bake_source(), build_menu(), copy_photo_to_clipboard(), CopySource, dismiss(), install_grid_context_menu(), local_photo_ids() (+8 more)

### Community 40 - ".new"
Cohesion: 0.24
Nodes (4): Arc, AtomicU64, F, Library

### Community 41 - "vrules.rs"
Cohesion: 0.20
Nodes (17): build_rule_row(), civil_from_days(), collect_rule(), date_roundtrip(), date_to_unix(), days_from_civil(), display_value(), open_rules_editor() (+9 more)

### Community 42 - "opencode.json"
Cohesion: 0.50
Nodes (3): plugin, $schema, .opencode/plugins/graphify.js

### Community 43 - "Result"
Cohesion: 0.24
Nodes (6): Library, HashSet, Option, Result, Vec, ImmichServer

### Community 44 - "enrich.rs"
Cohesion: 0.33
Nodes (15): append_ids(), enqueue_folder(), enqueue_root(), enqueue_visible(), enrich_one(), generate_all(), Msg, rescan_folder() (+7 more)

### Community 46 - "StatusBar"
Cohesion: 0.20
Nodes (6): ProgressBar, Box, Button, Label, Rc, StatusBar

### Community 47 - "Embedder"
Cohesion: 0.30
Nodes (8): Embedder, Mutex, Result, Session, String, Vec, sample_rgb(), umeyama_similarity()

### Community 48 - "TextureCache"
Cohesion: 0.26
Nodes (6): HashMap, Option, String, Texture, Vec, TextureCache

### Community 49 - "PhotoObject"
Cohesion: 0.20
Nodes (8): ObjectImpl, ObjectSubclass, PhotoObject, Option, RefCell, Self, String, Texture

### Community 50 - "Embedder"
Cohesion: 0.29
Nodes (7): Embedder, Mutex, Result, Session, String, Vec, sample_rgb()

### Community 51 - "name_style_clusters_dialog"
Cohesion: 0.44
Nodes (10): assign_style_cluster_to_character(), assign_style_clusters_to_character(), assign_style_face_dialog(), name_style_clusters(), name_style_clusters_dialog(), F, Rc, Result (+2 more)

### Community 52 - "RULE THREE: versioning and named release process"
Cohesion: 0.22
Nodes (10): RULE THREE: versioning and named release process, Build and release workflow, Bump build number step, Detect documentation-only push, Named release workflow, Publish GitHub release step, Push filtered snapshot to GitHub step, GitHub mirror pushes filtered snapshot, not full history (+2 more)

### Community 53 - "db/config.rs"
Cohesion: 0.56
Nodes (9): config_path(), data_dir(), default_data_dir(), home_dir(), read_configured_data_dir(), Option, PathBuf, Result (+1 more)

### Community 54 - "actions.rs"
Cohesion: 0.42
Nodes (10): add_library_folder(), enqueue_scan(), find_duplicates(), Msg, rescan_all(), resume_scan(), Rc, String (+2 more)

### Community 56 - "immich_thumbs.rs"
Cohesion: 0.20
Nodes (13): immich_thumbs_path_for_server(), ImmichThumbs, remove_all_immich_thumb_databases(), remove_db_files(), remove_immich_thumbs_for_server(), Connection, Mutex, Option (+5 more)

### Community 57 - "name_cluster_dialog"
Cohesion: 0.50
Nodes (8): assign_cluster_to_person(), assign_face_dialog(), name_cluster(), name_cluster_dialog(), F, Rc, Result, String

### Community 58 - "Controller"
Cohesion: 0.24
Nodes (5): Controller, Arc, AtomicBool, Mutex, Option

### Community 59 - "parse_log_level"
Cohesion: 0.46
Nodes (7): LevelFilter, init_logging(), main(), parse_log_level(), print_usage(), ExitCode, Result

### Community 60 - "toolbar.rs"
Cohesion: 0.44
Nodes (8): build_toolbar(), compact_button(), Button, GtkBox, Rc, start_slideshow(), start_slideshow_from_prefs(), toggle_properties()

### Community 61 - "inference_test.rs"
Cohesion: 0.43
Nodes (7): cosine(), data_dir(), face_pipeline_real_inference(), init(), load_rgb(), PathBuf, Vec

### Community 62 - "Result"
Cohesion: 0.24
Nodes (4): Library, HashSet, Result, Vec

### Community 63 - "HANDOFF.md"
Cohesion: 0.29
Nodes (4): RULE TWO: hand off before context is too large, Fast two-phase import and library freshness (HANDOFF), Fast two-phase import (README), Library freshness (README)

### Community 64 - "Human facial detection and recognition system"
Cohesion: 0.38
Nodes (7): ONNX Runtime download-on-demand design, CCIP embedder swap for stylised faces, Human facial detection and recognition system, Stylised face recognition and album Face type, Facial recognition, local and optional (README), People vs. Characters: two recognition pipelines (README), Facial detection & recognition (ROADMAP)

### Community 65 - "parse_tags"
Cohesion: 0.43
Nodes (5): clean_tag(), parse_tags(), parse_tags_cases(), String, Vec

### Community 66 - "FacesView"
Cohesion: 0.27
Nodes (8): FacesView, FlowBox, GtkBox, Label, Option, Rc, RefCell, Self

### Community 67 - "state.rs"
Cohesion: 0.20
Nodes (14): autoscan_routed(), Rc, scan_album_faces(), scan_art(), scan_photo(), immich_pane(), labeled_entry(), Entry (+6 more)

### Community 68 - "StyleFaceConfig"
Cohesion: 0.25
Nodes (4): Default, Self, String, StyleFaceConfig

### Community 69 - "Duplicate image finder (HANDOFF)"
Cohesion: 1.00
Nodes (3): Duplicate image finder (HANDOFF), Duplicate image finder (README), Duplicate image finder (ROADMAP)

### Community 70 - "Immich integration (HANDOFF)"
Cohesion: 1.00
Nodes (3): Immich integration (HANDOFF), Immich integration (README), Immich integration (ROADMAP, phased)

### Community 71 - "Non-destructive editing and color levels (HANDOFF)"
Cohesion: 1.00
Nodes (3): Non-destructive editing and color levels (HANDOFF), Non-destructive editing (README), Non-destructive image editing (ROADMAP)

### Community 72 - "Timeline, copy, crop overlay, slideshows, logging, freeze fixes"
Cohesion: 0.67
Nodes (3): Timeline, copy, crop overlay, slideshows, logging, freeze fixes, Move to llama.cpp instead of Ollama (planned), Slideshows (ROADMAP)

### Community 77 - "build_factory"
Cohesion: 0.20
Nodes (10): Overlay, SignalListItemFactory, apply_texture(), build_factory(), decode_texture(), overlay_parts(), Image, Label (+2 more)

### Community 109 - "VirtualRule"
Cohesion: 0.33
Nodes (7): build_membership_sql(), Box, String, rule_clause(), RuleOp, VirtualRule, ToSql

### Community 110 - "LevelPreset"
Cohesion: 0.27
Nodes (6): Library, map_preset(), Result, Row, Vec, LevelPreset

### Community 112 - "Config"
Cohesion: 0.38
Nodes (4): Config, normalize_fills_defaults_and_clamps(), Default, String

### Community 113 - "spin_row"
Cohesion: 0.57
Nodes (6): ai_pane(), fixed_label(), GtkBox, Label, Rc, spin_row()

### Community 114 - "settings_faces.rs"
Cohesion: 0.70
Nodes (4): delete_all_face_data(), faces_pane(), GtkBox, Rc

## Knowledge Gaps
- **36 isolated node(s):** `$schema`, `.opencode/plugins/graphify.js`, `pichouse`, `Msg`, `graphify` (+31 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **38 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `AppState` to `Sidebar`, `Generator`, `EditPanel`, `Viewer`, `stylefacescan.rs`, `PhotoEdit`, `db/mod.rs`, `facescan.rs`, `CharactersView`, `reconcile.rs`, `Properties`, `settings.rs`, `Grid`, `NewFilesView`, `Manager`, `dedup.rs`, `model.rs`, `aitag.rs`, `ui/immich.rs`, `Rc`, `app.rs`, `Prefs`, `FolderTree`, `vmenu.rs`, `vrules.rs`, `enrich.rs`, `StatusBar`, `name_style_clusters_dialog`, `actions.rs`, `name_cluster_dialog`, `Controller`, `toolbar.rs`, `FacesView`, `state.rs`, `StyleFaceConfig`, `Config`, `spin_row`, `settings_faces.rs`?**
  _High betweenness centrality (0.294) - this node is a cross-community bridge._
- **Why does `Photo` connect `Photo` to `Library`, `EditPanel`, `Viewer`, `PhotoEdit`, `scan.rs`, `Library`, `Result`, `AppState`, `Properties`, `Grid`, `NewFilesView`, `grid.rs`, `dedup.rs`, `Library`, `model.rs`, `aitag.rs`, `Library`, `ui/immich.rs`, `.show_duplicates`, `vmenu.rs`, `PhotoObject`, `Result`?**
  _High betweenness centrality (0.202) - this node is a cross-community bridge._
- **Why does `Grid` connect `Grid` to `state.rs`, `.show_duplicates`, `Photo`, `vmenu.rs`, `.new`, `AppState`, `build_factory`, `.set_sort_order`, `TextureCache`, `PhotoObject`, `grid.rs`, `Rc`?**
  _High betweenness centrality (0.089) - this node is a cross-community bridge._
- **Are the 53 inferred relationships involving `show_error()` (e.g. with `add_library_folder()` and `rescan_all()`) actually correct?**
  _`show_error()` has 53 INFERRED edges - model-reasoned connections that need verification._
- **What connects `$schema`, `.opencode/plugins/graphify.js`, `pichouse` to the rest of the system?**
  _36 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Sidebar` be split into smaller, more focused modules?**
  _Cohesion score 0.0803960396039604 - nodes in this community are weakly interconnected._
- **Should `Generator` be split into smaller, more focused modules?**
  _Cohesion score 0.0629399585921325 - nodes in this community are weakly interconnected._