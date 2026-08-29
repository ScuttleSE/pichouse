# Graph Report - pichouse  (2026-08-29)

## Corpus Check
- 97 files · ~122,496 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1756 nodes · 4298 edges · 109 communities (75 shown, 34 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 117 edges (avg confidence: 0.86)
- Token cost: 111,082 input · 0 output

## Community Hubs (Navigation)
- Library Sidebar Tree
- Thumbnail Generation & Cache
- Library Folder Database
- Photo Editor UI
- Photo Viewer UI
- ONNX Runtime Download
- Photo Edits & Export
- Filesystem Structure Scan
- Face Database Storage
- Style Face Database
- Face Thumbnail Cache
- Characters Settings UI
- Face Detector
- Immich API Client
- Characters View UI
- Library Reconciliation
- Photo Properties Panel
- Settings & Shortcuts UI
- Thumbnail Grid View
- Face Clustering Algorithm
- New Files View
- Grid Texture Rendering
- AI Tagging Config
- Duplicate Photo Finder
- Virtual Albums Database
- Core Data Model
- AI Auto-Tagging Scan
- Albums Database
- Immich Sync UI
- Tags Database (FTS)
- AppState View Accessors
- Style Face Model Catalog
- Virtual Album Rules UI
- Ollama AI Client
- Immich Thumbnail Cache
- Face Model Catalog
- Photo Status Enums
- Grid Photo Loading
- Faces View UI
- Virtual Album Menu
- Grid Cell Interaction
- Preferences Persistence
- Application Bootstrap
- Immich Servers Database
- Enrichment Worker Queue
- Raw Filesystem Folder Tree
- Status Bar UI
- Face Embedder
- Thumbnail Memory Cache
- Photo GObject Wrapper
- Style Face Embedder
- Character Cluster Assignment
- CI Release Pipeline
- Data Directory Config
- Scan Actions & Queue
- Background Job Controller
- Level Presets Database
- Person Cluster Assignment
- Toolbar UI
- Main Entry & Logging
- Face Recognition Config
- Face Inference Test
- Style Face Config
- Two-Phase Import (Docs)
- Face Recognition Docs
- AI Tag Parsing
- Duplicates Database
- Immich Settings UI
- Virtual Album SQL Builder
- Duplicate Finder (Docs)
- Immich Integration (Docs)
- Non-Destructive Editing (Docs)
- Planned UI Features
- Tech Stack (Docs)
- License (Docs)
- Virtual Albums (Docs)
- Color Levels (Docs)
- Prefs Size Formatting
- Background HTTP Pattern
- CI Description (Docs)
- DB Migration Pattern
- Grid Entry Points Pattern
- Project Description (Docs)
- Debug Cache Cleanup Rule
- Commit & Push Rule
- No-Speculation Rule
- ASD-STE100 Communication Rule
- Answer-First Rule
- Diagnose-With-Data Rule
- Settings Key-Value Pattern
- Sidebar Architecture Pattern
- Rolling Pre-Release Step
- Gitea Release Step
- Version Sync Step
- Parse Version From Tag
- Context-Menu Fixes (Docs)
- Bug Fixes Handoff (Docs)
- Release Prep Handoff (Docs)
- Cargo Package Root
- AI Tagging (Docs)
- Baked Export (Docs)
- New Files View (Docs)
- Mature Content Tagging (Planned)
- Geolocation & Maps (Planned)
- Picasa-Style Sidebar (Planned)
- RAW + JPEG Pairing (Planned)

## God Nodes (most connected - your core abstractions)
1. `AppState` - 202 edges
2. `Grid` - 83 edges
3. `Sidebar` - 68 edges
4. `Photo` - 65 edges
5. `show_error()` - 56 edges
6. `Viewer` - 53 edges
7. `EditPanel` - 50 edges
8. `Library` - 43 edges
9. `Library` - 36 edges
10. `Library` - 32 edges

## Surprising Connections (you probably didn't know these)
- `Releases section (README)` --semantically_similar_to--> `Named release workflow`  [INFERRED] [semantically similar]
  README.md → .gitea/workflows/release.yaml
- `pichouse tech stack (README)` --semantically_similar_to--> `pichouse tech stack (AGENTS.md)`  [INFERRED] [semantically similar]
  README.md → AGENTS.md
- `Duplicate image finder (README)` --semantically_similar_to--> `Duplicate image finder (HANDOFF)`  [INFERRED] [semantically similar]
  README.md → HANDOFF.md
- `Duplicate image finder (ROADMAP)` --semantically_similar_to--> `Duplicate image finder (HANDOFF)`  [INFERRED] [semantically similar]
  ROADMAP.md → HANDOFF.md
- `People vs. Characters: two recognition pipelines (README)` --semantically_similar_to--> `Stylised face recognition and album Face type`  [INFERRED] [semantically similar]
  README.md → HANDOFF.md

## Import Cycles
- 2-file cycle: `src/ui/properties.rs -> src/ui/state.rs -> src/ui/properties.rs`
- 2-file cycle: `src/ui/state.rs -> src/ui/status.rs -> src/ui/state.rs`
- 2-file cycle: `src/ui/state.rs -> src/ui/viewer.rs -> src/ui/state.rs`
- 3-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/properties.rs -> src/ui/editor.rs`
- 4-file cycle: `src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/state.rs -> src/ui/grid.rs`
- 5-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/editor.rs`

## Hyperedges (group relationships)
- **CI release pipeline: rolling build, named release, versioning rule** — gitea_workflows_build_build_and_release_workflow, gitea_workflows_release_named_release_workflow, agents_rule_three [INFERRED 0.85]
- **Dual face-recognition pipelines: human People and stylised Characters** — handoff_stylised_face_recognition, handoff_facial_detection_recognition, readme_people_vs_characters, roadmap_facial_detection_recognition [INFERRED 0.85]
- **Non-destructive editing feature documented across README, ROADMAP, and HANDOFF** — readme_non_destructive_editing, roadmap_non_destructive_editing, handoff_non_destructive_editing [INFERRED 0.80]

## Communities (109 total, 34 thin omitted)

### Community 0 - "Library Sidebar Tree"
Cohesion: 0.08
Nodes (42): ListItem, ListView, Propagation, confirm(), prompt_text(), F, Option, Rc (+34 more)

### Community 1 - "Thumbnail Generation & Cache"
Cohesion: 0.06
Nodes (44): ImageError, remove_all_thumb_databases(), Connection, Mutex, Option, P, PathBuf, Result (+36 more)

### Community 2 - "Library Folder Database"
Cohesion: 0.09
Nodes (24): MutexGuard, enrichment_under_root_is_scoped_by_prefix(), Library, map_photo(), migrate(), new_files_respects_first_scan_boundary(), now(), Connection (+16 more)

### Community 3 - "Photo Editor UI"
Cohesion: 0.12
Nodes (30): CheckButton, Context, Scale, SpinButton, channel_vals(), ChannelWidgets, Controls, draw_triangle() (+22 more)

### Community 4 - "Photo Viewer UI"
Cohesion: 0.11
Nodes (25): CropPermille, Picture, Pixbuf, SourceId, decode_edited(), decode_pixbuf(), immich_server_for(), pixbuf_to_rgba() (+17 more)

### Community 5 - "ONNX Runtime Download"
Cohesion: 0.07
Nodes (47): download_and_extract(), ensure_runtime(), ensure_runtime_progress(), extract_so_from_tgz(), init_runtime(), Fn, Path, PathBuf (+39 more)

### Community 6 - "Photo Edits & Export"
Cohesion: 0.09
Nodes (41): Rgba, Library, map_edit(), Result, Row, String, Vec, apply_brightness_contrast() (+33 more)

### Community 7 - "Filesystem Structure Scan"
Cohesion: 0.10
Nodes (41): Error, Exif, FnMut, G, civil_to_unix(), civil_unix_year_roundtrip(), dimensions(), enrich_file() (+33 more)

### Community 8 - "Face Database Storage"
Cohesion: 0.11
Nodes (18): add_photo(), blob_to_floats(), delete_all_clears_everything(), face_roundtrip_preserves_embedding(), face_scan_state_gates_needing_list(), floats_to_blob(), Library, map_face() (+10 more)

### Community 9 - "Style Face Database"
Cohesion: 0.09
Nodes (12): blob_to_floats(), floats_to_blob(), Library, map_character(), map_style_face(), HashMap, Option, Result (+4 more)

### Community 10 - "Face Thumbnail Cache"
Cohesion: 0.07
Nodes (35): face_thumbs_path(), FaceThumbs, remove_face_thumbs_database(), Connection, Mutex, Option, P, PathBuf (+27 more)

### Community 11 - "Characters Settings UI"
Cohesion: 0.09
Nodes (22): ApplicationWindow, characters_pane(), delete_all(), GtkBox, Rc, delete_all_face_data(), faces_pane(), GtkBox (+14 more)

### Community 12 - "Face Detector"
Cohesion: 0.09
Nodes (33): Cand, Detector, iou(), nms(), Mutex, Result, Session, String (+25 more)

### Community 13 - "Immich API Client"
Cohesion: 0.11
Nodes (19): civil_from_days(), civil_to_unix(), Client, Error, parse_rfc3339_seconds(), parse_taken_at(), Display, Formatter (+11 more)

### Community 14 - "Characters View UI"
Cohesion: 0.16
Nodes (22): Condvar, CharactersView, crop_pool(), CropJob, CropPool, queue_crop_job(), Arc, FlowBox (+14 more)

### Community 15 - "Library Reconciliation"
Cohesion: 0.14
Nodes (31): collect(), DbSnapshot, dir_has_images(), mtime_secs(), PhotoInsert, PhotoMove, plan_dir(), plan_vanished_dirs() (+23 more)

### Community 16 - "Photo Properties Panel"
Cohesion: 0.14
Nodes (18): Notebook, bold_label(), field(), Properties, Button, Entry, GtkBox, Label (+10 more)

### Community 17 - "Settings & Shortcuts UI"
Cohesion: 0.15
Nodes (23): action_key(), appearance_pane(), capture_shortcut(), folder_pane(), pane_box(), GtkBox, Label, Rc (+15 more)

### Community 18 - "Thumbnail Grid View"
Cohesion: 0.10
Nodes (12): GridView, ListStore, SignalHandlerId, Grid, Box, Button, DropDown, Fn (+4 more)

### Community 19 - "Face Clustering Algorithm"
Cohesion: 0.12
Nodes (23): Center, accumulate(), cluster(), ClusterAssignment, ClusterItem, cosine_similarity(), mean(), named_person_anchors_a_stable_cluster() (+15 more)

### Community 20 - "New Files View"
Cohesion: 0.11
Nodes (21): decode_texture(), Done, Job, NewFilesView, Arc, AtomicU64, Box, Cell (+13 more)

### Community 21 - "Grid Texture Rendering"
Cohesion: 0.11
Nodes (23): Overlay, SignalListItemFactory, apply_texture(), build_factory(), decode_texture(), Done, DupCellUi, DupGroupUi (+15 more)

### Community 22 - "AI Tagging Config"
Cohesion: 0.11
Nodes (17): Child, Config, normalize_fills_defaults_and_clamps(), Default, String, Manager, Client, Drop (+9 more)

### Community 23 - "Duplicate Photo Finder"
Cohesion: 0.15
Nodes (19): choose_keep(), DupGroup, exact_hash_groups(), find_duplicates(), format_rank(), keep_prefers_larger_then_lossless(), AtomicBool, Self (+11 more)

### Community 24 - "Virtual Albums Database"
Cohesion: 0.26
Nodes (13): cleanup(), crud_nesting_and_cycle(), Library, manual_membership_roundtrip(), mk_photo(), person_rule_matches_photos_of_that_person(), pin_and_exclusion_over_rules(), PathBuf (+5 more)

### Community 25 - "Core Data Model"
Cohesion: 0.12
Nodes (15): Album, AlbumKind, Character, ImmichAlbum, ImmichFolderLink, ImmichServer, LevelPreset, LibraryFolder (+7 more)

### Community 26 - "AI Auto-Tagging Scan"
Cohesion: 0.15
Nodes (21): ai_tag_folder(), ai_tag_library(), Msg, Arc, AtomicBool, Client, Library, Rc (+13 more)

### Community 27 - "Albums Database"
Cohesion: 0.20
Nodes (9): album_kind_inherits_down_the_tree(), folders_under_album_covers_subtree(), Library, remove_library_folder_then_use_albums(), HashMap, PathBuf, Result, Vec (+1 more)

### Community 28 - "Immich Sync UI"
Cohesion: 0.21
Nodes (22): autoupload_added(), link_and_sync(), refresh_albums(), PathBuf, Rc, String, Vec, sanitize_folder_name() (+14 more)

### Community 29 - "Tags Database (FTS)"
Cohesion: 0.22
Nodes (10): affected_photo_ids(), fts_query(), Library, merge_tag_into(), rebuild_photo_fts(), HashSet, Result, String (+2 more)

### Community 31 - "Style Face Model Catalog"
Cohesion: 0.24
Nodes (19): Response, catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry (+11 more)

### Community 32 - "Virtual Album Rules UI"
Cohesion: 0.20
Nodes (18): VirtualRule, build_rule_row(), civil_from_days(), collect_rule(), date_roundtrip(), date_to_unix(), days_from_civil(), display_value() (+10 more)

### Community 33 - "Ollama AI Client"
Cohesion: 0.18
Nodes (11): Client, Error, GenOptions, GenResult, Display, Formatter, From, Result (+3 more)

### Community 34 - "Immich Thumbnail Cache"
Cohesion: 0.20
Nodes (13): immich_thumbs_path_for_server(), ImmichThumbs, remove_all_immich_thumb_databases(), remove_db_files(), remove_immich_thumbs_for_server(), Connection, Mutex, Option (+5 more)

### Community 35 - "Face Model Catalog"
Cohesion: 0.25
Nodes (17): catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry, ModelKind (+9 more)

### Community 36 - "Photo Status Enums"
Cohesion: 0.11
Nodes (6): AiStatus, PhotoScanState, Self, RuleField, RuleOp, ScanStatus

### Community 37 - "Grid Photo Loading"
Cohesion: 0.24
Nodes (3): Photo, cell_key(), Vec

### Community 38 - "Faces View UI"
Cohesion: 0.21
Nodes (11): FacesView, FlowBox, GtkBox, Label, Option, Rc, RefCell, Self (+3 more)

### Community 39 - "Virtual Album Menu"
Cohesion: 0.23
Nodes (16): album_depth(), bake_source(), build_menu(), copy_photo_to_clipboard(), CopySource, dismiss(), install_grid_context_menu(), local_photo_ids() (+8 more)

### Community 40 - "Grid Cell Interaction"
Cohesion: 0.17
Nodes (6): Arc, AtomicU64, F, Library, Rc, Self

### Community 41 - "Preferences Persistence"
Cohesion: 0.25
Nodes (10): bool_setting(), load_ai_config(), load_face_config(), load_styleface_config(), parse_sizes(), Prefs, Default, Library (+2 more)

### Community 42 - "Application Bootstrap"
Cohesion: 0.26
Nodes (13): Application, apply_theme(), build_ui(), install_css(), load_folder_into_grid(), load_raw_folder_into_grid(), populate(), populate_deferred() (+5 more)

### Community 43 - "Immich Servers Database"
Cohesion: 0.22
Nodes (5): Library, HashSet, Option, Result, Vec

### Community 44 - "Enrichment Worker Queue"
Cohesion: 0.41
Nodes (13): append_ids(), enqueue(), enqueue_root(), enrich_one(), ensure_running(), Msg, postponed(), prioritize_folder() (+5 more)

### Community 45 - "Raw Filesystem Folder Tree"
Cohesion: 0.24
Nodes (10): base_name(), FolderTree, GtkBox, HashSet, Rc, RefCell, String, StringList (+2 more)

### Community 46 - "Status Bar UI"
Cohesion: 0.20
Nodes (6): ProgressBar, Box, Button, Label, Rc, StatusBar

### Community 47 - "Face Embedder"
Cohesion: 0.30
Nodes (8): Embedder, Mutex, Result, Session, String, Vec, sample_rgb(), umeyama_similarity()

### Community 48 - "Thumbnail Memory Cache"
Cohesion: 0.26
Nodes (6): HashMap, Option, String, Texture, Vec, TextureCache

### Community 49 - "Photo GObject Wrapper"
Cohesion: 0.20
Nodes (8): ObjectImpl, ObjectSubclass, PhotoObject, Option, RefCell, Self, String, Texture

### Community 50 - "Style Face Embedder"
Cohesion: 0.29
Nodes (7): Embedder, Mutex, Result, Session, String, Vec, sample_rgb()

### Community 51 - "Character Cluster Assignment"
Cohesion: 0.44
Nodes (10): assign_style_cluster_to_character(), assign_style_clusters_to_character(), assign_style_face_dialog(), name_style_clusters(), name_style_clusters_dialog(), F, Rc, Result (+2 more)

### Community 52 - "CI Release Pipeline"
Cohesion: 0.22
Nodes (10): RULE THREE: versioning and named release process, Build and release workflow, Bump build number step, Detect documentation-only push, Named release workflow, Publish GitHub release step, Push filtered snapshot to GitHub step, GitHub mirror pushes filtered snapshot, not full history (+2 more)

### Community 53 - "Data Directory Config"
Cohesion: 0.56
Nodes (9): config_path(), data_dir(), default_data_dir(), home_dir(), read_configured_data_dir(), Option, PathBuf, Result (+1 more)

### Community 54 - "Scan Actions & Queue"
Cohesion: 0.40
Nodes (9): add_library_folder(), enqueue_scan(), find_duplicates(), Msg, rescan_all(), Rc, String, Vec (+1 more)

### Community 55 - "Background Job Controller"
Cohesion: 0.24
Nodes (5): Controller, Arc, AtomicBool, Mutex, Option

### Community 56 - "Level Presets Database"
Cohesion: 0.28
Nodes (5): Library, map_preset(), Result, Row, Vec

### Community 57 - "Person Cluster Assignment"
Cohesion: 0.50
Nodes (8): assign_cluster_to_person(), assign_face_dialog(), name_cluster(), name_cluster_dialog(), F, Rc, Result, String

### Community 58 - "Toolbar UI"
Cohesion: 0.44
Nodes (8): build_toolbar(), compact_button(), Button, GtkBox, Rc, start_slideshow(), start_slideshow_from_prefs(), toggle_properties()

### Community 59 - "Main Entry & Logging"
Cohesion: 0.46
Nodes (7): LevelFilter, init_logging(), main(), parse_log_level(), print_usage(), ExitCode, Result

### Community 60 - "Face Recognition Config"
Cohesion: 0.25
Nodes (4): FaceConfig, Default, Self, String

### Community 61 - "Face Inference Test"
Cohesion: 0.43
Nodes (7): cosine(), data_dir(), face_pipeline_real_inference(), init(), load_rgb(), PathBuf, Vec

### Community 62 - "Style Face Config"
Cohesion: 0.25
Nodes (4): Default, Self, String, StyleFaceConfig

### Community 63 - "Two-Phase Import (Docs)"
Cohesion: 0.29
Nodes (4): RULE TWO: hand off before context is too large, Fast two-phase import and library freshness (HANDOFF), Fast two-phase import (README), Library freshness (README)

### Community 64 - "Face Recognition Docs"
Cohesion: 0.38
Nodes (7): ONNX Runtime download-on-demand design, CCIP embedder swap for stylised faces, Human facial detection and recognition system, Stylised face recognition and album Face type, Facial recognition, local and optional (README), People vs. Characters: two recognition pipelines (README), Facial detection & recognition (ROADMAP)

### Community 65 - "AI Tag Parsing"
Cohesion: 0.43
Nodes (5): clean_tag(), parse_tags(), parse_tags_cases(), String, Vec

### Community 66 - "Duplicates Database"
Cohesion: 0.38
Nodes (3): Library, Result, Vec

### Community 67 - "Immich Settings UI"
Cohesion: 0.53
Nodes (5): immich_pane(), labeled_entry(), Entry, GtkBox, Rc

### Community 68 - "Virtual Album SQL Builder"
Cohesion: 0.70
Nodes (5): build_membership_sql(), Box, String, rule_clause(), ToSql

### Community 69 - "Duplicate Finder (Docs)"
Cohesion: 1.00
Nodes (3): Duplicate image finder (HANDOFF), Duplicate image finder (README), Duplicate image finder (ROADMAP)

### Community 70 - "Immich Integration (Docs)"
Cohesion: 1.00
Nodes (3): Immich integration (HANDOFF), Immich integration (README), Immich integration (ROADMAP, phased)

### Community 71 - "Non-Destructive Editing (Docs)"
Cohesion: 1.00
Nodes (3): Non-destructive editing and color levels (HANDOFF), Non-destructive editing (README), Non-destructive image editing (ROADMAP)

### Community 72 - "Planned UI Features"
Cohesion: 0.67
Nodes (3): Timeline, copy, crop overlay, slideshows, logging, freeze fixes, Move to llama.cpp instead of Ollama (planned), Slideshows (ROADMAP)

## Knowledge Gaps
- **33 isolated node(s):** `pichouse`, `Msg`, `Build and release workflow`, `Detect documentation-only push`, `Publish rolling pre-release` (+28 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **34 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `Characters Settings UI` to `Library Sidebar Tree`, `Thumbnail Generation & Cache`, `Photo Editor UI`, `Photo Viewer UI`, `ONNX Runtime Download`, `Photo Edits & Export`, `Face Thumbnail Cache`, `Face Detector`, `Characters View UI`, `Library Reconciliation`, `Photo Properties Panel`, `Settings & Shortcuts UI`, `Thumbnail Grid View`, `New Files View`, `AI Tagging Config`, `Duplicate Photo Finder`, `Core Data Model`, `AI Auto-Tagging Scan`, `Immich Sync UI`, `AppState View Accessors`, `Virtual Album Rules UI`, `Faces View UI`, `Virtual Album Menu`, `Preferences Persistence`, `Application Bootstrap`, `Enrichment Worker Queue`, `Raw Filesystem Folder Tree`, `Status Bar UI`, `Character Cluster Assignment`, `Scan Actions & Queue`, `Background Job Controller`, `Person Cluster Assignment`, `Toolbar UI`, `Face Recognition Config`, `Style Face Config`, `Immich Settings UI`?**
  _High betweenness centrality (0.354) - this node is a cross-community bridge._
- **Why does `Photo` connect `Grid Photo Loading` to `Library Folder Database`, `Photo Editor UI`, `Photo Viewer UI`, `Photo Edits & Export`, `Filesystem Structure Scan`, `Face Database Storage`, `Style Face Database`, `Characters Settings UI`, `Photo Properties Panel`, `Thumbnail Grid View`, `New Files View`, `Grid Texture Rendering`, `Duplicate Photo Finder`, `Virtual Albums Database`, `Core Data Model`, `AI Auto-Tagging Scan`, `Albums Database`, `Immich Sync UI`, `Photo Status Enums`, `Virtual Album Menu`, `Grid Cell Interaction`, `Photo GObject Wrapper`, `Duplicates Database`?**
  _High betweenness centrality (0.174) - this node is a cross-community bridge._
- **Why does `Grid` connect `Thumbnail Grid View` to `Grid Photo Loading`, `Virtual Album Menu`, `Grid Cell Interaction`, `Characters Settings UI`, `Thumbnail Memory Cache`, `Photo GObject Wrapper`, `Grid Texture Rendering`, `AI Auto-Tagging Scan`, `AppState View Accessors`?**
  _High betweenness centrality (0.075) - this node is a cross-community bridge._
- **Are the 52 inferred relationships involving `show_error()` (e.g. with `add_library_folder()` and `rescan_all()`) actually correct?**
  _`show_error()` has 52 INFERRED edges - model-reasoned connections that need verification._
- **What connects `pichouse`, `Msg`, `Build and release workflow` to the rest of the system?**
  _33 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Library Sidebar Tree` be split into smaller, more focused modules?**
  _Cohesion score 0.08247422680412371 - nodes in this community are weakly interconnected._
- **Should `Thumbnail Generation & Cache` be split into smaller, more focused modules?**
  _Cohesion score 0.0629399585921325 - nodes in this community are weakly interconnected._