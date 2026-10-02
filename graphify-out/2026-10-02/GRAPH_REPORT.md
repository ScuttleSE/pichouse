# Graph Report - pichouse  (2026-10-02)

## Corpus Check
- 103 files · ~164,380 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2097 nodes · 5549 edges · 122 communities (86 shown, 36 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 152 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `ba26b344`
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
- Result
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
- styleface/cluster.rs
- NewFilesView
- app.rs
- Config
- dedup.rs
- vrules.rs
- Photo
- show_message
- Library
- ui/immich.rs
- Result
- Rc
- styleface/models.rs
- FolderTree
- Client
- Library
- face/models.rs
- Controller
- Result
- PhotoObject
- vmenu.rs
- build_factory
- Prefs
- opencode.json
- Library
- enrich.rs
- graphify.js
- StatusBar
- Embedder
- TextureCache
- characters.rs
- Embedder
- Result
- RULE THREE: versioning and named release process
- db/config.rs
- actions.rs
- CLAUDE.md
- FacesView
- name_cluster_dialog
- FaceConfig
- parse_log_level
- face/cluster.rs
- inference_test.rs
- edit.rs
- HANDOFF.md
- Human facial detection and recognition system
- parse_tags
- freshness.rs
- grid.rs
- immich_pane
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
- nftree.rs
- model.rs
- DiskAlbumMapper
- .show_duplicates
- styleface/detector.rs
- face/detector.rs
- settings_characters.rs
- settings_faces.rs
- .new
- Self
- FacePipeline
- StyleFacePipeline
- spin_row

## God Nodes (most connected - your core abstractions)
1. `AppState` - 239 edges
2. `Grid` - 113 edges
3. `Sidebar` - 95 edges
4. `Photo` - 73 edges
5. `show_error()` - 72 edges
6. `Library` - 54 edges
7. `Viewer` - 53 edges
8. `EditPanel` - 50 edges
9. `Library` - 45 edges
10. `Library` - 43 edges

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
- 2-file cycle: `src/ui/state.rs -> src/ui/viewer.rs -> src/ui/state.rs`
- 2-file cycle: `src/ui/state.rs -> src/ui/status.rs -> src/ui/state.rs`
- 2-file cycle: `src/ui/properties.rs -> src/ui/state.rs -> src/ui/properties.rs`
- 3-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/properties.rs -> src/ui/editor.rs`
- 4-file cycle: `src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/state.rs -> src/ui/grid.rs`
- 5-file cycle: `src/ui/editor.rs -> src/ui/state.rs -> src/ui/grid.rs -> src/ui/photo_object.rs -> src/ui/properties.rs -> src/ui/editor.rs`

## Hyperedges (group relationships)
- **Non-destructive editing feature documented across README, ROADMAP, and HANDOFF** — readme_non_destructive_editing, roadmap_non_destructive_editing, handoff_non_destructive_editing [INFERRED 0.80]
- **CI release pipeline: rolling build, named release, versioning rule** — gitea_workflows_build_build_and_release_workflow, gitea_workflows_release_named_release_workflow, agents_rule_three [INFERRED 0.85]
- **Dual face-recognition pipelines: human People and stylised Characters** — handoff_stylised_face_recognition, handoff_facial_detection_recognition, readme_people_vs_characters, roadmap_facial_detection_recognition [INFERRED 0.85]

## Communities (122 total, 36 thin omitted)

### Community 0 - "Sidebar"
Cohesion: 0.07
Nodes (47): ListItem, ListView, Propagation, confirm(), prompt_text(), F, Option, Rc (+39 more)

### Community 1 - "Generator"
Cohesion: 0.06
Nodes (44): ImageError, remove_all_thumb_databases(), Connection, Mutex, Option, P, PathBuf, Result (+36 more)

### Community 2 - "Library"
Cohesion: 0.09
Nodes (22): MutexGuard, CountCache, enrichment_in_is_scoped_to_folder_set(), enrichment_under_root_is_scoped_by_prefix(), Library, map_photo(), migrate(), new_files_respects_first_scan_boundary() (+14 more)

### Community 3 - "EditPanel"
Cohesion: 0.09
Nodes (37): SpinButton, Library, map_preset(), Result, Row, Vec, LevelPreset, Levels (+29 more)

### Community 4 - "Viewer"
Cohesion: 0.11
Nodes (25): CropPermille, Picture, Pixbuf, SourceId, decode_edited(), decode_pixbuf(), immich_server_for(), pixbuf_to_rgba() (+17 more)

### Community 5 - "stylefacescan.rs"
Cohesion: 0.09
Nodes (36): download_and_extract(), ensure_runtime(), ensure_runtime_progress(), extract_so_from_tgz(), init_runtime(), Fn, Path, PathBuf (+28 more)

### Community 6 - "PhotoEdit"
Cohesion: 0.12
Nodes (25): Library, map_edit(), HashMap, Result, Row, String, Vec, PhotoEdit (+17 more)

### Community 7 - "scan.rs"
Cohesion: 0.09
Nodes (44): Error, Exif, FnMut, civil_to_unix(), civil_unix_year_roundtrip(), dimensions(), enrich_file(), enrich_file_with_image() (+36 more)

### Community 8 - "Result"
Cohesion: 0.08
Nodes (34): add_photo(), blob_to_floats(), counts_are_photos_not_faces(), cover_if_unset_fills_gap_but_not_an_existing_choice(), delete_all_clears_everything(), face_roundtrip_preserves_embedding(), face_scan_marks_count_either_scan(), face_scan_state_gates_needing_list() (+26 more)

### Community 9 - "Library"
Cohesion: 0.08
Nodes (18): add_photo(), blob_to_floats(), counts_are_photos_not_faces(), cover_if_unset_fills_gap_but_not_an_existing_choice(), floats_to_blob(), Library, map_character(), map_style_face() (+10 more)

### Community 10 - "db/mod.rs"
Cohesion: 0.05
Nodes (48): face_thumbs_path(), FaceThumbs, remove_face_thumbs_database(), Connection, Mutex, Option, P, PathBuf (+40 more)

### Community 11 - "AppState"
Cohesion: 0.09
Nodes (26): ApplicationWindow, Condvar, SimpleAction, AppState, crop_pool(), CropJob, CropPool, now_millis() (+18 more)

### Community 12 - "facescan.rs"
Cohesion: 0.16
Nodes (23): FaceGroup, a_group_that_lost_its_photos_counts_zero_new(), counts_only_photos_added_to_a_pre_existing_group(), download_models(), fail(), Msg, new_photo_counts(), recluster() (+15 more)

### Community 13 - "Client"
Cohesion: 0.11
Nodes (19): civil_from_days(), civil_to_unix(), Client, Error, parse_rfc3339_seconds(), parse_taken_at(), Display, Formatter (+11 more)

### Community 14 - "Properties"
Cohesion: 0.14
Nodes (18): Notebook, bold_label(), field(), Properties, Button, Entry, GtkBox, Label (+10 more)

### Community 15 - "reconcile.rs"
Cohesion: 0.19
Nodes (26): collect(), DbSnapshot, dir_has_images(), mtime_secs(), PhotoInsert, PhotoMove, plan_dir(), plan_vanished_dirs() (+18 more)

### Community 16 - "CharactersView"
Cohesion: 0.22
Nodes (13): CharactersView, Button, FlowBox, GtkBox, Label, Option, Rc, RefCell (+5 more)

### Community 17 - "settings.rs"
Cohesion: 0.15
Nodes (23): action_key(), appearance_pane(), capture_shortcut(), folder_pane(), pane_box(), GtkBox, Label, Rc (+15 more)

### Community 18 - "Grid"
Cohesion: 0.09
Nodes (14): GridView, ListStore, SignalHandlerId, Grid, Box, Button, DrawingArea, DropDown (+6 more)

### Community 19 - "styleface/cluster.rs"
Cohesion: 0.17
Nodes (23): Center, _center_ref(), centroid(), chain_does_not_link_two_groups(), character_anchors_stable_cluster(), cluster(), ClusterAssignment, ClusterItem (+15 more)

### Community 20 - "NewFilesView"
Cohesion: 0.11
Nodes (24): ControlFlow, BuildState, decode_pixels(), Done, FolderRange, Job, NewFilesView, Arc (+16 more)

### Community 21 - "app.rs"
Cohesion: 0.24
Nodes (14): Application, apply_theme(), build_ui(), install_css(), load_folder_into_grid(), load_raw_folder_into_grid(), populate(), populate_deferred() (+6 more)

### Community 22 - "Config"
Cohesion: 0.14
Nodes (11): Child, Config, normalize_fills_defaults_and_clamps(), Default, String, Manager, Client, Drop (+3 more)

### Community 23 - "dedup.rs"
Cohesion: 0.14
Nodes (22): banned_pair_is_not_grouped(), choose_keep(), DupGroup, exact_hash_groups(), find_duplicates(), format_rank(), keep_prefers_larger_then_lossless(), norm_pair() (+14 more)

### Community 24 - "vrules.rs"
Cohesion: 0.08
Nodes (59): Frame, build_membership_sql(), character_rule_matches_photos_of_that_character(), cleanup(), crud_nesting_and_cycle(), empty_rule_group_contributes_no_clause(), Library, manual_membership_roundtrip() (+51 more)

### Community 25 - "Photo"
Cohesion: 0.28
Nodes (3): Photo, ignored_title(), Vec

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

### Community 32 - "FolderTree"
Cohesion: 0.24
Nodes (10): base_name(), FolderTree, GtkBox, HashSet, Rc, RefCell, String, StringList (+2 more)

### Community 33 - "Client"
Cohesion: 0.18
Nodes (11): Client, Error, GenOptions, GenResult, Display, Formatter, From, Result (+3 more)

### Community 34 - "Library"
Cohesion: 0.21
Nodes (14): character_can_belong_to_multiple_groups(), characters_under_group_covers_subtree_and_dedupes(), cleanup(), create_rename_delete_character_group(), delete_group_does_not_delete_characters(), Library, HashMap, Path (+6 more)

### Community 35 - "face/models.rs"
Cohesion: 0.25
Nodes (17): catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry, ModelKind (+9 more)

### Community 36 - "Controller"
Cohesion: 0.17
Nodes (8): Controller, Arc, AtomicBool, Duration, Instant, Mutex, Option, Session

### Community 37 - "Result"
Cohesion: 0.24
Nodes (4): Library, HashSet, Result, Vec

### Community 38 - "PhotoObject"
Cohesion: 0.18
Nodes (8): ObjectImpl, ObjectSubclass, PhotoObject, Option, RefCell, Self, String, Texture

### Community 39 - "vmenu.rs"
Cohesion: 0.22
Nodes (18): album_depth(), bake_source(), build_menu(), copy_photo_to_clipboard(), CopySource, dismiss(), install_grid_context_menu(), local_photo_ids() (+10 more)

### Community 40 - "build_factory"
Cohesion: 0.15
Nodes (12): Overlay, SignalListItemFactory, apply_texture(), build_factory(), decode_texture(), image_rect(), overlay_parts(), Image (+4 more)

### Community 41 - "Prefs"
Cohesion: 0.20
Nodes (13): apply_grouping_defaults_v2(), bool_setting(), format_sizes(), load_ai_config(), load_face_config(), load_styleface_config(), parse_sizes(), Prefs (+5 more)

### Community 42 - "opencode.json"
Cohesion: 0.50
Nodes (3): plugin, $schema, .opencode/plugins/graphify.js

### Community 43 - "Library"
Cohesion: 0.20
Nodes (15): cleanup(), create_rename_delete_person_group(), delete_group_does_not_delete_persons(), Library, person_can_belong_to_multiple_groups(), persons_under_group_covers_subtree_and_dedupes(), HashMap, Path (+7 more)

### Community 44 - "enrich.rs"
Cohesion: 0.31
Nodes (17): append_ids(), enqueue_folder(), enqueue_ids(), enqueue_root(), enqueue_visible(), enrich_one(), generate_all(), Msg (+9 more)

### Community 46 - "StatusBar"
Cohesion: 0.15
Nodes (11): MenuButton, ProgressBar, activity_indicator(), fmt_elapsed(), Box, Button, Duration, Label (+3 more)

### Community 47 - "Embedder"
Cohesion: 0.33
Nodes (7): Embedder, Mutex, Result, String, Vec, sample_rgb(), umeyama_similarity()

### Community 48 - "TextureCache"
Cohesion: 0.26
Nodes (6): HashMap, Option, String, Texture, Vec, TextureCache

### Community 49 - "characters.rs"
Cohesion: 0.35
Nodes (16): assign_photos_to_character_dialog(), assign_style_cluster_to_character(), assign_style_clusters_to_character(), assign_style_face_dialog(), assign_style_faces_per_face_dialog(), assign_style_faces_to_character(), name_style_clusters(), name_style_clusters_dialog() (+8 more)

### Community 50 - "Embedder"
Cohesion: 0.31
Nodes (6): Embedder, Mutex, Result, String, Vec, sample_rgb()

### Community 51 - "Result"
Cohesion: 0.24
Nodes (6): Library, HashSet, Option, Result, Vec, ImmichServer

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
Cohesion: 0.23
Nodes (12): FacesView, Button, FlowBox, GtkBox, Label, Option, Rc, RefCell (+4 more)

### Community 57 - "name_cluster_dialog"
Cohesion: 0.50
Nodes (8): assign_cluster_to_person(), assign_face_dialog(), name_cluster(), name_cluster_dialog(), F, Rc, Result, String

### Community 58 - "FaceConfig"
Cohesion: 0.20
Nodes (5): FaceConfig, ClusterParams, Default, Self, String

### Community 59 - "parse_log_level"
Cohesion: 0.46
Nodes (7): LevelFilter, init_logging(), main(), parse_log_level(), print_usage(), ExitCode, Result

### Community 60 - "face/cluster.rs"
Cohesion: 0.15
Nodes (19): Item, Iterator, accumulate(), at_deg(), cluster(), ClusterAssignment, ClusterItem, ClusterParams (+11 more)

### Community 61 - "inference_test.rs"
Cohesion: 0.43
Nodes (7): cosine(), data_dir(), face_pipeline_real_inference(), init(), load_rgb(), PathBuf, Vec

### Community 62 - "edit.rs"
Cohesion: 0.28
Nodes (16): Rgba, apply_brightness_contrast(), apply_edits(), apply_levels(), auto_levels(), auto_levels_finds_range(), channel_bounds(), channel_lut() (+8 more)

### Community 63 - "HANDOFF.md"
Cohesion: 0.29
Nodes (4): RULE TWO: hand off before context is too large, Fast two-phase import and library freshness (HANDOFF), Fast two-phase import (README), Library freshness (README)

### Community 64 - "Human facial detection and recognition system"
Cohesion: 0.38
Nodes (7): ONNX Runtime download-on-demand design, CCIP embedder swap for stylised faces, Human facial detection and recognition system, Stylised face recognition and album Face type, Facial recognition, local and optional (README), People vs. Characters: two recognition pipelines (README), Facial detection & recognition (ROADMAP)

### Community 65 - "parse_tags"
Cohesion: 0.43
Nodes (5): clean_tag(), parse_tags(), parse_tags_cases(), String, Vec

### Community 66 - "freshness.rs"
Cohesion: 0.57
Nodes (7): Msg, reconcile_now(), refresh_library(), Rc, run_reconcile(), scan_new_folders(), start_periodic()

### Community 67 - "grid.rs"
Cohesion: 0.16
Nodes (17): cell_key(), Done, draw_face_badge(), DupCellUi, DupGroupUi, FaceBoxRect, human_size(), immich_path() (+9 more)

### Community 68 - "immich_pane"
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

### Community 109 - "nftree.rs"
Cohesion: 0.24
Nodes (22): ancestors(), basename_lower(), build(), collapse_synthetics(), deep_new_folder_with_subfolders_nests(), fids(), find_locates_nested_node(), folder_with_subfolders_groups_under_synthetic_dir() (+14 more)

### Community 110 - "model.rs"
Cohesion: 0.20
Nodes (7): CharacterGroup, ImmichAlbum, ImmichFolderLink, String, Tag, TagCount, TagSource

### Community 113 - "DiskAlbumMapper"
Cohesion: 0.36
Nodes (7): DiskAlbumMapper, file_subtree_under_album(), HashMap, Library, Option, String, sync_disk_tree()

### Community 115 - "styleface/detector.rs"
Cohesion: 0.32
Nodes (9): Cand, Detector, iou(), nms(), Mutex, Result, String, Vec (+1 more)

### Community 116 - "face/detector.rs"
Cohesion: 0.32
Nodes (9): Cand, Detector, iou(), nms(), Mutex, Result, String, Vec (+1 more)

### Community 117 - "settings_characters.rs"
Cohesion: 0.54
Nodes (7): characters_pane(), delete_all(), grouping_section(), GtkBox, Rc, Scale, slider()

### Community 118 - "settings_faces.rs"
Cohesion: 0.42
Nodes (9): delete_all_face_data(), faces_pane(), grouping_section(), CheckButton, GtkBox, Rc, Scale, show_ignored_check() (+1 more)

### Community 119 - ".new"
Cohesion: 0.15
Nodes (5): Arc, AtomicU64, F, Library, SortOrder

### Community 120 - "Self"
Cohesion: 0.12
Nodes (5): AiStatus, AlbumKind, PhotoScanState, Self, ScanStatus

### Community 121 - "FacePipeline"
Cohesion: 0.29
Nodes (7): DetectedFace, FacePipeline, Detector, Embedder, Result, String, Vec

### Community 122 - "StyleFacePipeline"
Cohesion: 0.29
Nodes (7): DetectedStyleFace, Detector, Embedder, Result, String, Vec, StyleFacePipeline

### Community 125 - "spin_row"
Cohesion: 0.57
Nodes (6): ai_pane(), fixed_label(), GtkBox, Label, Rc, spin_row()

## Knowledge Gaps
- **36 isolated node(s):** `$schema`, `.opencode/plugins/graphify.js`, `pichouse`, `Msg`, `graphify` (+31 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **36 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `AppState` to `Sidebar`, `Generator`, `EditPanel`, `Viewer`, `stylefacescan.rs`, `PhotoEdit`, `db/mod.rs`, `facescan.rs`, `Properties`, `CharactersView`, `settings.rs`, `Grid`, `NewFilesView`, `app.rs`, `Config`, `dedup.rs`, `vrules.rs`, `show_message`, `ui/immich.rs`, `Rc`, `FolderTree`, `Controller`, `vmenu.rs`, `Prefs`, `enrich.rs`, `StatusBar`, `characters.rs`, `actions.rs`, `FacesView`, `name_cluster_dialog`, `FaceConfig`, `freshness.rs`, `immich_pane`, `toolbar.rs`, `model.rs`, `settings_characters.rs`, `settings_faces.rs`, `spin_row`?**
  _High betweenness centrality (0.337) - this node is a cross-community bridge._
- **Why does `Photo` connect `Photo` to `Library`, `EditPanel`, `Viewer`, `PhotoEdit`, `scan.rs`, `Result`, `Library`, `AppState`, `Properties`, `Grid`, `NewFilesView`, `dedup.rs`, `vrules.rs`, `show_message`, `Library`, `ui/immich.rs`, `Result`, `PhotoObject`, `vmenu.rs`, `grid.rs`, `model.rs`, `.show_duplicates`, `Self`?**
  _High betweenness centrality (0.147) - this node is a cross-community bridge._
- **Why does `Controller` connect `Controller` to `AppState`, `app.rs`, `StatusBar`?**
  _High betweenness centrality (0.076) - this node is a cross-community bridge._
- **Are the 68 inferred relationships involving `show_error()` (e.g. with `add_library_folder()` and `rescan_all()`) actually correct?**
  _`show_error()` has 68 INFERRED edges - model-reasoned connections that need verification._
- **What connects `$schema`, `.opencode/plugins/graphify.js`, `pichouse` to the rest of the system?**
  _36 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Sidebar` be split into smaller, more focused modules?**
  _Cohesion score 0.07424903100775193 - nodes in this community are weakly interconnected._
- **Should `Generator` be split into smaller, more focused modules?**
  _Cohesion score 0.0629399585921325 - nodes in this community are weakly interconnected._