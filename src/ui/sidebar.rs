//! Left sidebar Library tab: an album tree over scanned folders.
//!
//! Folders not in any album appear under "New folders". Node ids are strings:
//! `album:<id>`, `folder:<id>`, and the constant `newfolders`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, GestureClick, Image, Label, ListItem, ListView, Orientation,
    PopoverMenu, ScrolledWindow, SignalListItemFactory, StringList, StringObject, TreeExpander,
    TreeListModel, TreeListRow,
};

use crate::model::{Album, Folder, VirtualAlbum};
use super::dialogs::{confirm, prompt_text};
use super::state::{show_error, AppState};

const NEW_FOLDERS_ID: &str = "newfolders";
const NEW_FILES_ID: &str = "newfiles";
const MISSING_FILES_ID: &str = "missingfiles";
const BANNED_MATCHES_ID: &str = "bannedmatches";
const ALBUM_PREFIX: &str = "album:";
const FOLDER_PREFIX: &str = "folder:";
const VALBUM_PREFIX: &str = "valbum:";
/// Header row that groups all virtual albums, shown above normal albums.
const VIRTUAL_HEADER_ID: &str = "virtualheader";
/// Header row that groups named people from facial recognition.
const PEOPLE_HEADER_ID: &str = "peopleheader";
/// A single person node: `person:<person_id>`.
const PERSON_PREFIX: &str = "person:";
/// A person group node (e.g. "Disney"): `persongroup:<group_id>`. Groups may
/// nest and a person may belong to more than one group at once.
const PERSON_GROUP_PREFIX: &str = "persongroup:";
/// Header row that groups named stylised characters.
const CHARACTERS_HEADER_ID: &str = "charactersheader";
/// A single character node: `character:<character_id>`.
const CHARACTER_PREFIX: &str = "character:";
/// A character group node. Mirrors `PERSON_GROUP_PREFIX`.
const CHARACTER_GROUP_PREFIX: &str = "charactergroup:";
/// Header row that groups all Immich servers, shown below normal albums.
const IMMICH_HEADER_ID: &str = "immichheader";
/// A single Immich server node: `immichserver:<server_id>`.
const IMMICH_SERVER_PREFIX: &str = "immichserver:";
/// A single Immich album node: `immichalbum:<server_id>:<album_uuid>`.
const IMMICH_ALBUM_PREFIX: &str = "immichalbum:";
/// The whole-library timeline node for a server: `immichtimeline:<server_id>`.
const IMMICH_TIMELINE_PREFIX: &str = "immichtimeline:";
/// `library.db` settings key for the persisted set of expanded tree node ids.
const EXPANDED_SETTING_KEY: &str = "sidebar_expanded";

/// Tree data rebuilt on each reload.
#[derive(Default)]
struct TreeData {
    folders: HashMap<i64, Folder>,
    counts: HashMap<i64, i64>,
    albums: HashMap<i64, Album>,
    album_children: HashMap<i64, Vec<i64>>,
    album_folders: HashMap<i64, Vec<i64>>,
    unassigned: Vec<i64>,
    /// Count of "new files" across the library (for the New Files row).
    new_files_count: i64,
    /// Count of photos gone from disk (for the Missing Files row).
    missing_files_count: i64,
    banned_matches_count: i64,
    /// Virtual albums by id, plus the parent→children adjacency and per-album
    /// photo counts.
    virtual_albums: HashMap<i64, VirtualAlbum>,
    valbum_children: HashMap<i64, Vec<i64>>,
    valbum_counts: HashMap<i64, i64>,
    /// Named people from facial recognition, plus per-person photo counts and a
    /// cover face id for the icon.
    persons: Vec<crate::model::Person>,
    person_counts: HashMap<i64, i64>,
    /// Total detected faces, so the People header shows even before naming.
    total_faces: i64,
    /// Person groups (e.g. "Disney"), plus parent->children adjacency and
    /// direct group->member-person-ids membership.
    person_groups: HashMap<i64, crate::model::PersonGroup>,
    person_group_children: HashMap<i64, Vec<i64>>,
    person_group_members: HashMap<i64, Vec<i64>>,
    /// Reverse index: person id -> the group ids it directly belongs to. A
    /// person with no entry (or an empty one) here is "ungrouped" and shows
    /// at the People header's top level instead of under a group.
    person_memberships: HashMap<i64, Vec<i64>>,
    /// Named stylised characters, plus per-character photo counts.
    characters: Vec<crate::model::Character>,
    character_counts: HashMap<i64, i64>,
    /// Total detected stylised faces, so the Characters header shows early.
    total_style_faces: i64,
    /// Character groups. Mirrors the person_group_* fields.
    character_groups: HashMap<i64, crate::model::CharacterGroup>,
    character_group_children: HashMap<i64, Vec<i64>>,
    character_group_members: HashMap<i64, Vec<i64>>,
    character_memberships: HashMap<i64, Vec<i64>>,
    /// Immich servers, ordered as shown. Each is `(id, name)`.
    immich_servers: Vec<(i64, String)>,
    /// Cached albums per Immich server id, as `(album_uuid, name, count)`.
    immich_albums: HashMap<i64, Vec<(String, String, i64)>>,
    /// Folder ids linked to an Immich album for auto-upload.
    immich_linked_folders: std::collections::HashSet<i64>,
}

impl TreeData {
    /// Total photos in a folder (cached scan count).
    fn folder_photo_count(&self, folder_id: i64) -> i64 {
        self.counts.get(&folder_id).copied().unwrap_or(0)
    }

    /// Total photos across an album's direct member folders. This matches what
    /// `Library::photos_in_album` uploads (direct folders, not sub-albums).
    fn album_photo_count(&self, album_id: i64) -> i64 {
        self.album_folders
            .get(&album_id)
            .into_iter()
            .flatten()
            .map(|fid| self.folder_photo_count(*fid))
            .sum()
    }
}

/// The Library-tab album tree sidebar.
pub struct Sidebar {
    root: GtkBox,
    list_root: StringList,
    tree_model: TreeListModel,
    list_view: ListView,
    data: RefCell<TreeData>,
    expanded: RefCell<std::collections::HashSet<String>>,
    state: RefCell<Option<Rc<AppState>>>,
    /// A shared per-right-click popover (rebuilt each time).
    menu_pop: RefCell<Option<PopoverMenu>>,
    /// A weak self-reference, used to hook per-row signal handlers.
    weak_self: std::rc::Weak<Sidebar>,
    /// When true, per-row expand/collapse notifications do not change the saved
    /// expansion set. Set during a reload so tree teardown/rebuild does not wipe
    /// the state.
    suppress_expand_notify: std::cell::Cell<bool>,
    /// When true, selection-changed notifications do not trigger navigation.
    /// Set while a reload re-selects the previously selected rows (whose
    /// underlying objects were just recreated), so a background refresh (e.g.
    /// during a scan) does not repeatedly reload the currently viewed folder.
    suppress_selection_notify: std::cell::Cell<bool>,
    /// The row id that should receive keyboard focus once its widget exists
    /// again after a reload. `ListView::grab_focus()` cannot target a specific
    /// row under GTK 4.10 (the per-position focus API needs 4.12), so instead
    /// `bind_row` grabs focus itself the moment it binds a widget to this id,
    /// then clears it so it only fires once.
    pending_focus_id: RefCell<Option<String>>,
    /// The last `(folder_count, album_count)` this sidebar rebuilt from. During
    /// a scan, `reload` compares the live signature against this and skips the
    /// rebuild when neither grew, so an idle refresh tick costs nothing. `None`
    /// forces the next reload to run (set on any real change, e.g. a user edit).
    last_tree_signature: std::cell::Cell<Option<(i64, i64)>>,
}

impl Sidebar {
    /// Build the sidebar. `bind_state` must be called before use.
    pub fn new() -> Rc<Sidebar> {
        let list_root = StringList::new(&[]);

        let sidebar = Rc::new_cyclic(|weak: &std::rc::Weak<Sidebar>| {
            let weak_for_model = weak.clone();
            let tree_model = TreeListModel::new(list_root.clone(), false, false, move |item| {
                let so = item.downcast_ref::<StringObject>()?;
                let id = so.string().to_string();
                let sidebar = weak_for_model.upgrade()?;
                let kids = sidebar.child_ids(&id);
                if kids.is_empty() {
                    return None;
                }
                let kid_refs: Vec<&str> = kids.iter().map(|s| s.as_str()).collect();
                let list = StringList::new(&kid_refs);
                Some(list.upcast())
            });

            let selection = gtk4::MultiSelection::new(Some(tree_model.clone()));

            let factory = SignalListItemFactory::new();
            let weak_setup = weak.clone();
            factory.connect_setup(move |_, item| {
                let item = item.downcast_ref::<ListItem>().unwrap();
                let expander = TreeExpander::new();
                expander.set_indent_for_icon(true);
                expander.set_indent_for_depth(true);
                let row = GtkBox::new(Orientation::Horizontal, 4);
                let icon = Image::from_icon_name("folder-symbolic");
                let label = Label::new(None);
                label.set_xalign(0.0);
                row.append(&icon);
                row.append(&label);
                expander.set_child(Some(&row));
                item.set_child(Some(&expander));
                if let Some(sidebar) = weak_setup.upgrade() {
                    sidebar.attach_row_menu(&expander);
                    sidebar.attach_row_drag(&expander);
                    sidebar.attach_row_activate(&expander);
                }
            });
            let weak_bind = weak.clone();
            factory.connect_bind(move |_, item| {
                if let Some(sidebar) = weak_bind.upgrade() {
                    sidebar.bind_row(item.downcast_ref::<ListItem>().unwrap());
                }
            });

            let list_view = ListView::new(Some(selection.clone()), Some(factory));

            let weak_sel = weak.clone();
            selection.connect_selection_changed(move |sel, _, _| {
                if let Some(sidebar) = weak_sel.upgrade() {
                    sidebar.on_selection_changed(sel);
                }
            });

            // Keyboard tree navigation: Right opens a node, Left collapses it or
            // moves to the parent node.
            let key = gtk4::EventControllerKey::new();
            let weak_key = weak.clone();
            key.connect_key_pressed(move |_, keyval, _, _| {
                let Some(sidebar) = weak_key.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                match keyval {
                    gdk::Key::Right => sidebar.on_expand_key(),
                    gdk::Key::Left => sidebar.on_collapse_key(),
                    _ => glib::Propagation::Proceed,
                }
            });
            list_view.add_controller(key);

            let new_album = Button::with_label("New Album");
            new_album.set_halign(gtk4::Align::Start);
            new_album.set_margin_top(4);
            new_album.set_margin_start(4);
            new_album.set_margin_bottom(4);
            let weak_btn = weak.clone();
            new_album.connect_clicked(move |_| {
                if let Some(sidebar) = weak_btn.upgrade() {
                    sidebar.prompt_create_album(0);
                }
            });

            let scroll = ScrolledWindow::new();
            scroll.set_vexpand(true);
            scroll.set_child(Some(&list_view));

            let root = GtkBox::new(Orientation::Vertical, 0);
            root.append(&new_album);
            root.append(&scroll);

            Sidebar {
                root,
                list_root: list_root.clone(),
                tree_model,
                list_view,
                data: RefCell::new(TreeData::default()),
                expanded: RefCell::new(std::collections::HashSet::new()),
                state: RefCell::new(None),
                menu_pop: RefCell::new(None),
                weak_self: weak.clone(),
                suppress_expand_notify: std::cell::Cell::new(false),
                suppress_selection_notify: std::cell::Cell::new(false),
                pending_focus_id: RefCell::new(None),
                last_tree_signature: std::cell::Cell::new(None),
            }
        });

        sidebar.install_context_menu();
        sidebar
    }

    /// Give the sidebar access to shared state.
    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state);
        // Restore the expansion set persisted from the last session before the
        // first reload builds the tree.
        self.load_expansion();
    }

    fn state(&self) -> Option<Rc<AppState>> {
        self.state.borrow().clone()
    }

    /// The sidebar root widget.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// The child node-id strings for a node id.
    fn child_ids(&self, id: &str) -> Vec<String> {
        let data = self.data.borrow();
        if id == VIRTUAL_HEADER_ID {
            data.valbum_children
                .get(&0)
                .into_iter()
                .flatten()
                .map(|vid| format!("{VALBUM_PREFIX}{vid}"))
                .collect()
        } else if id == PEOPLE_HEADER_ID {
            let mut out: Vec<String> = data
                .person_group_children
                .get(&0)
                .into_iter()
                .flatten()
                .map(|gid| format!("{PERSON_GROUP_PREFIX}{gid}"))
                .collect();
            out.extend(data.persons.iter().filter_map(|p| {
                let ungrouped = data
                    .person_memberships
                    .get(&p.id)
                    .map(|g| g.is_empty())
                    .unwrap_or(true);
                ungrouped.then(|| format!("{PERSON_PREFIX}{}", p.id))
            }));
            out
        } else if let Some(gid) = person_group_id_of(id) {
            let mut out: Vec<String> = data
                .person_group_children
                .get(&gid)
                .into_iter()
                .flatten()
                .map(|child| format!("{PERSON_GROUP_PREFIX}{child}"))
                .collect();
            out.extend(
                data.person_group_members
                    .get(&gid)
                    .into_iter()
                    .flatten()
                    .map(|pid| format!("{PERSON_PREFIX}{pid}")),
            );
            out
        } else if id == CHARACTERS_HEADER_ID {
            let mut out: Vec<String> = data
                .character_group_children
                .get(&0)
                .into_iter()
                .flatten()
                .map(|gid| format!("{CHARACTER_GROUP_PREFIX}{gid}"))
                .collect();
            out.extend(data.characters.iter().filter_map(|c| {
                let ungrouped = data
                    .character_memberships
                    .get(&c.id)
                    .map(|g| g.is_empty())
                    .unwrap_or(true);
                ungrouped.then(|| format!("{CHARACTER_PREFIX}{}", c.id))
            }));
            out
        } else if let Some(gid) = character_group_id_of(id) {
            let mut out: Vec<String> = data
                .character_group_children
                .get(&gid)
                .into_iter()
                .flatten()
                .map(|child| format!("{CHARACTER_GROUP_PREFIX}{child}"))
                .collect();
            out.extend(
                data.character_group_members
                    .get(&gid)
                    .into_iter()
                    .flatten()
                    .map(|cid| format!("{CHARACTER_PREFIX}{cid}")),
            );
            out
        } else if id == IMMICH_HEADER_ID {
            data.immich_servers
                .iter()
                .map(|(sid, _)| format!("{IMMICH_SERVER_PREFIX}{sid}"))
                .collect()
        } else if let Some(sid) = immich_server_id_of(id) {
            let mut out = vec![format!("{IMMICH_TIMELINE_PREFIX}{sid}")];
            out.extend(
                data.immich_albums
                    .get(&sid)
                    .into_iter()
                    .flatten()
                    .map(|(uuid, _, _)| format!("{IMMICH_ALBUM_PREFIX}{sid}:{uuid}")),
            );
            out
        } else if let Some(vid) = valbum_id_of(id) {
            data.valbum_children
                .get(&vid)
                .into_iter()
                .flatten()
                .map(|child| format!("{VALBUM_PREFIX}{child}"))
                .collect()
        } else if let Some(aid) = album_id_of(id) {
            let mut out = Vec::new();
            for &child in data.album_children.get(&aid).into_iter().flatten() {
                out.push(format!("{ALBUM_PREFIX}{child}"));
            }
            for &fid in data.album_folders.get(&aid).into_iter().flatten() {
                out.push(format!("{FOLDER_PREFIX}{fid}"));
            }
            out
        } else if id == NEW_FOLDERS_ID {
            data.unassigned
                .iter()
                .map(|fid| format!("{FOLDER_PREFIX}{fid}"))
                .collect()
        } else {
            Vec::new()
        }
    }

    fn bind_row(&self, item: &ListItem) {
        let Some(row) = item.item().and_downcast::<TreeListRow>() else {
            return;
        };
        let Some(expander) = item.child().and_downcast::<TreeExpander>() else {
            return;
        };
        expander.set_list_row(Some(&row));
        let Some(so) = row.item().and_downcast::<StringObject>() else {
            return;
        };
        let id = so.string().to_string();
        // If a reload is waiting to hand keyboard focus back to this row, this
        // is the moment its widget actually exists again — grab it here, not
        // via `ListView::grab_focus()` (see `reload()`), and consume the
        // pending id so it only fires once.
        if self.pending_focus_id.borrow().as_deref() == Some(id.as_str()) {
            self.pending_focus_id.borrow_mut().take();
            expander.grab_focus();
        }
        expander.set_widget_name(&id);
        let Some(box_) = expander.child().and_downcast::<GtkBox>() else {
            return;
        };
        let icon = box_.first_child().and_downcast::<Image>();
        let label = box_.last_child().and_downcast::<Label>();
        let (name, icon_name) = self.node_label(&id);
        if let Some(icon) = icon {
            icon.set_from_icon_name(Some(icon_name));
        }
        if let Some(label) = label {
            label.set_text(&name);
        }

        // Persist expansion changes immediately when the user expands/collapses
        // this row, so the tree view survives a restart even without a reload.
        // Disconnect the handler left on this recycled item from its previous
        // row before connecting to the current one.
        unsafe {
            if let Some(prev) =
                item.steal_data::<(glib::WeakRef<TreeListRow>, glib::SignalHandlerId)>(
                    "expanded-handler",
                )
            {
                if let Some(prev_row) = prev.0.upgrade() {
                    prev_row.disconnect(prev.1);
                }
            }
        }
        let sidebar_weak = self.weak_self.clone();
        let handler = row.connect_expanded_notify(move |r| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                // Ignore notifications caused by a reload's teardown/rebuild.
                if sidebar.suppress_expand_notify.get() {
                    return;
                }
                if let Some(so) = r.item().and_downcast::<StringObject>() {
                    let id = so.string().to_string();
                    if r.is_expanded() {
                        sidebar.expanded.borrow_mut().insert(id);
                    } else {
                        sidebar.expanded.borrow_mut().remove(&id);
                    }
                    sidebar.persist_expansion();
                }
            }
        });
        let weak_row = glib::object::ObjectExt::downgrade(&row);
        unsafe {
            item.set_data("expanded-handler", (weak_row, handler));
        }
    }

    fn node_label(&self, id: &str) -> (String, &'static str) {
        let data = self.data.borrow();
        if id == NEW_FILES_ID {
            (
                format!("New Files ({})", data.new_files_count),
                "document-open-recent-symbolic",
            )
        } else if id == MISSING_FILES_ID {
            (
                format!("Missing Files ({})", data.missing_files_count),
                "edit-delete-symbolic",
            )
        } else if id == BANNED_MATCHES_ID {
            (
                format!("Banned Matches ({})", data.banned_matches_count),
                "action-unavailable-symbolic",
            )
        } else if id == VIRTUAL_HEADER_ID {
            ("Virtual Albums".to_string(), "starred-symbolic")
        } else if id == PEOPLE_HEADER_ID {
            (
                format!("People ({})", data.persons.len()),
                "avatar-default-symbolic",
            )
        } else if let Some(gid) = person_group_id_of(id) {
            let name = data
                .person_groups
                .get(&gid)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            let count = data.person_group_members.get(&gid).map(|m| m.len()).unwrap_or(0);
            (format!("{name} ({count})"), "folder-new-symbolic")
        } else if let Some(pid) = person_id_of(id) {
            let name = data
                .persons
                .iter()
                .find(|p| p.id == pid)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let count = data.person_counts.get(&pid).copied().unwrap_or(0);
            (format!("{name} ({count})"), "avatar-default-symbolic")
        } else if id == CHARACTERS_HEADER_ID {
            (
                format!("Characters ({})", data.characters.len()),
                "face-smile-symbolic",
            )
        } else if let Some(gid) = character_group_id_of(id) {
            let name = data
                .character_groups
                .get(&gid)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            let count = data
                .character_group_members
                .get(&gid)
                .map(|m| m.len())
                .unwrap_or(0);
            (format!("{name} ({count})"), "folder-new-symbolic")
        } else if let Some(cid) = character_id_of(id) {
            let name = data
                .characters
                .iter()
                .find(|c| c.id == cid)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let count = data.character_counts.get(&cid).copied().unwrap_or(0);
            (format!("{name} ({count})"), "face-smile-symbolic")
        } else if id == IMMICH_HEADER_ID {
            ("Immich".to_string(), "network-server-symbolic")
        } else if immich_timeline_id_of(id).is_some() {
            ("Timeline".to_string(), "x-office-calendar-symbolic")
        } else if let Some(sid) = immich_server_id_of(id) {
            let name = data
                .immich_servers
                .iter()
                .find(|(x, _)| *x == sid)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            let count = data.immich_albums.get(&sid).map(|a| a.len()).unwrap_or(0);
            (format!("{name} ({count})"), "network-server-symbolic")
        } else if let Some((sid, uuid)) = immich_album_of(id) {
            let entry = data
                .immich_albums
                .get(&sid)
                .into_iter()
                .flatten()
                .find(|(u, _, _)| *u == uuid);
            match entry {
                Some((_, name, count)) => {
                    (format!("{name} ({count})"), "folder-remote-symbolic")
                }
                None => (uuid, "folder-remote-symbolic"),
            }
        } else if let Some(vid) = valbum_id_of(id) {
            let name = data
                .virtual_albums
                .get(&vid)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            let count = data.valbum_counts.get(&vid).copied().unwrap_or(0);
            (format!("{name} ({count})"), "starred-symbolic")
        } else if id == NEW_FOLDERS_ID {
            (
                format!("New folders ({})", data.unassigned.len()),
                "folder-symbolic",
            )
        } else if let Some(aid) = album_id_of(id) {
            let album = data.albums.get(&aid);
            let name = album.map(|a| a.name.clone()).unwrap_or_default();
            let suffix = match album.map(|a| a.kind) {
                Some(crate::model::AlbumKind::Photo) => " (Photo)",
                Some(crate::model::AlbumKind::Art) => " (Art)",
                _ => "",
            };
            (format!("{name}{suffix}"), "folder-new-symbolic")
        } else if let Some(fid) = folder_id_of(id) {
            let name = data
                .folders
                .get(&fid)
                .map(|f| f.name.clone())
                .unwrap_or_default();
            // The photo count is omitted when it is not known (during a scan the
            // sidebar skips the full-table count query, so the map is empty).
            let count_suffix = match data.counts.get(&fid) {
                Some(n) => format!(" ({n})"),
                None => String::new(),
            };
            // Mark a folder that is synced to an Immich album.
            let synced = if data.immich_linked_folders.contains(&fid) {
                " ⇅"
            } else {
                ""
            };
            (
                format!("{name}{count_suffix}{synced}"),
                "image-x-generic-symbolic",
            )
        } else {
            (id.to_string(), "folder-symbolic")
        }
    }

    fn on_selection_changed(&self, sel: &gtk4::MultiSelection) {
        if self.suppress_selection_notify.get() {
            return;
        }
        for id in self.selected_ids(sel) {
            if id == NEW_FILES_ID {
                if let Some(state) = self.state() {
                    state.show_new_files();
                    return;
                }
            }
            if id == MISSING_FILES_ID {
                if let Some(state) = self.state() {
                    state.show_missing_files();
                    return;
                }
            }
            if id == BANNED_MATCHES_ID {
                if let Some(state) = self.state() {
                    state.show_banned_matches();
                    return;
                }
            }
            if id == PEOPLE_HEADER_ID {
                if let Some(state) = self.state() {
                    state.show_faces();
                    return;
                }
            }
            if id == CHARACTERS_HEADER_ID {
                if let Some(state) = self.state() {
                    state.show_characters();
                    return;
                }
            }
            if let Some(vid) = valbum_id_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .virtual_albums
                    .get(&vid)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    state.show_virtual_album(vid, &name);
                    return;
                }
            }
            if let Some(sid) = immich_timeline_id_of(&id) {
                if let Some(state) = self.state() {
                    super::immich::show_timeline(&state, sid, "Timeline");
                    return;
                }
            }
            if let Some(pid) = person_id_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .persons
                    .iter()
                    .find(|p| p.id == pid)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    state.show_person(pid, &name);
                    return;
                }
            }
            if let Some(gid) = person_group_id_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .person_groups
                    .get(&gid)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    state.show_person_group(gid, &name);
                    return;
                }
            }
            if let Some(cid) = character_id_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .characters
                    .iter()
                    .find(|c| c.id == cid)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    state.show_character(cid, &name);
                    return;
                }
            }
            if let Some(gid) = character_group_id_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .character_groups
                    .get(&gid)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    state.show_character_group(gid, &name);
                    return;
                }
            }
            if let Some((sid, uuid)) = immich_album_of(&id) {
                let name = self
                    .data
                    .borrow()
                    .immich_albums
                    .get(&sid)
                    .into_iter()
                    .flatten()
                    .find(|(u, _, _)| *u == uuid)
                    .map(|(_, n, _)| n.clone())
                    .unwrap_or_default();
                if let Some(state) = self.state() {
                    super::immich::show_album(&state, sid, &uuid, &name);
                    return;
                }
            }
            if let Some(fid) = folder_id_of(&id) {
                let folder = self.data.borrow().folders.get(&fid).cloned();
                if let (Some(state), Some(f)) = (self.state(), folder) {
                    state.show_grid();
                    super::app::load_folder_into_grid(&state, &f);
                    return;
                }
            }
        }
    }

    fn selected_ids(&self, sel: &gtk4::MultiSelection) -> Vec<String> {
        let mut out = Vec::new();
        let bitset = sel.selection();
        let n = bitset.size();
        for i in 0..n {
            let pos = bitset.nth(i as u32);
            if let Some(row) = sel.item(pos).and_downcast::<TreeListRow>() {
                if let Some(so) = row.item().and_downcast::<StringObject>() {
                    out.push(so.string().to_string());
                }
            }
        }
        out
    }

    /// The current tree row for keyboard navigation: the first selected row.
    fn current_row(&self) -> Option<(gtk4::MultiSelection, u32, TreeListRow)> {
        let sel = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()?;
        let bitset = sel.selection();
        if bitset.size() == 0 {
            return None;
        }
        let pos = bitset.nth(0);
        let row = sel.item(pos).and_downcast::<TreeListRow>()?;
        Some((sel, pos, row))
    }

    /// Right arrow: open the current node if it can expand.
    fn on_expand_key(&self) -> glib::Propagation {
        let Some((_, _, row)) = self.current_row() else {
            return glib::Propagation::Proceed;
        };
        if row.is_expandable() && !row.is_expanded() {
            row.set_expanded(true);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    /// Left arrow: collapse the current node, or move to the parent node.
    fn on_collapse_key(&self) -> glib::Propagation {
        let Some((sel, _, row)) = self.current_row() else {
            return glib::Propagation::Proceed;
        };
        if row.is_expandable() && row.is_expanded() {
            row.set_expanded(false);
            return glib::Propagation::Stop;
        }
        if let Some(parent) = row.parent() {
            let ppos = parent.position();
            sel.select_item(ppos, true);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    fn selected_folder_ids(&self) -> Vec<i64> {
        let Some(sel) = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
        else {
            return Vec::new();
        };
        self.selected_ids(&sel)
            .into_iter()
            .filter_map(|id| folder_id_of(&id))
            .collect()
    }

    fn selected_album_ids(&self) -> Vec<i64> {
        let Some(sel) = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
        else {
            return Vec::new();
        };
        self.selected_ids(&sel)
            .into_iter()
            .filter_map(|id| album_id_of(&id))
            .collect()
    }

    fn selected_person_ids(&self) -> Vec<i64> {
        let Some(sel) = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
        else {
            return Vec::new();
        };
        self.selected_ids(&sel)
            .into_iter()
            .filter_map(|id| person_id_of(&id))
            .collect()
    }

    fn selected_character_ids(&self) -> Vec<i64> {
        let Some(sel) = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
        else {
            return Vec::new();
        };
        self.selected_ids(&sel)
            .into_iter()
            .filter_map(|id| character_id_of(&id))
            .collect()
    }

    /// Schedule a tree rebuild on the next idle tick. Use this from a
    /// context-menu action so the popover finishes closing and its row widget
    /// is not recycled/destroyed while the action is still being dispatched
    /// (which crashes GTK).
    pub fn reload_deferred(self: &Rc<Self>) {
        let this = self.clone();
        gtk4::glib::idle_add_local_once(move || {
            this.reload();
        });
    }

    /// Force a full rebuild on the next `reload`, ignoring the scan skip guard.
    /// Used at scan end so the final refresh always lands even if the folder and
    /// album counts did not change on the last scan tick.
    pub fn invalidate_signature(&self) {
        self.last_tree_signature.set(None);
    }

    /// Rebuild the tree from the current database state.
    pub fn reload(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        // Skip the rebuild during a scan when nothing visible changed. The scan
        // fires a refresh on a timer; if no new folder or album appeared since
        // the last rebuild, the tree would be identical, so the full `TreeData`
        // build and tree-model splice are wasted work that grows with the
        // library. A user action (add/remove folder, album edit) is not a scan
        // tick, so it always rebuilds and refreshes the baseline below.
        let signature = state.lib.tree_signature().ok();
        if state.scan.running() {
            if let (Some(sig), Some(last)) = (signature, self.last_tree_signature.get()) {
                if sig == last {
                    log::debug!("sidebar.reload: skipped (scan, unchanged {sig:?})");
                    return;
                }
            }
        }
        self.last_tree_signature.set(signature);
        // The splice below replaces every root row with a fresh object, which
        // also discards any expanded subtree's rows — including the currently
        // selected and/or keyboard-focused one. Capture both here so they can
        // be restored once the same rows exist again, so a background refresh
        // (e.g. the live updates during a scan) does not interrupt the user
        // navigating the tree.
        let had_focus = self.list_view.focus_child().is_some();
        let selected_before = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
            .map(|sel| self.selected_ids(&sel))
            .unwrap_or_default();
        // `ListView::grab_focus()` cannot target a specific row under GTK 4.10
        // (see `bind_row`), so record which row should get focus back and let
        // `bind_row` grab it once that row's widget exists again.
        if had_focus {
            if let Some(id) = selected_before.first() {
                *self.pending_focus_id.borrow_mut() = Some(id.clone());
            }
        }

        // Suppress per-row expand/collapse persistence while the tree is torn
        // down and rebuilt, so teardown notifications do not wipe the saved set.
        self.suppress_expand_notify.set(true);
        let t_reload = std::time::Instant::now();
        // During a scan the two full-table aggregates below (a GROUP BY over all
        // photos, and the New Files join) cost O(total photos) and run on every
        // 5-second scan-refresh tick, so they make the scan slow down as the
        // library grows. Skip them while scanning: folder rows show no photo
        // count and New Files shows 0 until the scan ends, when the forced
        // reload recomputes them once.
        let scanning = state.scan.running();
        log::debug!("sidebar.reload: folders()");
        let mut folders = state.lib.folders().unwrap_or_default();
        let t_counts = std::time::Instant::now();
        let counts = if scanning {
            std::collections::HashMap::new()
        } else {
            log::debug!("sidebar.reload: folder_photo_counts()");
            state.lib.folder_photo_counts().unwrap_or_default()
        };
        let counts_ms = t_counts.elapsed();
        log::debug!("sidebar.reload: albums()/folder_albums()/virtual_albums()");
        let mut albums = state.lib.albums().unwrap_or_default();
        let folder_album = state.lib.folder_albums().unwrap_or_default();
        let mut virtual_albums = state.lib.virtual_albums().unwrap_or_default();
        let t_new = std::time::Instant::now();
        let new_files_count = if scanning {
            0
        } else {
            log::debug!("sidebar.reload: new_photos_count()");
            state
                .lib
                .new_photos_count(state.prefs.borrow().new_max_age_secs())
                .unwrap_or(0)
        };
        let new_ms = t_new.elapsed();

        let missing_files_count = state.lib.missing_photo_count().unwrap_or(0);
        let banned_matches_count = state.lib.banned_dup_count().unwrap_or(0);

        folders.sort_by(|a, b| a.name.cmp(&b.name));
        // Show albums alphabetically at every level (case-insensitive). They are
        // pushed into album_children in this order, so children sort too.
        albums.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        virtual_albums.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut data = TreeData {
            counts,
            new_files_count,
            missing_files_count,
            banned_matches_count,
            ..TreeData::default()
        };
        for a in &albums {
            data.albums.insert(a.id, a.clone());
            data.album_children
                .entry(a.parent_id)
                .or_default()
                .push(a.id);
        }
        let t_va = std::time::Instant::now();
        for va in &virtual_albums {
            data.valbum_children
                .entry(va.parent_id)
                .or_default()
                .push(va.id);
            data.valbum_counts.insert(
                va.id,
                state.lib.virtual_album_photo_count(va.id).unwrap_or(0),
            );
            data.virtual_albums.insert(va.id, va.clone());
        }
        let va_ms = t_va.elapsed();
        // Named people from facial recognition.
        for (person, count) in state.lib.persons().unwrap_or_default() {
            data.person_counts.insert(person.id, count);
            data.persons.push(person);
        }
        data.total_faces = state.lib.total_face_count().unwrap_or(0);
        let mut person_groups = state.lib.person_groups().unwrap_or_default();
        person_groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        for g in &person_groups {
            data.person_group_children
                .entry(g.parent_id)
                .or_default()
                .push(g.id);
            data.person_groups.insert(g.id, g.clone());
        }
        data.person_group_members = state.lib.person_group_members().unwrap_or_default();
        for (&gid, members) in &data.person_group_members {
            for &pid in members {
                data.person_memberships.entry(pid).or_default().push(gid);
            }
        }
        // Named stylised characters.
        for (character, count) in state.lib.characters().unwrap_or_default() {
            data.character_counts.insert(character.id, count);
            data.characters.push(character);
        }
        data.total_style_faces = state.lib.total_style_face_count().unwrap_or(0);
        let mut character_groups = state.lib.character_groups().unwrap_or_default();
        character_groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        for g in &character_groups {
            data.character_group_children
                .entry(g.parent_id)
                .or_default()
                .push(g.id);
            data.character_groups.insert(g.id, g.clone());
        }
        data.character_group_members = state.lib.character_group_members().unwrap_or_default();
        for (&gid, members) in &data.character_group_members {
            for &cid in members {
                data.character_memberships.entry(cid).or_default().push(gid);
            }
        }
        for f in &folders {
            data.folders.insert(f.id, f.clone());
            if let Some(&aid) = folder_album.get(&f.id) {
                data.album_folders.entry(aid).or_default().push(f.id);
            } else {
                data.unassigned.push(f.id);
            }
        }

        // Immich servers and their cached albums. The album cache is filled by
        // a background refresh in `super::immich::refresh_albums`.
        let servers = state.lib.immich_servers().unwrap_or_default();
        for s in &servers {
            data.immich_servers.push((s.id, s.name.clone()));
        }
        data.immich_linked_folders = state.lib.linked_immich_folders().unwrap_or_default();
        {
            let cache = state.immich_albums.borrow();
            for s in &servers {
                if let Some(albums) = cache.get(&s.id) {
                    let mut list: Vec<(String, String, i64)> = albums
                        .iter()
                        .map(|a| (a.id.clone(), a.name.clone(), a.asset_count))
                        .collect();
                    // Show Immich albums alphabetically (case-insensitive).
                    list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
                    data.immich_albums.insert(s.id, list);
                }
            }
        }

        self.save_expansion();
        *self.data.borrow_mut() = data;

        // Rebuild the root list: top-level albums, then New folders.
        let mut roots: Vec<String> = Vec::new();
        {
            let data = self.data.borrow();
            if data.new_files_count > 0 {
                roots.push(NEW_FILES_ID.to_string());
            }
            if data.missing_files_count > 0 {
                roots.push(MISSING_FILES_ID.to_string());
            }
            if data.banned_matches_count > 0 {
                roots.push(BANNED_MATCHES_ID.to_string());
            }
            if !data.unassigned.is_empty() {
                roots.push(NEW_FOLDERS_ID.to_string());
            }
            // Virtual albums section, shown above normal folder-albums. Always
            // present so the user has a place to create the first one.
            roots.push(VIRTUAL_HEADER_ID.to_string());
            // People section, shown once any face is detected (named or not).
            if data.total_faces > 0 {
                roots.push(PEOPLE_HEADER_ID.to_string());
            }
            // Characters section, shown once any stylised face is detected.
            if data.total_style_faces > 0 {
                roots.push(CHARACTERS_HEADER_ID.to_string());
            }
            for &aid in data.album_children.get(&0).into_iter().flatten() {
                roots.push(format!("{ALBUM_PREFIX}{aid}"));
            }
            // Immich section, shown below normal albums, only when the user has
            // added at least one server.
            if !data.immich_servers.is_empty() {
                roots.push(IMMICH_HEADER_ID.to_string());
            }
        }
        // Splice the root list only where it actually changed. Replacing every
        // root row (the old `splice(0, n, all)`) forces the tree model to
        // recreate all root rows and their expanded child models, and to redo
        // selection/expansion/focus restoration over the whole realized tree —
        // a cost that grows with the tree. During a scan new folders usually
        // land as children of existing album roots, so the root list is
        // unchanged and this does nothing. When it does change (a new top-level
        // album), only the differing tail is spliced.
        let n = self.list_root.n_items();
        let current: Vec<String> = (0..n)
            .filter_map(|i| self.list_root.string(i).map(|s| s.to_string()))
            .collect();
        if current != roots {
            // Common prefix stays; replace the differing suffix in one splice.
            let common = current
                .iter()
                .zip(roots.iter())
                .take_while(|(a, b)| a == b)
                .count();
            let tail_refs: Vec<&str> =
                roots[common..].iter().map(|s| s.as_str()).collect();
            self.list_root
                .splice(common as u32, n - common as u32, &tail_refs);
        }

        // Refresh the child lists of already-expanded rows in place. Because the
        // root splice above no longer recreates every root each tick, an
        // expanded album's static child `StringList` would otherwise never gain
        // the folders filed under it during a scan. Walk the realized rows and,
        // for each expanded row, splice its child list to match `child_ids`.
        // Only the differing tail is spliced, so an unchanged subtree is free.
        self.refresh_expanded_children();

        self.restore_expansion();
        self.suppress_expand_notify.set(false);

        // Re-select whichever of the previously selected rows still exist, on
        // the freshly created row objects, without re-triggering navigation —
        // the user is already looking at that folder/album; a background
        // refresh reselecting it must not reload the grid again.
        if !selected_before.is_empty() {
            if let Some(sel) = self.list_view.model().and_downcast::<gtk4::MultiSelection>() {
                self.suppress_selection_notify.set(true);
                let mut unselect_rest = true;
                let n = self.tree_model.n_items();
                for i in 0..n {
                    let Some(row) = self.tree_model.row(i) else {
                        continue;
                    };
                    let Some(so) = row.item().and_downcast::<StringObject>() else {
                        continue;
                    };
                    if selected_before.contains(&so.string().to_string()) {
                        sel.select_item(i, unselect_rest);
                        unselect_rest = false;
                    }
                }
                self.suppress_selection_notify.set(false);
                // Keyboard focus, if any, is handed back by `bind_row` once
                // the focused row's widget exists again (see `pending_focus_id`
                // above) — `ListView::grab_focus()` cannot target a specific
                // row under GTK 4.10.
            }
        }

        // The expansion set may have changed during this reload (e.g. a newly
        // created album's parent was marked expanded). Persist the final state.
        self.persist_expansion();
        log::debug!(
            "sidebar.reload {:.2?} (folder_counts {:.2?}, new_photos_count {:.2?}, va_counts {:.2?})",
            t_reload.elapsed(),
            counts_ms,
            new_ms,
            va_ms
        );
    }

    /// Update the child `StringList` of every currently-expanded row so it
    /// matches `child_ids` from the freshly rebuilt `TreeData`. Splices only the
    /// differing tail, so an unchanged subtree costs one comparison. This lets
    /// an expanded album grow live during a scan without recreating the row.
    fn refresh_expanded_children(&self) {
        // Snapshot the expanded rows first. Splicing a child list changes the
        // flattened item count, so collecting row handles up front (GObject
        // references, not indices) keeps the walk stable while we apply splices.
        let n = self.tree_model.n_items();
        let mut targets: Vec<(StringList, String)> = Vec::new();
        for i in 0..n {
            let Some(row) = self.tree_model.row(i) else {
                continue;
            };
            if !row.is_expanded() {
                continue;
            }
            let Some(so) = row.item().and_downcast::<StringObject>() else {
                continue;
            };
            let Some(child_model) = row.children() else {
                continue;
            };
            let Some(list) = child_model.downcast_ref::<StringList>() else {
                continue;
            };
            targets.push((list.clone(), so.string().to_string()));
        }
        for (list, id) in targets {
            let want = self.child_ids(&id);
            let have_n = list.n_items();
            let have: Vec<String> = (0..have_n)
                .filter_map(|j| list.string(j).map(|s| s.to_string()))
                .collect();
            if have == want {
                continue;
            }
            let common = have
                .iter()
                .zip(want.iter())
                .take_while(|(a, b)| a == b)
                .count();
            let tail: Vec<&str> = want[common..].iter().map(|s| s.as_str()).collect();
            list.splice(common as u32, have_n - common as u32, &tail);
        }
    }

    fn save_expansion(&self) {
        let n = self.tree_model.n_items();
        // Only rows currently present in the tree can have their state observed.
        // Rebuild their expanded/collapsed state, but keep ids for rows that are
        // not currently realized (e.g. collapsed ancestors hide descendants) so
        // a deep expansion survives a reload.
        let mut expanded = self.expanded.borrow_mut();
        for i in 0..n {
            if let Some(row) = self.tree_model.row(i) {
                if let Some(so) = row.item().and_downcast::<StringObject>() {
                    let id = so.string().to_string();
                    if row.is_expanded() {
                        expanded.insert(id);
                    } else {
                        expanded.remove(&id);
                    }
                }
            }
        }
        drop(expanded);
        self.persist_expansion();
    }

    /// Save the current expansion set to `library.db` so it survives restarts.
    fn persist_expansion(&self) {
        let Some(state) = self.state() else { return };
        let ids: Vec<String> = self.expanded.borrow().iter().cloned().collect();
        let joined = ids.join("\n");
        let _ = state.lib.set_setting(EXPANDED_SETTING_KEY, &joined);
    }

    /// Load the persisted expansion set from `library.db`. Called once when the
    /// sidebar is bound to state, before the first reload.
    fn load_expansion(&self) {
        let Some(state) = self.state() else { return };
        let raw = state
            .lib
            .get_setting(EXPANDED_SETTING_KEY, "")
            .unwrap_or_default();
        let mut expanded = self.expanded.borrow_mut();
        expanded.clear();
        for id in raw.split('\n') {
            if !id.is_empty() {
                expanded.insert(id.to_string());
            }
        }
    }

    fn restore_expansion(&self) {
        let expanded = self.expanded.borrow().clone();
        for _ in 0..32 {
            let mut changed = false;
            let n = self.tree_model.n_items();
            for i in 0..n {
                if let Some(row) = self.tree_model.row(i) {
                    if let Some(so) = row.item().and_downcast::<StringObject>() {
                        let id = so.string().to_string();
                        if expanded.contains(&id) && !row.is_expanded() {
                            row.set_expanded(true);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn mark_expanded(&self, id: &str) {
        self.expanded.borrow_mut().insert(id.to_string());
    }

    // --- album operations ---

    fn prompt_create_album(self: &Rc<Self>, parent_id: i64) {
        let Some(state) = self.state() else { return };
        let title = if parent_id != 0 {
            "New Sub-Album"
        } else {
            "New Album"
        };
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(&state, None, title, "Album name:", "", move |name| {
            if let Err(e) = state2.lib.create_album(&name, parent_id) {
                show_error(&state2, &e.to_string());
                return;
            }
            if parent_id != 0 {
                this.mark_expanded(&format!("{ALBUM_PREFIX}{parent_id}"));
            }
            this.reload_deferred();
        });
    }

    fn prompt_rename_album(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .albums
            .get(&id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Album",
            "Album name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_album(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_album(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .albums
            .get(&id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Album",
            &format!("Delete album \"{name}\"? Its folders return to New folders; sub-albums are also deleted."),
            move || {
                if let Err(e) = state2.lib.delete_album(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    /// Set an album's Face type (0 inherit, 1 Photo, 2 Art) and refresh the row.
    fn set_album_kind(self: &Rc<Self>, id: i64, kind: i64) {
        if id == 0 {
            return;
        }
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.set_album_kind(id, kind) {
            show_error(&state, &e.to_string());
            return;
        }
        self.reload_deferred();
    }

    /// Scan (or rescan) faces for an album, routed by its Face type.
    fn scan_album_faces(self: &Rc<Self>, id: i64, rescan: bool) {
        if id == 0 {
            return;
        }
        let Some(state) = self.state() else { return };
        super::albumscan::scan_album_faces(&state, id, rescan);
    }

    /// Scan (or rescan) faces for a single folder, routed by its effective
    /// Face type.
    fn scan_folder_faces(self: &Rc<Self>, id: i64, rescan: bool) {
        if id == 0 {
            return;
        }
        let Some(state) = self.state() else { return };
        super::albumscan::scan_folder_faces(&state, id, rescan);
    }

    fn move_folders_to_album(self: &Rc<Self>, fids: &[i64], target: i64) {
        if fids.is_empty() {
            return;
        }
        let Some(state) = self.state() else { return };
        for &fid in fids {
            if let Err(e) = state.lib.add_folder_to_album(fid, target) {
                show_error(&state, &e.to_string());
                return;
            }
        }
        self.mark_expanded(&format!("{ALBUM_PREFIX}{target}"));
        self.reload_deferred();
    }

    fn remove_folders_from_album(self: &Rc<Self>, fids: &[i64]) {
        let Some(state) = self.state() else { return };
        for &fid in fids {
            if let Err(e) = state.lib.remove_folder_from_album(fid) {
                show_error(&state, &e.to_string());
                return;
            }
        }
        self.reload_deferred();
    }

    /// Re-parent one or more albums under `target_album` (drag albums onto an
    /// album). Reload once.
    fn reparent_albums(self: &Rc<Self>, src_albums: &[i64], target_album: i64) {
        let Some(state) = self.state() else { return };
        let mut changed = false;
        for &src_album in src_albums {
            if src_album == target_album {
                continue;
            }
            if let Err(e) = state.lib.set_album_parent(src_album, target_album) {
                show_error(&state, &e.to_string());
                return;
            }
            changed = true;
        }
        if !changed {
            return;
        }
        self.mark_expanded(&format!("{ALBUM_PREFIX}{target_album}"));
        self.reload_deferred();
    }

    // --- virtual album operations ---

    fn prompt_create_virtual_album(self: &Rc<Self>, parent_id: i64) {
        let Some(state) = self.state() else { return };
        let title = if parent_id != 0 {
            "New Sub-Album"
        } else {
            "New Virtual Album"
        };
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(&state, None, title, "Album name:", "", move |name| {
            if let Err(e) = state2.lib.create_virtual_album(&name, parent_id) {
                show_error(&state2, &e.to_string());
                return;
            }
            this.mark_expanded(VIRTUAL_HEADER_ID);
            if parent_id != 0 {
                this.mark_expanded(&format!("{VALBUM_PREFIX}{parent_id}"));
            }
            this.reload_deferred();
        });
    }

    fn prompt_rename_virtual_album(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .virtual_albums
            .get(&id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Virtual Album",
            "Album name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_virtual_album(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_virtual_album(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .virtual_albums
            .get(&id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Virtual Album",
            &format!("Delete virtual album \"{name}\"? Sub-albums are also deleted. Photos on disk are not affected."),
            move || {
                if let Err(e) = state2.lib.delete_virtual_album(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn prompt_rename_person(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .persons
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Person",
            "Person name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_person(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_person(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .persons
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Person",
            &format!("Delete person \"{name}\"? The faces stay but lose the name. Photos on disk are not affected."),
            move || {
                if let Err(e) = state2.lib.delete_person(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    // --- person group operations ---

    fn prompt_create_person_group(self: &Rc<Self>, parent_id: i64) {
        let Some(state) = self.state() else { return };
        let title = if parent_id != 0 {
            "New Sub-Group"
        } else {
            "New Group"
        };
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(&state, None, title, "Group name:", "", move |name| {
            if let Err(e) = state2.lib.create_person_group(&name, parent_id) {
                show_error(&state2, &e.to_string());
                return;
            }
            this.mark_expanded(PEOPLE_HEADER_ID);
            if parent_id != 0 {
                this.mark_expanded(&format!("{PERSON_GROUP_PREFIX}{parent_id}"));
            }
            this.reload_deferred();
        });
    }

    fn prompt_rename_person_group(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .person_groups
            .get(&id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Group",
            "Group name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_person_group(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_person_group(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .person_groups
            .get(&id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Group",
            &format!("Delete group \"{name}\"? People in it are not deleted; sub-groups are also deleted."),
            move || {
                if let Err(e) = state2.lib.delete_person_group(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    /// Add one or more persons to a group. Additive: does not remove any
    /// existing membership.
    fn add_persons_to_group(self: &Rc<Self>, pids: &[i64], target: i64) {
        if pids.is_empty() {
            return;
        }
        let Some(state) = self.state() else { return };
        for &pid in pids {
            if let Err(e) = state.lib.add_person_to_group(pid, target) {
                show_error(&state, &e.to_string());
                return;
            }
        }
        self.mark_expanded(&format!("{PERSON_GROUP_PREFIX}{target}"));
        self.reload_deferred();
    }

    fn remove_person_from_group(self: &Rc<Self>, pid: i64, gid: i64) {
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.remove_person_from_group(pid, gid) {
            show_error(&state, &e.to_string());
            return;
        }
        self.reload_deferred();
    }

    /// Re-parent one or more person groups under `target` (drag a group onto
    /// another group).
    fn reparent_person_groups(self: &Rc<Self>, src_groups: &[i64], target: i64) {
        let Some(state) = self.state() else { return };
        let mut changed = false;
        for &src in src_groups {
            if src == target {
                continue;
            }
            if let Err(e) = state.lib.set_person_group_parent(src, target) {
                show_error(&state, &e.to_string());
                return;
            }
            changed = true;
        }
        if !changed {
            return;
        }
        self.mark_expanded(&format!("{PERSON_GROUP_PREFIX}{target}"));
        self.reload_deferred();
    }

    // --- character group operations ---

    fn prompt_create_character_group(self: &Rc<Self>, parent_id: i64) {
        let Some(state) = self.state() else { return };
        let title = if parent_id != 0 {
            "New Sub-Group"
        } else {
            "New Group"
        };
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(&state, None, title, "Group name:", "", move |name| {
            if let Err(e) = state2.lib.create_character_group(&name, parent_id) {
                show_error(&state2, &e.to_string());
                return;
            }
            this.mark_expanded(CHARACTERS_HEADER_ID);
            if parent_id != 0 {
                this.mark_expanded(&format!("{CHARACTER_GROUP_PREFIX}{parent_id}"));
            }
            this.reload_deferred();
        });
    }

    fn prompt_rename_character_group(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .character_groups
            .get(&id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Group",
            "Group name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_character_group(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_character_group(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .character_groups
            .get(&id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Group",
            &format!("Delete group \"{name}\"? Characters in it are not deleted; sub-groups are also deleted."),
            move || {
                if let Err(e) = state2.lib.delete_character_group(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    /// Add one or more characters to a group. Additive: does not remove any
    /// existing membership.
    fn add_characters_to_group(self: &Rc<Self>, cids: &[i64], target: i64) {
        if cids.is_empty() {
            return;
        }
        let Some(state) = self.state() else { return };
        for &cid in cids {
            if let Err(e) = state.lib.add_character_to_group(cid, target) {
                show_error(&state, &e.to_string());
                return;
            }
        }
        self.mark_expanded(&format!("{CHARACTER_GROUP_PREFIX}{target}"));
        self.reload_deferred();
    }

    fn remove_character_from_group(self: &Rc<Self>, cid: i64, gid: i64) {
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.remove_character_from_group(cid, gid) {
            show_error(&state, &e.to_string());
            return;
        }
        self.reload_deferred();
    }

    /// Re-parent one or more character groups under `target`.
    fn reparent_character_groups(self: &Rc<Self>, src_groups: &[i64], target: i64) {
        let Some(state) = self.state() else { return };
        let mut changed = false;
        for &src in src_groups {
            if src == target {
                continue;
            }
            if let Err(e) = state.lib.set_character_group_parent(src, target) {
                show_error(&state, &e.to_string());
                return;
            }
            changed = true;
        }
        if !changed {
            return;
        }
        self.mark_expanded(&format!("{CHARACTER_GROUP_PREFIX}{target}"));
        self.reload_deferred();
    }

    fn clear_banned_matches(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let n = state.lib.banned_dup_count().unwrap_or(0);
        if n == 0 {
            return;
        }
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Clear Banned Matches",
            &format!("Remove all {n} banned match(es)? These pairs can group as duplicates again."),
            move || match state2.lib.clear_all_dup_bans() {
                Ok(_) => {
                    this.reload_deferred();
                    state2.show_grid();
                }
                Err(e) => show_error(&state2, &e.to_string()),
            },
        );
    }

    fn clear_missing_files(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let n = state.lib.missing_photo_count().unwrap_or(0);
        if n == 0 {
            return;
        }
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Clear Missing Files",
            &format!("Permanently remove {n} missing photo(s) from the library? The files are already gone from disk. This cannot be undone."),
            move || {
                match state2.lib.delete_missing_photos() {
                    Ok(_) => {
                        this.reload_deferred();
                        state2.show_missing_files();
                    }
                    Err(e) => show_error(&state2, &e.to_string()),
                }
            },
        );
    }

    fn delete_person_and_ban(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .persons
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete and Ban Person",
            &format!("Delete person \"{name}\" and ban its faces? A future scan never groups these faces as a person again. Photos on disk are not affected."),
            move || {
                if let Err(e) = state2.lib.delete_person_and_ban(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn prompt_rename_character(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let current = self
            .data
            .borrow()
            .characters
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        prompt_text(
            &state,
            None,
            "Rename Character",
            "Character name:",
            &current,
            move |name| {
                if let Err(e) = state2.lib.rename_character(id, &name) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_character(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .characters
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete Character",
            &format!("Delete character \"{name}\"? The faces stay but lose the name. Photos on disk are not affected."),
            move || {
                if let Err(e) = state2.lib.delete_character(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    fn delete_character_and_ban(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .characters
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        let state2 = state.clone();
        confirm(
            &state,
            None,
            "Delete and Ban Character",
            &format!("Delete character \"{name}\" and ban its faces? A future scan never groups these faces as a character again. Photos on disk are not affected."),
            move || {
                if let Err(e) = state2.lib.delete_character_and_ban(id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                this.reload_deferred();
            },
        );
    }

    /// Re-parent a virtual album under another (drag onto a virtual album).
    fn reparent_virtual_album(self: &Rc<Self>, src: i64, target: i64) {
        if src == target {
            return;
        }
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.set_virtual_album_parent(src, target) {
            show_error(&state, &e.to_string());
            return;
        }
        self.mark_expanded(&format!("{VALBUM_PREFIX}{target}"));
        self.reload_deferred();
    }

    /// Add photos (from a grid drag) to a virtual album, then refresh.
    fn add_photos_to_virtual_album(self: &Rc<Self>, album_id: i64, photo_ids: &[i64]) {
        if photo_ids.is_empty() {
            return;
        }
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.add_photos_to_virtual_album(album_id, photo_ids) {
            show_error(&state, &e.to_string());
            return;
        }
        // A drop leaves the target row selected, so a following click on it
        // would not fire selection-changed and the album would not open. Clear
        // the selection so the next click is a real change.
        if let Some(sel) = self
            .list_view
            .model()
            .and_downcast::<gtk4::MultiSelection>()
        {
            sel.unselect_all();
        }
        self.reload_deferred();
        // If the target album is being viewed, refresh the grid so the added
        // photos appear immediately.
        if state.grid().current_virtual_album() == Some(album_id) {
            state.grid().reload_from_source();
        }
    }

    /// Open the rules editor for a virtual album.
    fn edit_virtual_album_rules(self: &Rc<Self>, id: i64) {
        let Some(state) = self.state() else { return };
        let name = self
            .data
            .borrow()
            .virtual_albums
            .get(&id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let this = self.clone();
        super::vrules::open_rules_editor(&state, id, &name, move || {
            this.reload_deferred();
        });
    }

    // --- context menu ---

    /// Close and unparent the context-menu popover, if any. Called before a
    /// menu action rebuilds the tree so the popover never outlives the row it is
    /// parented to.
    fn dismiss_menu(&self) {
        if let Some(pop) = self.menu_pop.borrow_mut().take() {
            pop.popdown();
            if pop.parent().is_some() {
                pop.unparent();
            }
        }
    }

    fn install_context_menu(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        let vt = glib::VariantTy::STRING;

        // Each action first dismisses the popover, then runs, so the tree can be
        // rebuilt without recycling the row that owns a still-parented popover.
        let weak = Rc::downgrade(self);
        let add = move |name: &str, group: &gio::SimpleActionGroup, f: Rc<dyn Fn(&str)>| {
            let act = gio::SimpleAction::new(name, Some(vt));
            let weak = weak.clone();
            act.connect_activate(move |_, param| {
                if let Some(this) = weak.upgrade() {
                    this.dismiss_menu();
                }
                let target = param.and_then(|p| p.str()).unwrap_or("");
                f(target);
            });
            group.add_action(&act);
        };

        {
            let this = self.clone();
            add(
                "new-album",
                &group,
                Rc::new(move |_| this.prompt_create_album(0)),
            );
        }
        {
            let this = self.clone();
            add(
                "new-subalbum",
                &group,
                Rc::new(move |t| this.prompt_create_album(album_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-album",
                &group,
                Rc::new(move |t| this.prompt_rename_album(album_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-album",
                &group,
                Rc::new(move |t| this.delete_album(album_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "move-to-album",
                &group,
                Rc::new(move |t| {
                    let target = album_id_of(t).unwrap_or(0);
                    let fids = this.selected_folder_ids();
                    this.move_folders_to_album(&fids, target);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "album-kind-inherit",
                &group,
                Rc::new(move |t| this.set_album_kind(album_id_of(t).unwrap_or(0), crate::model::AlbumKind::Inherit.as_i64())),
            );
        }
        {
            let this = self.clone();
            add(
                "album-kind-photo",
                &group,
                Rc::new(move |t| this.set_album_kind(album_id_of(t).unwrap_or(0), crate::model::AlbumKind::Photo.as_i64())),
            );
        }
        {
            let this = self.clone();
            add(
                "album-kind-art",
                &group,
                Rc::new(move |t| this.set_album_kind(album_id_of(t).unwrap_or(0), crate::model::AlbumKind::Art.as_i64())),
            );
        }
        {
            let this = self.clone();
            add(
                "scan-album-faces",
                &group,
                Rc::new(move |t| this.scan_album_faces(album_id_of(t).unwrap_or(0), false)),
            );
        }
        {
            let this = self.clone();
            add(
                "rescan-album-faces",
                &group,
                Rc::new(move |t| this.scan_album_faces(album_id_of(t).unwrap_or(0), true)),
            );
        }
        {
            let this = self.clone();
            add(
                "scan-folder-faces",
                &group,
                Rc::new(move |t| this.scan_folder_faces(folder_id_of(t).unwrap_or(0), false)),
            );
        }
        {
            let this = self.clone();
            add(
                "rescan-folder-faces",
                &group,
                Rc::new(move |t| this.scan_folder_faces(folder_id_of(t).unwrap_or(0), true)),
            );
        }
        {
            let this = self.clone();
            add(
                "refresh-immich",
                &group,
                Rc::new(move |_| {
                    if let Some(state) = this.state() {
                        super::immich::refresh_albums(&state);
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "album-to-local",
                &group,
                Rc::new(move |t| {
                    let Some((sid, uuid)) = immich_album_of(t) else {
                        return;
                    };
                    let Some(state) = this.state() else { return };
                    let name = this
                        .data
                        .borrow()
                        .immich_albums
                        .get(&sid)
                        .into_iter()
                        .flatten()
                        .find(|(u, _, _)| *u == uuid)
                        .map(|(_, n, _)| n.clone())
                        .unwrap_or_else(|| uuid.clone());
                    super::immich::show_album_to_local_dialog(&state, sid, &uuid, &name);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "sync-immich",
                &group,
                Rc::new(move |t| {
                    let Some(fid) = folder_id_of(t) else { return };
                    let Some(state) = this.state() else { return };
                    let name = this
                        .data
                        .borrow()
                        .folders
                        .get(&fid)
                        .map(|f| f.name.clone())
                        .unwrap_or_default();
                    super::immich::show_sync_dialog(&state, fid, &name);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "unsync-immich",
                &group,
                Rc::new(move |t| {
                    let Some(fid) = folder_id_of(t) else { return };
                    let Some(state) = this.state() else { return };
                    if let Err(e) = state.lib.delete_immich_folder_link(fid) {
                        show_error(&state, &e.to_string());
                        return;
                    }
                    this.reload_deferred();
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "syncnow-immich",
                &group,
                Rc::new(move |t| {
                    let Some(fid) = folder_id_of(t) else { return };
                    let Some(state) = this.state() else { return };
                    super::immich::sync_folder_down(&state, fid);
                    if let Ok(Some(link)) = state.lib.immich_folder_link(fid) {
                        if let Ok(Some(server)) = state.lib.immich_server(link.server_id) {
                            let name = state
                                .lib
                                .folder_by_id(fid)
                                .ok()
                                .flatten()
                                .map(|f| f.name)
                                .unwrap_or_default();
                            super::immich::upload_photos(
                                &state,
                                super::immich::UploadSource::Folder(fid),
                                &name,
                                server.id,
                                super::immich::UploadTarget::ExistingAlbum(
                                    link.immich_album_id,
                                ),
                            );
                        }
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "upload-to-immich",
                &group,
                Rc::new(move |t| {
                    let Some(state) = this.state() else { return };
                    // The target id is an `album:<id>` or `folder:<id>` node.
                    if let Some(aid) = album_id_of(t) {
                        let name = this
                            .data
                            .borrow()
                            .albums
                            .get(&aid)
                            .map(|a| a.name.clone())
                            .unwrap_or_default();
                        super::immich::show_upload_dialog(
                            &state,
                            super::immich::UploadSource::Album(aid),
                            &name,
                        );
                    } else if let Some(fid) = folder_id_of(t) {
                        let name = this
                            .data
                            .borrow()
                            .folders
                            .get(&fid)
                            .map(|f| f.name.clone())
                            .unwrap_or_default();
                        super::immich::show_upload_dialog(
                            &state,
                            super::immich::UploadSource::Folder(fid),
                            &name,
                        );
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "remove-folder",
                &group,
                Rc::new(move |t| {
                    let mut fids = this.selected_folder_ids();
                    if fids.is_empty() {
                        if let Some(fid) = folder_id_of(t) {
                            fids.push(fid);
                        }
                    }
                    this.remove_folders_from_album(&fids);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "scan-folder-thumbs",
                &group,
                Rc::new(move |t| {
                    if let (Some(state), Some(fid)) = (this.state(), folder_id_of(t)) {
                        super::enrich::enqueue_folder(&state, fid);
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "rescan-folder-thumbs",
                &group,
                Rc::new(move |t| {
                    if let (Some(state), Some(fid)) = (this.state(), folder_id_of(t)) {
                        super::enrich::rescan_folder(&state, fid);
                    }
                }),
            );
        }

        {
            let this = self.clone();
            add(
                "new-valbum",
                &group,
                Rc::new(move |_| this.prompt_create_virtual_album(0)),
            );
        }
        {
            let this = self.clone();
            add(
                "new-subvalbum",
                &group,
                Rc::new(move |t| this.prompt_create_virtual_album(valbum_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-valbum",
                &group,
                Rc::new(move |t| this.prompt_rename_virtual_album(valbum_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-valbum",
                &group,
                Rc::new(move |t| this.delete_virtual_album(valbum_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "edit-valbum-rules",
                &group,
                Rc::new(move |t| this.edit_virtual_album_rules(valbum_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-person",
                &group,
                Rc::new(move |t| this.prompt_rename_person(person_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-person",
                &group,
                Rc::new(move |t| this.delete_person(person_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-person-ban",
                &group,
                Rc::new(move |t| this.delete_person_and_ban(person_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "new-person-group",
                &group,
                Rc::new(move |_| this.prompt_create_person_group(0)),
            );
        }
        {
            let this = self.clone();
            add(
                "new-person-subgroup",
                &group,
                Rc::new(move |t| this.prompt_create_person_group(person_group_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-person-group",
                &group,
                Rc::new(move |t| this.prompt_rename_person_group(person_group_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-person-group",
                &group,
                Rc::new(move |t| this.delete_person_group(person_group_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "add-selected-persons-to-group",
                &group,
                Rc::new(move |t| {
                    let target = person_group_id_of(t).unwrap_or(0);
                    let pids = this.selected_person_ids();
                    this.add_persons_to_group(&pids, target);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "add-person-to-group",
                &group,
                Rc::new(move |t| {
                    if let Some((gid, pid)) = parse_id_pair(t) {
                        this.add_persons_to_group(&[pid], gid);
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "remove-person-from-group",
                &group,
                Rc::new(move |t| {
                    if let Some((gid, pid)) = parse_id_pair(t) {
                        this.remove_person_from_group(pid, gid);
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "clear-missing",
                &group,
                Rc::new(move |_| this.clear_missing_files()),
            );
        }
        {
            let this = self.clone();
            add(
                "clear-banned",
                &group,
                Rc::new(move |_| this.clear_banned_matches()),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-character",
                &group,
                Rc::new(move |t| this.prompt_rename_character(character_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-character",
                &group,
                Rc::new(move |t| this.delete_character(character_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-character-ban",
                &group,
                Rc::new(move |t| this.delete_character_and_ban(character_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "new-character-group",
                &group,
                Rc::new(move |_| this.prompt_create_character_group(0)),
            );
        }
        {
            let this = self.clone();
            add(
                "new-character-subgroup",
                &group,
                Rc::new(move |t| {
                    this.prompt_create_character_group(character_group_id_of(t).unwrap_or(0))
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "rename-character-group",
                &group,
                Rc::new(move |t| {
                    this.prompt_rename_character_group(character_group_id_of(t).unwrap_or(0))
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "delete-character-group",
                &group,
                Rc::new(move |t| this.delete_character_group(character_group_id_of(t).unwrap_or(0))),
            );
        }
        {
            let this = self.clone();
            add(
                "add-selected-characters-to-group",
                &group,
                Rc::new(move |t| {
                    let target = character_group_id_of(t).unwrap_or(0);
                    let cids = this.selected_character_ids();
                    this.add_characters_to_group(&cids, target);
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "add-character-to-group",
                &group,
                Rc::new(move |t| {
                    if let Some((gid, cid)) = parse_id_pair(t) {
                        this.add_characters_to_group(&[cid], gid);
                    }
                }),
            );
        }
        {
            let this = self.clone();
            add(
                "remove-character-from-group",
                &group,
                Rc::new(move |t| {
                    if let Some((gid, cid)) = parse_id_pair(t) {
                        this.remove_character_from_group(cid, gid);
                    }
                }),
            );
        }

        self.list_view.insert_action_group("sidebar", Some(&group));
    }

    /// Make a folder or album row draggable, and album rows drop targets.
    /// Dropping folders onto an album moves them into it; dropping an album onto
    /// an album makes it a sub-album. The dragged node id travels as a string.
    fn attach_row_drag(self: &Rc<Self>, expander: &TreeExpander) {
        let src = gtk4::DragSource::new();
        src.set_actions(gdk::DragAction::MOVE);
        let expander_weak = expander.downgrade();
        src.connect_prepare(move |_, _, _| {
            let expander = expander_weak.upgrade()?;
            let id = expander.widget_name().to_string();
            if album_id_of(&id).is_none()
                && folder_id_of(&id).is_none()
                && valbum_id_of(&id).is_none()
                && person_id_of(&id).is_none()
                && person_group_id_of(&id).is_none()
                && character_id_of(&id).is_none()
                && character_group_id_of(&id).is_none()
            {
                return None;
            }
            let value = id.to_value();
            Some(gdk::ContentProvider::for_value(&value))
        });
        expander.add_controller(src);

        let tgt = gtk4::DropTarget::new(
            glib::types::Type::STRING,
            gdk::DragAction::MOVE | gdk::DragAction::COPY,
        );
        let this = self.clone();
        let expander_weak = expander.downgrade();
        tgt.connect_drop(move |_, value, _, _| {
            let Some(expander) = expander_weak.upgrade() else {
                return false;
            };
            let target_id = expander.widget_name().to_string();
            let dragged: String = match value.get() {
                Ok(s) => s,
                Err(_) => return false,
            };
            // Dropping photos from the grid onto a virtual album adds them.
            if let Some(ids) = photo_ids_of(&dragged) {
                if let Some(target_v) = valbum_id_of(&target_id) {
                    this.add_photos_to_virtual_album(target_v, &ids);
                    return true;
                }
                return false;
            }
            // Dropping a virtual album onto another makes it a sub-album.
            if let (Some(target_v), Some(src_v)) =
                (valbum_id_of(&target_id), valbum_id_of(&dragged))
            {
                this.reparent_virtual_album(src_v, target_v);
                return true;
            }
            // Dropping a person onto a person-group node adds membership
            // (additive: does not evict other memberships). Dropping a
            // person-group onto another re-parents it (subgroup nesting).
            if let Some(target_group) = person_group_id_of(&target_id) {
                if let Some(src_person) = person_id_of(&dragged) {
                    let mut pids = this.selected_person_ids();
                    if !pids.contains(&src_person) {
                        pids.push(src_person);
                    }
                    this.add_persons_to_group(&pids, target_group);
                    return true;
                }
                if let Some(src_group) = person_group_id_of(&dragged) {
                    this.reparent_person_groups(&[src_group], target_group);
                    return true;
                }
                return false;
            }
            // Mirrors the person-group handling above for characters.
            if let Some(target_group) = character_group_id_of(&target_id) {
                if let Some(src_character) = character_id_of(&dragged) {
                    let mut cids = this.selected_character_ids();
                    if !cids.contains(&src_character) {
                        cids.push(src_character);
                    }
                    this.add_characters_to_group(&cids, target_group);
                    return true;
                }
                if let Some(src_group) = character_group_id_of(&dragged) {
                    this.reparent_character_groups(&[src_group], target_group);
                    return true;
                }
                return false;
            }
            let Some(target_album) = album_id_of(&target_id) else {
                return false;
            };
            if let Some(src_album) = album_id_of(&dragged) {
                let mut aids = this.selected_album_ids();
                if !aids.contains(&src_album) {
                    aids.push(src_album);
                }
                this.reparent_albums(&aids, target_album);
                true
            } else if let Some(fid) = folder_id_of(&dragged) {
                let mut fids = this.selected_folder_ids();
                if fids.is_empty() {
                    fids.push(fid);
                }
                this.move_folders_to_album(&fids, target_album);
                true
            } else {
                false
            }
        });
        expander.add_controller(tgt);
    }

    fn attach_row_menu(self: &Rc<Self>, expander: &TreeExpander) {
        let click = GestureClick::new();
        click.set_button(gdk::BUTTON_SECONDARY);
        let this = self.clone();
        let expander_weak = expander.downgrade();
        click.connect_pressed(move |_, _, x, y| {
            if let Some(expander) = expander_weak.upgrade() {
                let id = expander.widget_name().to_string();
                if !id.is_empty() {
                    this.show_row_menu(&id, &expander, x, y);
                }
            }
        });
        expander.add_controller(click);
    }

    /// Double-click an album (folder-album, virtual album, or the Virtual
    /// Albums header) to expand or collapse its children.
    fn attach_row_activate(self: &Rc<Self>, expander: &TreeExpander) {
        let click = GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        let expander_weak = expander.downgrade();
        click.connect_pressed(move |gesture, n_press, _, _| {
            if n_press < 2 {
                return;
            }
            let Some(expander) = expander_weak.upgrade() else {
                return;
            };
            let id = expander.widget_name().to_string();
            // Only nodes with children toggle: albums, virtual albums, and the
            // virtual header. Folders and leaf rows are ignored.
            let toggles = album_id_of(&id).is_some()
                || valbum_id_of(&id).is_some()
                || person_group_id_of(&id).is_some()
                || character_group_id_of(&id).is_some()
                || id == VIRTUAL_HEADER_ID
                || id == PEOPLE_HEADER_ID
                || id == CHARACTERS_HEADER_ID
                || id == IMMICH_HEADER_ID
                || immich_server_id_of(&id).is_some();
            if !toggles {
                return;
            }
            if let Some(row) = expander.list_row() {
                if row.is_expandable() {
                    row.set_expanded(!row.is_expanded());
                    // Consume the event so it does not also select/activate.
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
        });
        expander.add_controller(click);
    }

    fn show_row_menu(&self, id: &str, expander: &TreeExpander, x: f64, y: f64) {
        let Some(menu) = self.build_row_menu(id) else {
            return;
        };
        if let Some(old) = self.menu_pop.borrow_mut().take() {
            if old.parent().is_some() {
                old.unparent();
            }
        }
        let pop = PopoverMenu::from_model_full(&menu, gtk4::PopoverMenuFlags::NESTED);
        pop.set_has_arrow(false);
        // Parent the popover to the stable ListView, not the per-row expander.
        // A reload during a scan tears down and rebuilds the row widgets. A
        // popover parented to a destroyed expander causes a use-after-free and
        // a crash. The ListView lives for the whole sidebar lifetime.
        // Translate the pointer position from the expander to the ListView.
        let (lx, ly) = expander
            .translate_coordinates(&self.list_view, x, y)
            .unwrap_or((x, y));
        pop.set_parent(&self.list_view);
        pop.set_position(gtk4::PositionType::Right);
        let rect = gdk::Rectangle::new(lx as i32, ly as i32, 1, 1);
        pop.set_pointing_to(Some(&rect));
        pop.popup();
        *self.menu_pop.borrow_mut() = Some(pop);
    }

    fn build_row_menu(&self, id: &str) -> Option<gio::Menu> {
        let menu = gio::Menu::new();
        let data = self.data.borrow();
        if id == VIRTUAL_HEADER_ID {
            menu.append(
                Some("New Virtual Album…"),
                Some(&detailed("new-valbum", id)),
            );
        } else if id == MISSING_FILES_ID {
            menu.append(
                Some("Clear Missing Files…"),
                Some(&detailed("clear-missing", id)),
            );
        } else if id == BANNED_MATCHES_ID {
            menu.append(
                Some("Clear Banned Matches…"),
                Some(&detailed("clear-banned", id)),
            );
        } else if valbum_id_of(id).is_some() {
            menu.append(Some("New Sub-Album…"), Some(&detailed("new-subvalbum", id)));
            menu.append(Some("Rename Album…"), Some(&detailed("rename-valbum", id)));
            menu.append(
                Some("Edit Rules…"),
                Some(&detailed("edit-valbum-rules", id)),
            );
            menu.append(Some("Delete Album"), Some(&detailed("delete-valbum", id)));
        } else if id == PEOPLE_HEADER_ID {
            menu.append(Some("New Group…"), Some(&detailed("new-person-group", id)));
        } else if id == CHARACTERS_HEADER_ID {
            menu.append(
                Some("New Group…"),
                Some(&detailed("new-character-group", id)),
            );
        } else if person_group_id_of(id).is_some() {
            menu.append(
                Some("New Sub-Group…"),
                Some(&detailed("new-person-subgroup", id)),
            );
            menu.append(
                Some("Rename Group…"),
                Some(&detailed("rename-person-group", id)),
            );
            menu.append(Some("Delete Group"), Some(&detailed("delete-person-group", id)));
            if !self.selected_person_ids().is_empty() {
                menu.append(
                    Some("Add selected here"),
                    Some(&detailed("add-selected-persons-to-group", id)),
                );
            }
        } else if character_group_id_of(id).is_some() {
            menu.append(
                Some("New Sub-Group…"),
                Some(&detailed("new-character-subgroup", id)),
            );
            menu.append(
                Some("Rename Group…"),
                Some(&detailed("rename-character-group", id)),
            );
            menu.append(
                Some("Delete Group"),
                Some(&detailed("delete-character-group", id)),
            );
            if !self.selected_character_ids().is_empty() {
                menu.append(
                    Some("Add selected here"),
                    Some(&detailed("add-selected-characters-to-group", id)),
                );
            }
        } else if let Some(pid) = person_id_of(id) {
            menu.append(Some("Rename Person…"), Some(&detailed("rename-person", id)));
            menu.append(Some("Delete Person"), Some(&detailed("delete-person", id)));
            menu.append(
                Some("Delete and Ban Person"),
                Some(&detailed("delete-person-ban", id)),
            );
            if !data.person_groups.is_empty() {
                let submenu = self.build_add_to_person_group_submenu(&data, 0, pid);
                menu.append_submenu(Some("Add to Group"), &submenu);
            }
            if let Some(remove_menu) = self.build_remove_from_person_group_menu(&data, pid) {
                menu.append_submenu(Some("Remove from Group"), &remove_menu);
            }
        } else if let Some(cid) = character_id_of(id) {
            menu.append(
                Some("Rename Character…"),
                Some(&detailed("rename-character", id)),
            );
            menu.append(
                Some("Delete Character"),
                Some(&detailed("delete-character", id)),
            );
            menu.append(
                Some("Delete and Ban Character"),
                Some(&detailed("delete-character-ban", id)),
            );
            if !data.character_groups.is_empty() {
                let submenu = self.build_add_to_character_group_submenu(&data, 0, cid);
                menu.append_submenu(Some("Add to Group"), &submenu);
            }
            if let Some(remove_menu) = self.build_remove_from_character_group_menu(&data, cid) {
                menu.append_submenu(Some("Remove from Group"), &remove_menu);
            }
        } else if album_id_of(id).is_some() {
            menu.append(Some("New Sub-Album…"), Some(&detailed("new-subalbum", id)));
            menu.append(Some("Rename Album…"), Some(&detailed("rename-album", id)));
            menu.append(Some("Delete Album"), Some(&detailed("delete-album", id)));
            if !self.selected_folder_ids().is_empty() {
                menu.append(
                    Some("Move selected here"),
                    Some(&detailed("move-to-album", id)),
                );
            }
            // Face type submenu. The title shows the album's own explicit kind
            // (Inherit/Photo/Art). The three items set the kind.
            if let Some(aid) = album_id_of(id) {
                let own = data
                    .albums
                    .get(&aid)
                    .map(|a| a.kind)
                    .unwrap_or(crate::model::AlbumKind::Inherit);
                let title = match own {
                    crate::model::AlbumKind::Photo => "Face type: Photo",
                    crate::model::AlbumKind::Art => "Face type: Art",
                    crate::model::AlbumKind::Inherit => "Face type: Inherit",
                };
                let sub = gio::Menu::new();
                sub.append(Some("Inherit from parent"), Some(&detailed("album-kind-inherit", id)));
                sub.append(Some("Photo"), Some(&detailed("album-kind-photo", id)));
                sub.append(Some("Art"), Some(&detailed("album-kind-art", id)));
                menu.append_submenu(Some(title), &sub);
                menu.append(
                    Some("Scan faces in album"),
                    Some(&detailed("scan-album-faces", id)),
                );
                menu.append(
                    Some("Rescan faces in album"),
                    Some(&detailed("rescan-album-faces", id)),
                );
            }
            // Offer upload only when a server exists and the album has photos.
            if let Some(aid) = album_id_of(id) {
                if !data.immich_servers.is_empty() && data.album_photo_count(aid) > 0 {
                    menu.append(
                        Some("Upload to Immich…"),
                        Some(&detailed("upload-to-immich", id)),
                    );
                }
            }
        } else if folder_id_of(id).is_some() {
            if data.albums.is_empty() {
                menu.append(Some("New Album…"), Some(&detailed("new-album", id)));
            } else {
                let submenu = self.build_move_submenu(&data, 0);
                menu.append_submenu(Some("Move to Album"), &submenu);
            }
            menu.append(
                Some("Remove from Album"),
                Some(&detailed("remove-folder", id)),
            );
            // Thumbnail scan for this folder.
            menu.append(
                Some("Scan all thumbnails (unfinished)"),
                Some(&detailed("scan-folder-thumbs", id)),
            );
            menu.append(
                Some("Rescan all thumbnails (all)"),
                Some(&detailed("rescan-folder-thumbs", id)),
            );
            menu.append(
                Some("Scan faces in folder"),
                Some(&detailed("scan-folder-faces", id)),
            );
            menu.append(
                Some("Rescan faces in folder"),
                Some(&detailed("rescan-folder-faces", id)),
            );
            // Offer upload only when a server exists and the folder has photos.
            if let Some(fid) = folder_id_of(id) {
                if !data.immich_servers.is_empty() && data.folder_photo_count(fid) > 0 {
                    menu.append(
                        Some("Upload to Immich…"),
                        Some(&detailed("upload-to-immich", id)),
                    );
                }
                // Sync (link) the folder to an Immich album for auto-upload.
                if !data.immich_servers.is_empty() {
                    if data.immich_linked_folders.contains(&fid) {
                        menu.append(
                            Some("Sync Now"),
                            Some(&detailed("syncnow-immich", id)),
                        );
                        menu.append(
                            Some("Unsync from Immich"),
                            Some(&detailed("unsync-immich", id)),
                        );
                    } else {
                        menu.append(
                            Some("Sync with Immich album…"),
                            Some(&detailed("sync-immich", id)),
                        );
                    }
                }
            }
        } else if id == NEW_FOLDERS_ID {
            menu.append(Some("New Album…"), Some(&detailed("new-album", id)));
        } else if id == IMMICH_HEADER_ID || immich_server_id_of(id).is_some() {
            menu.append(
                Some("Refresh Albums"),
                Some(&detailed("refresh-immich", id)),
            );
        } else if immich_album_of(id).is_some() {
            menu.append(
                Some("Sync to local folder…"),
                Some(&detailed("album-to-local", id)),
            );
        } else {
            return None;
        }
        Some(menu)
    }

    fn build_move_submenu(&self, data: &TreeData, parent_id: i64) -> gio::Menu {
        let m = gio::Menu::new();
        let mut children: Vec<i64> = data
            .album_children
            .get(&parent_id)
            .cloned()
            .unwrap_or_default();
        children.sort_by(|a, b| {
            let na = data.albums.get(a).map(|x| x.name.as_str()).unwrap_or("");
            let nb = data.albums.get(b).map(|x| x.name.as_str()).unwrap_or("");
            na.cmp(nb)
        });
        for aid in children {
            let target = format!("{ALBUM_PREFIX}{aid}");
            let name = data
                .albums
                .get(&aid)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            if data
                .album_children
                .get(&aid)
                .map(|c| c.is_empty())
                .unwrap_or(true)
            {
                m.append(Some(&name), Some(&detailed("move-to-album", &target)));
            } else {
                let sub = gio::Menu::new();
                let here = gio::Menu::new();
                here.append(Some("Move here"), Some(&detailed("move-to-album", &target)));
                sub.append_section(None, &here);
                sub.append_section(None, &self.build_move_submenu(data, aid));
                m.append_submenu(Some(&name), &sub);
            }
        }
        m
    }

    /// Build a nested "Add to Group" menu for a person, listing every person
    /// group under `parent_id`. Unlike `build_move_submenu`, adding is
    /// additive (does not evict the person from any other group), so the
    /// target encodes both the destination group and the specific person
    /// (`"<group_id>:<person_id>"`) rather than relying on tree selection.
    fn build_add_to_person_group_submenu(&self, data: &TreeData, parent_id: i64, pid: i64) -> gio::Menu {
        let m = gio::Menu::new();
        let mut children: Vec<i64> = data
            .person_group_children
            .get(&parent_id)
            .cloned()
            .unwrap_or_default();
        children.sort_by(|a, b| {
            let na = data.person_groups.get(a).map(|g| g.name.as_str()).unwrap_or("");
            let nb = data.person_groups.get(b).map(|g| g.name.as_str()).unwrap_or("");
            na.cmp(nb)
        });
        for gid in children {
            let target = format!("{gid}:{pid}");
            let name = data
                .person_groups
                .get(&gid)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            if data
                .person_group_children
                .get(&gid)
                .map(|c| c.is_empty())
                .unwrap_or(true)
            {
                m.append(Some(&name), Some(&detailed("add-person-to-group", &target)));
            } else {
                let sub = gio::Menu::new();
                let here = gio::Menu::new();
                here.append(Some("Add here"), Some(&detailed("add-person-to-group", &target)));
                sub.append_section(None, &here);
                sub.append_section(None, &self.build_add_to_person_group_submenu(data, gid, pid));
                m.append_submenu(Some(&name), &sub);
            }
        }
        m
    }

    /// The groups a person directly belongs to, as a flat "Remove from Group"
    /// menu, or `None` when the person belongs to no group.
    fn build_remove_from_person_group_menu(&self, data: &TreeData, pid: i64) -> Option<gio::Menu> {
        let gids = data.person_memberships.get(&pid)?;
        if gids.is_empty() {
            return None;
        }
        let mut items: Vec<(i64, String)> = gids
            .iter()
            .map(|&gid| {
                (
                    gid,
                    data.person_groups.get(&gid).map(|g| g.name.clone()).unwrap_or_default(),
                )
            })
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        let m = gio::Menu::new();
        for (gid, name) in items {
            let target = format!("{gid}:{pid}");
            m.append(Some(&name), Some(&detailed("remove-person-from-group", &target)));
        }
        Some(m)
    }

    /// Mirrors `build_add_to_person_group_submenu` for characters.
    fn build_add_to_character_group_submenu(
        &self,
        data: &TreeData,
        parent_id: i64,
        cid: i64,
    ) -> gio::Menu {
        let m = gio::Menu::new();
        let mut children: Vec<i64> = data
            .character_group_children
            .get(&parent_id)
            .cloned()
            .unwrap_or_default();
        children.sort_by(|a, b| {
            let na = data.character_groups.get(a).map(|g| g.name.as_str()).unwrap_or("");
            let nb = data.character_groups.get(b).map(|g| g.name.as_str()).unwrap_or("");
            na.cmp(nb)
        });
        for gid in children {
            let target = format!("{gid}:{cid}");
            let name = data
                .character_groups
                .get(&gid)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            if data
                .character_group_children
                .get(&gid)
                .map(|c| c.is_empty())
                .unwrap_or(true)
            {
                m.append(Some(&name), Some(&detailed("add-character-to-group", &target)));
            } else {
                let sub = gio::Menu::new();
                let here = gio::Menu::new();
                here.append(
                    Some("Add here"),
                    Some(&detailed("add-character-to-group", &target)),
                );
                sub.append_section(None, &here);
                sub.append_section(None, &self.build_add_to_character_group_submenu(data, gid, cid));
                m.append_submenu(Some(&name), &sub);
            }
        }
        m
    }

    /// Mirrors `build_remove_from_person_group_menu` for characters.
    fn build_remove_from_character_group_menu(&self, data: &TreeData, cid: i64) -> Option<gio::Menu> {
        let gids = data.character_memberships.get(&cid)?;
        if gids.is_empty() {
            return None;
        }
        let mut items: Vec<(i64, String)> = gids
            .iter()
            .map(|&gid| {
                (
                    gid,
                    data.character_groups
                        .get(&gid)
                        .map(|g| g.name.clone())
                        .unwrap_or_default(),
                )
            })
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        let m = gio::Menu::new();
        for (gid, name) in items {
            let target = format!("{gid}:{cid}");
            m.append(
                Some(&name),
                Some(&detailed("remove-character-from-group", &target)),
            );
        }
        Some(m)
    }

    /// Select the first folder in the tree, if any, and return it.
    pub fn select_first_folder(self: &Rc<Self>) -> Option<Folder> {
        let data = self.data.borrow();
        // Prefer an unassigned folder; else the first album folder.
        let fid = data
            .unassigned
            .first()
            .copied()
            .or_else(|| data.album_folders.values().flatten().next().copied())?;
        data.folders.get(&fid).cloned()
    }
}

/// Build a "sidebar.<action>::<target>" detailed action string.
fn detailed(action: &str, target: &str) -> String {
    format!("sidebar.{action}::{target}")
}

fn album_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(ALBUM_PREFIX).and_then(|n| n.parse().ok())
}

fn folder_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(FOLDER_PREFIX).and_then(|n| n.parse().ok())
}

fn valbum_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(VALBUM_PREFIX).and_then(|n| n.parse().ok())
}

fn person_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(PERSON_PREFIX).and_then(|n| n.parse().ok())
}

fn person_group_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(PERSON_GROUP_PREFIX)
        .and_then(|n| n.parse().ok())
}

fn character_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(CHARACTER_PREFIX).and_then(|n| n.parse().ok())
}

fn character_group_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(CHARACTER_GROUP_PREFIX)
        .and_then(|n| n.parse().ok())
}

/// Parse a `"<a>:<b>"` action-target string of two ids. Used for the
/// "Add to Group ▸" and "Remove from Group ▸" submenus, always in
/// `<group_id>:<person_or_character_id>` order, where a group id alone is not
/// enough context (a person/character can be reached under several group rows
/// at once, and "add" needs a specific person/character regardless of the
/// current tree selection).
fn parse_id_pair(target: &str) -> Option<(i64, i64)> {
    let (a, b) = target.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn immich_server_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(IMMICH_SERVER_PREFIX)
        .and_then(|n| n.parse().ok())
}

/// Parse an `immichtimeline:<server_id>` node id.
fn immich_timeline_id_of(id: &str) -> Option<i64> {
    id.strip_prefix(IMMICH_TIMELINE_PREFIX)
        .and_then(|n| n.parse().ok())
}

/// Parse an `immichalbum:<server_id>:<album_uuid>` node id.
fn immich_album_of(id: &str) -> Option<(i64, String)> {
    let rest = id.strip_prefix(IMMICH_ALBUM_PREFIX)?;
    let (sid, uuid) = rest.split_once(':')?;
    let server_id: i64 = sid.parse().ok()?;
    if uuid.is_empty() {
        return None;
    }
    Some((server_id, uuid.to_string()))
}

/// Parse a grid drag payload `photos:<id>,<id>,...` into photo ids. Returns
/// `None` when the payload is not a photo drag.
fn photo_ids_of(payload: &str) -> Option<Vec<i64>> {
    let rest = payload.strip_prefix("photos:")?;
    let ids: Vec<i64> = rest
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    Some(ids)
}
