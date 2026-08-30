# Graph Report - pichouse  (2026-08-30)

## Corpus Check
- 102 files · ~147,705 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1940 nodes · 5010 edges · 115 communities (78 shown, 37 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 152 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3f2fcd00`
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
- Library
- db/mod.rs
- AppState
- facescan.rs
- Client
- Properties
- reconcile.rs
- CharactersView
- settings.rs
- Grid
- face/cluster.rs
- NewFilesView
- grid.rs
- Manager
- dedup.rs
- Library
- model.rs
- show_message
- Library
- ui/immich.rs
- Result
- Rc
- styleface/models.rs
- immich_thumbs.rs
- Client
- Library
- face/models.rs
- PhotoObject
- Photo
- vrules.rs
- vmenu.rs
- .new
- Prefs
- opencode.json
- Library
- enrich.rs
- graphify.js
- StatusBar
- Embedder
- TextureCache
- app.rs
- Embedder
- Result
- RULE THREE: versioning and named release process
- db/config.rs
- actions.rs
- CLAUDE.md
- FacesView
- name_cluster_dialog
- Config
- parse_log_level
- Result
- inference_test.rs
- FolderTree
- HANDOFF.md
- Human facial detection and recognition system
- parse_tags
- Controller
- immich_pane
- .set_sort_order
- Duplicate image finder (HANDOFF)
- Immich integration (HANDOFF)
- Non-destructive editing and color levels (HANDOFF)
- Timeline, copy, crop overlay, slideshows, logging, freeze fixes
- pichouse tech stack (AGENTS.md)
- License switch from MIT to the Unlicense
- Virtual albums, drag-drop, tree persistence (HANDOFF)
- Color levels for negative scans (README)
- toolbar.rs
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
- FaceConfig
- StyleFaceConfig
- settings_characters.rs
- spin_row
- build_membership_sql
- settings_faces.rs

## God Nodes (most connected - your core abstractions)
1. `AppState` - 223 edges
2. `Grid` - 105 edges
3. `Sidebar` - 91 edges
4. `show_error()` - 72 edges
5. `Photo` - 68 edges
6. `Viewer` - 53 edges
7. `Library` - 52 edges
8. `EditPanel` - 50 edges
9. `Library` - 39 edges
10. `Library` - 34 edges

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

## Communities (115 total, 37 thin omitted)

### Community 0 - "Sidebar"
Cohesion: 0.08
Nodes (45): ListItem, ListView, Propagation, confirm(), prompt_text(), F, Option, Rc (+37 more)

### Community 1 - "Generator"
Cohesion: 0.06
Nodes (44): ImageError, remove_all_thumb_databases(), Connection, Mutex, Option, P, PathBuf, Result (+36 more)

### Community 2 - "Library"
Cohesion: 0.08
Nodes (27): MutexGuard, CountCache, enrichment_in_is_scoped_to_folder_set(), enrichment_under_root_is_scoped_by_prefix(), Library, map_photo(), migrate(), new_files_respects_first_scan_boundary() (+19 more)

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
Cohesion: 0.07
Nodes (47): Rgba, Library, map_edit(), Result, Row, String, Vec, Library (+39 more)

### Community 7 - "scan.rs"
Cohesion: 0.09
Nodes (44): Error, Exif, FnMut, Instant, civil_to_unix(), civil_unix_year_roundtrip(), dimensions(), enrich_file() (+36 more)

### Community 8 - "Library"
Cohesion: 0.10
Nodes (22): add_photo(), blob_to_floats(), delete_all_clears_everything(), face_roundtrip_preserves_embedding(), face_scan_state_gates_needing_list(), FaceGroup, floats_to_blob(), Library (+14 more)

### Community 9 - "Library"
Cohesion: 0.09
Nodes (16): add_photo(), blob_to_floats(), floats_to_blob(), Library, map_character(), map_style_face(), photos_in_style_cluster_excludes_already_named_faces(), HashMap (+8 more)

### Community 10 - "db/mod.rs"
Cohesion: 0.08
Nodes (35): face_thumbs_path(), FaceThumbs, remove_face_thumbs_database(), Connection, Mutex, Option, P, PathBuf (+27 more)

### Community 11 - "AppState"
Cohesion: 0.09
Nodes (25): ApplicationWindow, Condvar, SimpleAction, AppState, crop_pool(), CropJob, CropPool, now_millis() (+17 more)

### Community 12 - "facescan.rs"
Cohesion: 0.08
Nodes (38): Cand, Detector, iou(), nms(), Mutex, Result, Session, String (+30 more)

### Community 13 - "Client"
Cohesion: 0.11
Nodes (19): civil_from_days(), civil_to_unix(), Client, Error, parse_rfc3339_seconds(), parse_taken_at(), Display, Formatter (+11 more)

### Community 14 - "Properties"
Cohesion: 0.14
Nodes (18): Notebook, bold_label(), field(), Properties, Button, Entry, GtkBox, Label (+10 more)

### Community 15 - "reconcile.rs"
Cohesion: 0.15
Nodes (30): collect(), DbSnapshot, dir_has_images(), mtime_secs(), PhotoInsert, PhotoMove, plan_dir(), plan_vanished_dirs() (+22 more)

### Community 16 - "CharactersView"
Cohesion: 0.23
Nodes (13): CharactersView, Button, FlowBox, GtkBox, Label, Option, Rc, RefCell (+5 more)

### Community 17 - "settings.rs"
Cohesion: 0.15
Nodes (23): action_key(), appearance_pane(), capture_shortcut(), folder_pane(), pane_box(), GtkBox, Label, Rc (+15 more)

### Community 18 - "Grid"
Cohesion: 0.07
Nodes (14): GridView, ListStore, SignalHandlerId, Grid, Box, Button, DrawingArea, DropDown (+6 more)

### Community 19 - "face/cluster.rs"
Cohesion: 0.12
Nodes (23): Center, accumulate(), cluster(), ClusterAssignment, ClusterItem, cosine_similarity(), mean(), named_person_anchors_a_stable_cluster() (+15 more)

### Community 20 - "NewFilesView"
Cohesion: 0.11
Nodes (21): decode_texture(), Done, Job, NewFilesView, Arc, AtomicU64, Box, Cell (+13 more)

### Community 21 - "grid.rs"
Cohesion: 0.11
Nodes (25): Overlay, SignalListItemFactory, apply_texture(), build_factory(), cell_key(), decode_texture(), Done, DupCellUi (+17 more)

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
Cohesion: 0.07
Nodes (16): AiStatus, AlbumKind, ImmichAlbum, PersonGroup, PhotoScanState, Self, String, RuleField (+8 more)

### Community 26 - "show_message"
Cohesion: 0.17
Nodes (22): ai_tag_folder(), ai_tag_library(), Msg, Arc, AtomicBool, Client, Library, Rc (+14 more)

### Community 27 - "Library"
Cohesion: 0.18
Nodes (11): album_kind_inherits_down_the_tree(), folder_effective_face_kind_follows_its_album_or_defaults_to_photo(), folders_under_album_covers_subtree(), Library, remove_library_folder_then_use_albums(), HashMap, PathBuf, Result (+3 more)

### Community 28 - "ui/immich.rs"
Cohesion: 0.21
Nodes (22): autoupload_added(), link_and_sync(), refresh_albums(), PathBuf, Rc, String, Vec, sanitize_folder_name() (+14 more)

### Community 29 - "Result"
Cohesion: 0.22
Nodes (10): affected_photo_ids(), fts_query(), Library, merge_tag_into(), rebuild_photo_fts(), HashSet, Result, String (+2 more)

### Community 31 - "styleface/models.rs"
Cohesion: 0.24
Nodes (19): Response, catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry (+11 more)

### Community 32 - "immich_thumbs.rs"
Cohesion: 0.20
Nodes (13): immich_thumbs_path_for_server(), ImmichThumbs, remove_all_immich_thumb_databases(), remove_db_files(), remove_immich_thumbs_for_server(), Connection, Mutex, Option (+5 more)

### Community 33 - "Client"
Cohesion: 0.18
Nodes (11): Client, Error, GenOptions, GenResult, Display, Formatter, From, Result (+3 more)

### Community 34 - "Library"
Cohesion: 0.20
Nodes (15): character_can_belong_to_multiple_groups(), characters_under_group_covers_subtree_and_dedupes(), cleanup(), create_rename_delete_character_group(), delete_group_does_not_delete_characters(), Library, HashMap, Path (+7 more)

### Community 35 - "face/models.rs"
Cohesion: 0.25
Nodes (17): catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry, ModelKind (+9 more)

### Community 36 - "PhotoObject"
Cohesion: 0.15
Nodes (10): ObjectImpl, ObjectSubclass, Rc, Self, PhotoObject, Option, RefCell, Self (+2 more)

### Community 38 - "vrules.rs"
Cohesion: 0.20
Nodes (17): build_rule_row(), civil_from_days(), collect_rule(), date_roundtrip(), date_to_unix(), days_from_civil(), display_value(), open_rules_editor() (+9 more)

### Community 39 - "vmenu.rs"
Cohesion: 0.22
Nodes (18): album_depth(), bake_source(), build_menu(), copy_photo_to_clipboard(), CopySource, dismiss(), install_grid_context_menu(), local_photo_ids() (+10 more)

### Community 40 - ".new"
Cohesion: 0.24
Nodes (4): Arc, AtomicU64, F, Library

### Community 41 - "Prefs"
Cohesion: 0.21
Nodes (12): bool_setting(), format_sizes(), load_ai_config(), load_face_config(), load_styleface_config(), parse_sizes(), Prefs, Default (+4 more)

### Community 42 - "opencode.json"
Cohesion: 0.50
Nodes (3): plugin, $schema, .opencode/plugins/graphify.js

### Community 43 - "Library"
Cohesion: 0.21
Nodes (14): cleanup(), create_rename_delete_person_group(), delete_group_does_not_delete_persons(), Library, person_can_belong_to_multiple_groups(), persons_under_group_covers_subtree_and_dedupes(), HashMap, Path (+6 more)

### Community 44 - "enrich.rs"
Cohesion: 0.31
Nodes (17): append_ids(), enqueue_folder(), enqueue_ids(), enqueue_root(), enqueue_visible(), enrich_one(), generate_all(), Msg (+9 more)

### Community 46 - "StatusBar"
Cohesion: 0.19
Nodes (6): ProgressBar, Box, Button, Label, Rc, StatusBar

### Community 47 - "Embedder"
Cohesion: 0.30
Nodes (8): Embedder, Mutex, Result, Session, String, Vec, sample_rgb(), umeyama_similarity()

### Community 48 - "TextureCache"
Cohesion: 0.26
Nodes (6): HashMap, Option, String, Texture, Vec, TextureCache

### Community 49 - "app.rs"
Cohesion: 0.24
Nodes (14): Application, apply_theme(), build_ui(), install_css(), load_folder_into_grid(), load_raw_folder_into_grid(), populate(), populate_deferred() (+6 more)

### Community 50 - "Embedder"
Cohesion: 0.29
Nodes (7): Embedder, Mutex, Result, Session, String, Vec, sample_rgb()

### Community 51 - "Result"
Cohesion: 0.22
Nodes (7): Library, HashSet, Option, Result, Vec, ImmichFolderLink, ImmichServer

### Community 52 - "RULE THREE: versioning and named release process"
Cohesion: 0.22
Nodes (10): RULE THREE: versioning and named release process, Build and release workflow, Bump build number step, Detect documentation-only push, Named release workflow, Publish GitHub release step, Push filtered snapshot to GitHub step, GitHub mirror pushes filtered snapshot, not full history (+2 more)

### Community 53 - "db/config.rs"
Cohesion: 0.56
Nodes (9): config_path(), data_dir(), default_data_dir(), home_dir(), read_configured_data_dir(), Option, PathBuf, Result (+1 more)

### Community 54 - "actions.rs"
Cohesion: 0.42
Nodes (10): add_library_folder(), enqueue_scan(), find_duplicates(), Msg, rescan_all(), resume_scan(), Rc, String (+2 more)

### Community 56 - "FacesView"
Cohesion: 0.14
Nodes (28): assign_photos_to_character_dialog(), assign_style_cluster_to_character(), assign_style_clusters_to_character(), assign_style_face_dialog(), assign_style_faces_per_face_dialog(), assign_style_faces_to_character(), name_style_clusters(), name_style_clusters_dialog() (+20 more)

### Community 57 - "name_cluster_dialog"
Cohesion: 0.50
Nodes (8): assign_cluster_to_person(), assign_face_dialog(), name_cluster(), name_cluster_dialog(), F, Rc, Result, String

### Community 58 - "Config"
Cohesion: 0.38
Nodes (4): Config, normalize_fills_defaults_and_clamps(), Default, String

### Community 59 - "parse_log_level"
Cohesion: 0.46
Nodes (7): LevelFilter, init_logging(), main(), parse_log_level(), print_usage(), ExitCode, Result

### Community 60 - "Result"
Cohesion: 0.24
Nodes (4): Library, HashSet, Result, Vec

### Community 61 - "inference_test.rs"
Cohesion: 0.43
Nodes (7): cosine(), data_dir(), face_pipeline_real_inference(), init(), load_rgb(), PathBuf, Vec

### Community 62 - "FolderTree"
Cohesion: 0.24
Nodes (10): base_name(), FolderTree, GtkBox, HashSet, Rc, RefCell, String, StringList (+2 more)

### Community 63 - "HANDOFF.md"
Cohesion: 0.29
Nodes (4): RULE TWO: hand off before context is too large, Fast two-phase import and library freshness (HANDOFF), Fast two-phase import (README), Library freshness (README)

### Community 64 - "Human facial detection and recognition system"
Cohesion: 0.38
Nodes (7): ONNX Runtime download-on-demand design, CCIP embedder swap for stylised faces, Human facial detection and recognition system, Stylised face recognition and album Face type, Facial recognition, local and optional (README), People vs. Characters: two recognition pipelines (README), Facial detection & recognition (ROADMAP)

### Community 65 - "parse_tags"
Cohesion: 0.43
Nodes (5): clean_tag(), parse_tags(), parse_tags_cases(), String, Vec

### Community 66 - "Controller"
Cohesion: 0.24
Nodes (5): Controller, Arc, AtomicBool, Mutex, Option

### Community 67 - "immich_pane"
Cohesion: 0.53
Nodes (5): immich_pane(), labeled_entry(), Entry, GtkBox, Rc

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

### Community 77 - "toolbar.rs"
Cohesion: 0.44
Nodes (8): build_toolbar(), compact_button(), Button, GtkBox, Rc, start_slideshow(), start_slideshow_from_prefs(), toggle_properties()

### Community 109 - "FaceConfig"
Cohesion: 0.25
Nodes (4): FaceConfig, Default, Self, String

### Community 110 - "StyleFaceConfig"
Cohesion: 0.25
Nodes (4): Default, Self, String, StyleFaceConfig

### Community 111 - "settings_characters.rs"
Cohesion: 0.70
Nodes (4): characters_pane(), delete_all(), GtkBox, Rc

### Community 112 - "spin_row"
Cohesion: 0.57
Nodes (6): ai_pane(), fixed_label(), GtkBox, Label, Rc, spin_row()

### Community 113 - "build_membership_sql"
Cohesion: 0.70
Nodes (5): build_membership_sql(), Box, String, rule_clause(), ToSql

### Community 115 - "settings_faces.rs"
Cohesion: 0.70
Nodes (4): delete_all_face_data(), faces_pane(), GtkBox, Rc

## Knowledge Gaps
- **36 isolated node(s):** `$schema`, `.opencode/plugins/graphify.js`, `pichouse`, `Msg`, `graphify` (+31 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **37 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `AppState` to `Sidebar`, `Generator`, `EditPanel`, `Viewer`, `stylefacescan.rs`, `PhotoEdit`, `Library`, `db/mod.rs`, `facescan.rs`, `Properties`, `reconcile.rs`, `CharactersView`, `settings.rs`, `Grid`, `NewFilesView`, `Manager`, `dedup.rs`, `model.rs`, `show_message`, `ui/immich.rs`, `Rc`, `vrules.rs`, `vmenu.rs`, `Prefs`, `enrich.rs`, `StatusBar`, `app.rs`, `actions.rs`, `FacesView`, `name_cluster_dialog`, `Config`, `FolderTree`, `Controller`, `immich_pane`, `toolbar.rs`, `FaceConfig`, `StyleFaceConfig`, `settings_characters.rs`, `spin_row`, `settings_faces.rs`?**
  _High betweenness centrality (0.333) - this node is a cross-community bridge._
- **Why does `Photo` connect `Photo` to `Library`, `EditPanel`, `Viewer`, `PhotoEdit`, `scan.rs`, `Library`, `Library`, `AppState`, `Properties`, `Grid`, `NewFilesView`, `grid.rs`, `dedup.rs`, `Library`, `model.rs`, `show_message`, `Library`, `ui/immich.rs`, `PhotoObject`, `vmenu.rs`, `Result`?**
  _High betweenness centrality (0.174) - this node is a cross-community bridge._
- **Why does `Grid` connect `Grid` to `PhotoObject`, `Photo`, `.set_sort_order`, `vmenu.rs`, `.new`, `AppState`, `TextureCache`, `grid.rs`, `Rc`?**
  _High betweenness centrality (0.085) - this node is a cross-community bridge._
- **Are the 68 inferred relationships involving `show_error()` (e.g. with `add_library_folder()` and `rescan_all()`) actually correct?**
  _`show_error()` has 68 INFERRED edges - model-reasoned connections that need verification._
- **What connects `$schema`, `.opencode/plugins/graphify.js`, `pichouse` to the rest of the system?**
  _36 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Sidebar` be split into smaller, more focused modules?**
  _Cohesion score 0.07650273224043716 - nodes in this community are weakly interconnected._
- **Should `Generator` be split into smaller, more focused modules?**
  _Cohesion score 0.0629399585921325 - nodes in this community are weakly interconnected._