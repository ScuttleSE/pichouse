# Graph Report - pichouse  (2026-10-07)

## Corpus Check
- 123 files · ~197,215 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2561 nodes · 6691 edges · 137 communities (102 shown, 35 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 174 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `17b8c42d`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Sidebar
- thumb.rs
- Library
- EditPanel
- Viewer
- stylefacescan.rs
- ptrui.rs
- scan.rs
- Result
- Result
- db/mod.rs
- AppState
- edit.rs
- Client
- Properties
- reconcile.rs
- CharactersView
- TagEntry
- Grid
- styleface/cluster.rs
- NewFilesView
- FolderTree
- pathtip.rs
- dedup.rs
- vrules.rs
- Photo
- show_message
- Library
- ui/immich.rs
- Result
- Rc
- styleface/models.rs
- FacesView
- Client
- Library
- face/models.rs
- grid.rs
- build_factory
- PhotoObject
- vmenu.rs
- Prefs
- Config
- opencode.json
- Library
- enrich.rs
- graphify.js
- settings.rs
- place.rs
- TextureCache
- model.rs
- TileState
- .tags_for_sha256
- RULE THREE: versioning and named release process
- db/config.rs
- actions.rs
- CLAUDE.md
- ThumbPic
- people.rs
- Thumbs
- parse_log_level
- face/cluster.rs
- inference_test.rs
- Session
- HANDOFF.md
- Human facial detection and recognition system
- parse_tags
- export.rs
- mosaic.rs
- Result
- Duplicate image finder (HANDOFF)
- Immich integration (HANDOFF)
- Non-destructive editing and color levels (HANDOFF)
- Timeline, copy, crop overlay, slideshows, logging, freeze fixes
- pichouse tech stack (AGENTS.md)
- License switch from MIT to the Unlicense
- Virtual albums, drag-drop, tree persistence (HANDOFF)
- Color levels for negative scans (README)
- LevelPreset
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
- FaceConfig
- StyleFaceConfig
- bench.rs
- DiskAlbumMapper
- Rc
- facescan.rs
- Result
- app.rs
- settings_faces.rs
- toolbar.rs
- settings_tagging.rs
- PTR tag database — plan
- .new
- freshness.rs
- immich_pane
- StatusBar
- Generator
- Error
- phash.rs
- String
- sync.rs
- Update
- PtrDb
- settings_characters.rs
- spin_row
- ptr-sync.rs

## God Nodes (most connected - your core abstractions)
1. `AppState` - 263 edges
2. `Grid` - 136 edges
3. `Sidebar` - 105 edges
4. `Photo` - 81 edges
5. `show_error()` - 76 edges
6. `Viewer` - 61 edges
7. `Library` - 59 edges
8. `Library` - 53 edges
9. `EditPanel` - 50 edges
10. `Library` - 47 edges

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

## Communities (137 total, 35 thin omitted)

### Community 0 - "Sidebar"
Cohesion: 0.07
Nodes (50): ListItem, ListView, Propagation, confirm(), prompt_text(), remove_empty_albums(), F, Option (+42 more)

### Community 1 - "thumb.rs"
Cohesion: 0.32
Nodes (18): decode(), decode_oriented_rgb(), encode(), encode_for_ai(), encode_produces_jpeg(), Inner, render_face_crop(), resize() (+10 more)

### Community 2 - "Library"
Cohesion: 0.08
Nodes (27): MutexGuard, CountCache, create_face_stats_triggers(), create_unassigned_triggers(), enrichment_in_is_scoped_to_folder_set(), enrichment_under_root_is_scoped_by_prefix(), Library, map_photo() (+19 more)

### Community 3 - "EditPanel"
Cohesion: 0.12
Nodes (30): SpinButton, channel_vals(), ChannelWidgets, Controls, draw_triangle(), draw_triangle_outline(), EditPanel, histogram_from_source() (+22 more)

### Community 4 - "Viewer"
Cohesion: 0.07
Nodes (42): CropPermille, Picture, Pixbuf, Library, map_edit(), HashMap, Result, Row (+34 more)

### Community 5 - "stylefacescan.rs"
Cohesion: 0.09
Nodes (38): download_and_extract(), ensure_runtime(), ensure_runtime_progress(), extract_so_from_tgz(), init_runtime(), Fn, Path, PathBuf (+30 more)

### Community 6 - "ptrui.rs"
Cohesion: 0.09
Nodes (45): Editable, Send, Controller, Duration, Mutex, Option, available(), change_location() (+37 more)

### Community 7 - "scan.rs"
Cohesion: 0.09
Nodes (42): Error, Exif, civil_to_unix(), civil_unix_year_roundtrip(), dimensions(), enrich_file(), enrich_file_with_image(), Enrichment (+34 more)

### Community 8 - "Result"
Cohesion: 0.07
Nodes (46): add_photo(), assert_stats(), blob_to_floats(), box_matches(), counts_are_photos_not_faces(), cover_if_unset_fills_gap_but_not_an_existing_choice(), delete_all_clears_everything(), delete_and_ban_rejects_every_face() (+38 more)

### Community 9 - "Result"
Cohesion: 0.08
Nodes (20): add_photo(), blob_to_floats(), counts_are_photos_not_faces(), cover_if_unset_fills_gap_but_not_an_existing_choice(), floats_to_blob(), insert_style_face_row(), Library, map_character() (+12 more)

### Community 10 - "db/mod.rs"
Cohesion: 0.05
Nodes (52): face_thumbs_path(), FaceThumbs, remove_face_thumbs_database(), Connection, Mutex, Option, P, PathBuf (+44 more)

### Community 11 - "AppState"
Cohesion: 0.09
Nodes (26): ApplicationWindow, Condvar, SimpleAction, AppState, crop_pool(), CropJob, CropPool, now_millis() (+18 more)

### Community 12 - "edit.rs"
Cohesion: 0.28
Nodes (16): Rgba, apply_brightness_contrast(), apply_edits(), apply_levels(), auto_levels(), auto_levels_finds_range(), channel_bounds(), channel_lut() (+8 more)

### Community 13 - "Client"
Cohesion: 0.11
Nodes (19): civil_from_days(), civil_to_unix(), Client, Error, parse_rfc3339_seconds(), parse_taken_at(), Display, Formatter (+11 more)

### Community 14 - "Properties"
Cohesion: 0.14
Nodes (18): bold_label(), field(), Properties, Button, GtkBox, Label, Notebook, Option (+10 more)

### Community 15 - "reconcile.rs"
Cohesion: 0.17
Nodes (29): collect(), DbSnapshot, dir_has_images(), mtime_secs(), PhotoInsert, PhotoMove, plan_dir(), plan_vanished_dirs() (+21 more)

### Community 16 - "CharactersView"
Cohesion: 0.07
Nodes (42): Vec, UnnamedGroupInfo, character_folder_faces(), CharactersView, fill_style_crop(), Button, FlowBox, GtkBox (+34 more)

### Community 17 - "TagEntry"
Cohesion: 0.09
Nodes (28): ListBox, Popover, Arc, Box, Cell, Entry, F, Fn (+20 more)

### Community 18 - "Grid"
Cohesion: 0.07
Nodes (13): CssProvider, GridView, ListStore, SignalHandlerId, Grid, Box, DropDown, Fn (+5 more)

### Community 19 - "styleface/cluster.rs"
Cohesion: 0.17
Nodes (24): Center, _center_ref(), centroid(), chain_does_not_link_two_groups(), character_anchors_stable_cluster(), cluster(), ClusterAssignment, ClusterItem (+16 more)

### Community 20 - "NewFilesView"
Cohesion: 0.11
Nodes (24): ControlFlow, BuildState, decode_pixels(), Done, FolderRange, Job, NewFilesView, Arc (+16 more)

### Community 21 - "FolderTree"
Cohesion: 0.24
Nodes (10): base_name(), FolderTree, GtkBox, HashSet, Rc, RefCell, String, StringList (+2 more)

### Community 22 - "pathtip.rs"
Cohesion: 0.22
Nodes (13): attach(), bind(), path_text(), Fn, IsA, Overlay, String, Vec (+5 more)

### Community 23 - "dedup.rs"
Cohesion: 0.14
Nodes (22): banned_pair_is_not_grouped(), choose_keep(), DupGroup, exact_hash_groups(), find_duplicates(), format_rank(), keep_prefers_larger_then_lossless(), norm_pair() (+14 more)

### Community 24 - "vrules.rs"
Cohesion: 0.09
Nodes (58): Frame, build_membership_sql(), character_rule_matches_photos_of_that_character(), cleanup(), crud_nesting_and_cycle(), empty_rule_group_contributes_no_clause(), Library, manual_membership_roundtrip() (+50 more)

### Community 25 - "Photo"
Cohesion: 0.16
Nodes (3): Photo, ignored_title(), Vec

### Community 26 - "show_message"
Cohesion: 0.17
Nodes (22): ai_tag_folder(), ai_tag_library(), Msg, Arc, AtomicBool, Client, Library, Rc (+14 more)

### Community 27 - "Library"
Cohesion: 0.14
Nodes (15): album_kind_inherits_down_the_tree(), empty_albums_by_kind(), folder_effective_face_kind_follows_its_album_or_defaults_to_photo(), folders_under_album_covers_subtree(), gone_folder_row_is_removed_album_stays(), Library, remove_library_folder_then_use_albums(), HashMap (+7 more)

### Community 28 - "ui/immich.rs"
Cohesion: 0.21
Nodes (22): autoupload_added(), link_and_sync(), refresh_albums(), PathBuf, Rc, String, Vec, sanitize_folder_name() (+14 more)

### Community 29 - "Result"
Cohesion: 0.16
Nodes (13): affected_photo_ids(), fts_query(), GroupTag, Library, merge_tag_into(), rebuild_photo_fts(), HashMap, HashSet (+5 more)

### Community 31 - "styleface/models.rs"
Cohesion: 0.24
Nodes (19): Response, catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry (+11 more)

### Community 32 - "FacesView"
Cohesion: 0.12
Nodes (33): assign_photos_to_character_dialog(), assign_style_cluster_to_character(), assign_style_clusters_to_character(), assign_style_face_dialog(), assign_style_faces_per_face_dialog(), assign_style_faces_to_character(), name_style_clusters(), name_style_clusters_dialog() (+25 more)

### Community 33 - "Client"
Cohesion: 0.18
Nodes (11): Client, Error, GenOptions, GenResult, Display, Formatter, From, Result (+3 more)

### Community 34 - "Library"
Cohesion: 0.19
Nodes (15): character_can_belong_to_multiple_groups(), characters_under_group_covers_subtree_and_dedupes(), cleanup(), create_rename_delete_character_group(), delete_group_does_not_delete_characters(), Library, HashMap, Path (+7 more)

### Community 35 - "face/models.rs"
Cohesion: 0.25
Nodes (17): catalog(), ensure_model(), ensure_model_progress(), entry(), model_path(), model_present(), ModelEntry, ModelKind (+9 more)

### Community 36 - "grid.rs"
Cohesion: 0.17
Nodes (19): cell_key(), Done, DupCellUi, DupGroupUi, FaceBoxRect, human_size(), immich_path(), ImmichJob (+11 more)

### Community 37 - "build_factory"
Cohesion: 0.12
Nodes (14): SignalListItemFactory, apply_texture(), build_factory(), build_tag_icon(), decode_texture(), draw_face_badge(), overlay_parts(), Context (+6 more)

### Community 38 - "PhotoObject"
Cohesion: 0.13
Nodes (8): PhotoObject, ObjectImpl, ObjectSubclass, Option, RefCell, Self, String, Texture

### Community 39 - "vmenu.rs"
Cohesion: 0.19
Nodes (24): album_depth(), append_recent_items(), bake_source(), build_menu(), copy_photo_to_clipboard(), CopySource, dismiss(), group_face_ids() (+16 more)

### Community 40 - "Prefs"
Cohesion: 0.20
Nodes (13): apply_grouping_defaults_v2(), bool_setting(), format_sizes(), load_ai_config(), load_face_config(), load_styleface_config(), parse_sizes(), Prefs (+5 more)

### Community 41 - "Config"
Cohesion: 0.14
Nodes (11): Child, Config, normalize_fills_defaults_and_clamps(), Default, String, Manager, Client, Drop (+3 more)

### Community 42 - "opencode.json"
Cohesion: 0.50
Nodes (3): plugin, $schema, .opencode/plugins/graphify.js

### Community 43 - "Library"
Cohesion: 0.20
Nodes (14): cleanup(), create_rename_delete_person_group(), delete_group_does_not_delete_persons(), Library, person_can_belong_to_multiple_groups(), persons_under_group_covers_subtree_and_dedupes(), HashMap, Path (+6 more)

### Community 44 - "enrich.rs"
Cohesion: 0.31
Nodes (17): append_ids(), enqueue_folder(), enqueue_ids(), enqueue_root(), enqueue_visible(), enrich_one(), generate_all(), Msg (+9 more)

### Community 46 - "settings.rs"
Cohesion: 0.15
Nodes (23): action_key(), appearance_pane(), capture_shortcut(), folder_pane(), pane_box(), GtkBox, Label, Rc (+15 more)

### Community 47 - "place.rs"
Cohesion: 0.20
Nodes (16): check_fs(), db_files(), db_size(), fs_info(), FsVerdict, move_db(), move_renames_on_same_fs(), AtomicBool (+8 more)

### Community 48 - "TextureCache"
Cohesion: 0.26
Nodes (6): HashMap, Option, String, Texture, Vec, TextureCache

### Community 49 - "model.rs"
Cohesion: 0.08
Nodes (12): AiStatus, AlbumKind, ImmichAlbum, PersonGroup, PhotoScanState, Self, String, RuleOp (+4 more)

### Community 50 - "TileState"
Cohesion: 0.18
Nodes (20): GString, Paintable, alt_held(), attach(), next_rand(), restore(), Arc, Cell (+12 more)

### Community 51 - ".tags_for_sha256"
Cohesion: 0.16
Nodes (15): hex_to_bytes(), is_excluded(), lookup_resolves_siblings_parents_and_filters(), LookupOptions, make_db(), normalize(), PtrReader, Connection (+7 more)

### Community 52 - "RULE THREE: versioning and named release process"
Cohesion: 0.22
Nodes (10): RULE THREE: versioning and named release process, Build and release workflow, Bump build number step, Detect documentation-only push, Named release workflow, Publish GitHub release step, Push filtered snapshot to GitHub step, GitHub mirror pushes filtered snapshot, not full history (+2 more)

### Community 53 - "db/config.rs"
Cohesion: 0.56
Nodes (9): config_path(), data_dir(), default_data_dir(), home_dir(), read_configured_data_dir(), Option, PathBuf, Result (+1 more)

### Community 54 - "actions.rs"
Cohesion: 0.42
Nodes (10): add_library_folder(), enqueue_scan(), find_duplicates(), Msg, rescan_all(), resume_scan(), Rc, String (+2 more)

### Community 56 - "ThumbPic"
Cohesion: 0.12
Nodes (12): Snapshot, Cell, Default, Focus, ObjectImpl, ObjectSubclass, Option, RefCell (+4 more)

### Community 57 - "people.rs"
Cohesion: 0.40
Nodes (12): assign_cluster_to_person(), assign_face_dialog(), assign_faces_to_person(), assign_photos_to_person_dialog(), name_cluster(), name_cluster_dialog(), reload_sidebar(), F (+4 more)

### Community 58 - "Thumbs"
Cohesion: 0.19
Nodes (10): remove_all_thumb_databases(), Connection, Mutex, Option, P, PathBuf, Result, Vec (+2 more)

### Community 59 - "parse_log_level"
Cohesion: 0.46
Nodes (7): LevelFilter, init_logging(), main(), parse_log_level(), print_usage(), ExitCode, Result

### Community 60 - "face/cluster.rs"
Cohesion: 0.15
Nodes (19): Item, Iterator, accumulate(), at_deg(), cluster(), ClusterAssignment, ClusterItem, ClusterParams (+11 more)

### Community 61 - "inference_test.rs"
Cohesion: 0.43
Nodes (7): cosine(), data_dir(), face_pipeline_real_inference(), init(), load_rgb(), PathBuf, Vec

### Community 62 - "Session"
Cohesion: 0.06
Nodes (42): Cand, Detector, iou(), nms(), Mutex, Result, String, Vec (+34 more)

### Community 63 - "HANDOFF.md"
Cohesion: 0.29
Nodes (4): RULE TWO: hand off before context is too large, Fast two-phase import and library freshness (HANDOFF), Fast two-phase import (README), Library freshness (README)

### Community 64 - "Human facial detection and recognition system"
Cohesion: 0.38
Nodes (7): ONNX Runtime download-on-demand design, CCIP embedder swap for stylised faces, Human facial detection and recognition system, Stylised face recognition and album Face type, Facial recognition, local and optional (README), People vs. Characters: two recognition pipelines (README), Facial detection & recognition (ROADMAP)

### Community 65 - "parse_tags"
Cohesion: 0.43
Nodes (5): clean_tag(), parse_tags(), parse_tags_cases(), String, Vec

### Community 66 - "export.rs"
Cohesion: 0.31
Nodes (16): bake_and_write(), choose_and_export(), export_photos(), ExportOpts, load_opts(), open_options(), rotate_full(), Path (+8 more)

### Community 67 - "mosaic.rs"
Cohesion: 0.23
Nodes (8): build(), pick_faces(), reps_first_then_fill_without_repeats(), Rng, Fn, HashMap, Image, Vec

### Community 68 - "Result"
Cohesion: 0.24
Nodes (4): Library, HashSet, Result, Vec

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

### Community 77 - "LevelPreset"
Cohesion: 0.27
Nodes (6): Library, map_preset(), Result, Row, Vec, LevelPreset

### Community 109 - "nftree.rs"
Cohesion: 0.24
Nodes (22): ancestors(), basename_lower(), build(), collapse_synthetics(), deep_new_folder_with_subfolders_nests(), fids(), find_locates_nested_node(), folder_with_subfolders_groups_under_synthetic_dir() (+14 more)

### Community 110 - "FaceConfig"
Cohesion: 0.20
Nodes (5): FaceConfig, ClusterParams, Default, Self, String

### Community 111 - "StyleFaceConfig"
Cohesion: 0.20
Nodes (5): ClusterParams, Default, Self, String, StyleFaceConfig

### Community 112 - "bench.rs"
Cohesion: 0.36
Nodes (9): bench_all(), open(), FnMut, Library, Option, T, seed(), time() (+1 more)

### Community 113 - "DiskAlbumMapper"
Cohesion: 0.36
Nodes (7): DiskAlbumMapper, file_subtree_under_album(), HashMap, Library, Option, String, sync_disk_tree()

### Community 114 - "Rc"
Cohesion: 0.24
Nodes (5): Button, Library, Rc, Self, style_tag_btn()

### Community 115 - "facescan.rs"
Cohesion: 0.16
Nodes (23): FaceGroup, a_group_that_lost_its_photos_counts_zero_new(), counts_only_photos_added_to_a_pre_existing_group(), download_models(), fail(), Msg, new_photo_counts(), recluster() (+15 more)

### Community 116 - "Result"
Cohesion: 0.22
Nodes (7): Library, HashSet, Option, Result, Vec, ImmichFolderLink, ImmichServer

### Community 117 - "app.rs"
Cohesion: 0.24
Nodes (14): Application, apply_theme(), build_ui(), install_css(), load_folder_into_grid(), load_raw_folder_into_grid(), populate(), populate_deferred() (+6 more)

### Community 118 - "settings_faces.rs"
Cohesion: 0.42
Nodes (9): delete_all_face_data(), faces_pane(), grouping_section(), CheckButton, GtkBox, Rc, Scale, show_ignored_check() (+1 more)

### Community 119 - "toolbar.rs"
Cohesion: 0.44
Nodes (8): build_toolbar(), compact_button(), Button, GtkBox, Rc, start_slideshow(), start_slideshow_from_prefs(), toggle_properties()

### Community 120 - "settings_tagging.rs"
Cohesion: 0.47
Nodes (8): apply(), choice_row(), default_of(), general_tab(), GtkBox, Notebook, Rc, tagging_pane()

### Community 121 - "PTR tag database — plan"
Cohesion: 0.09
Nodes (22): Commands, Decisions, Facts about the PTR, Files, Full sync results (measured 2026-10-05, on the user's desktop), Goal, Handoff (read this first), Next steps (+14 more)

### Community 122 - ".new"
Cohesion: 0.16
Nodes (4): Arc, AtomicU64, F, SortOrder

### Community 123 - "freshness.rs"
Cohesion: 0.57
Nodes (7): Msg, reconcile_now(), refresh_library(), Rc, run_reconcile(), scan_new_folders(), start_periodic()

### Community 124 - "immich_pane"
Cohesion: 0.53
Nodes (5): immich_pane(), labeled_entry(), Entry, GtkBox, Rc

### Community 125 - "StatusBar"
Cohesion: 0.14
Nodes (11): MenuButton, activity_indicator(), fmt_elapsed(), Box, Button, Duration, Label, ProgressBar (+3 more)

### Community 127 - "Generator"
Cohesion: 0.21
Nodes (4): Generator, FnOnce, Mutex, T

### Community 128 - "Error"
Cohesion: 0.20
Nodes (8): ImageError, cache_key(), Error, Display, Formatter, From, Self, String

### Community 130 - "phash.rs"
Cohesion: 0.36
Nodes (4): dhash_file(), dhash_rgb(), identical_buffers_match(), Path

### Community 132 - "String"
Cohesion: 0.33
Nodes (12): Client, decode(), hex(), Metadata, MetaEntry, parse_metadata(), Option, Result (+4 more)

### Community 134 - "sync.rs"
Cohesion: 0.18
Nodes (11): Drop, PathBuf, TempPath, Progress, AtomicBool, Client, FnMut, Option (+3 more)

### Community 135 - "Update"
Cohesion: 0.22
Nodes (10): spike_ptr(), parse(), Option, Result, String, Value, Vec, skips_non_sha256_hashes() (+2 more)

### Community 136 - "PtrDb"
Cohesion: 0.25
Nodes (6): PtrDb, Connection, Path, Result, Self, Vec

### Community 137 - "settings_characters.rs"
Cohesion: 0.54
Nodes (7): characters_pane(), delete_all(), grouping_section(), GtkBox, Rc, Scale, slider()

### Community 138 - "spin_row"
Cohesion: 0.57
Nodes (6): ai_pane(), fixed_label(), GtkBox, Label, Rc, spin_row()

### Community 139 - "ptr-sync.rs"
Cohesion: 0.83
Nodes (3): die(), main(), usage()

## Knowledge Gaps
- **56 isolated node(s):** `$schema`, `.opencode/plugins/graphify.js`, `pichouse`, `Msg`, `graphify` (+51 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **35 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppState` connect `AppState` to `Sidebar`, `EditPanel`, `Viewer`, `stylefacescan.rs`, `ptrui.rs`, `settings_characters.rs`, `spin_row`, `db/mod.rs`, `Properties`, `CharactersView`, `Grid`, `NewFilesView`, `FolderTree`, `dedup.rs`, `vrules.rs`, `show_message`, `ui/immich.rs`, `Rc`, `FacesView`, `vmenu.rs`, `Prefs`, `Config`, `enrich.rs`, `settings.rs`, `model.rs`, `actions.rs`, `people.rs`, `export.rs`, `FaceConfig`, `StyleFaceConfig`, `facescan.rs`, `app.rs`, `settings_faces.rs`, `toolbar.rs`, `settings_tagging.rs`, `freshness.rs`, `immich_pane`, `StatusBar`, `Generator`?**
  _High betweenness centrality (0.326) - this node is a cross-community bridge._
- **Why does `Photo` connect `Photo` to `Library`, `EditPanel`, `Viewer`, `scan.rs`, `Result`, `Result`, `AppState`, `Properties`, `Grid`, `NewFilesView`, `dedup.rs`, `vrules.rs`, `show_message`, `Library`, `ui/immich.rs`, `grid.rs`, `PhotoObject`, `vmenu.rs`, `model.rs`, `TileState`, `export.rs`, `Result`?**
  _High betweenness centrality (0.144) - this node is a cross-community bridge._
- **Why does `Grid` connect `Grid` to `mosaic.rs`, `grid.rs`, `build_factory`, `PhotoObject`, `vmenu.rs`, `AppState`, `TextureCache`, `Rc`, `Photo`, `.new`, `Result`, `Rc`?**
  _High betweenness centrality (0.110) - this node is a cross-community bridge._
- **Are the 72 inferred relationships involving `show_error()` (e.g. with `add_library_folder()` and `rescan_all()`) actually correct?**
  _`show_error()` has 72 INFERRED edges - model-reasoned connections that need verification._
- **What connects `$schema`, `.opencode/plugins/graphify.js`, `pichouse` to the rest of the system?**
  _56 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Sidebar` be split into smaller, more focused modules?**
  _Cohesion score 0.06939595895482915 - nodes in this community are weakly interconnected._
- **Should `Library` be split into smaller, more focused modules?**
  _Cohesion score 0.08190304125263474 - nodes in this community are weakly interconnected._