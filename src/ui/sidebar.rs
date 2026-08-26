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

use crate::model::{Album, Folder};

use super::dialogs::{confirm, prompt_text};
use super::state::{show_error, AppState};

const NEW_FOLDERS_ID: &str = "newfolders";
const NEW_FILES_ID: &str = "newfiles";
const ALBUM_PREFIX: &str = "album:";
const FOLDER_PREFIX: &str = "folder:";

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
            }
        });

        sidebar.install_context_menu();
        sidebar
    }

    /// Give the sidebar access to shared state.
    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state);
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
        if let Some(aid) = album_id_of(id) {
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
    }

    fn node_label(&self, id: &str) -> (String, &'static str) {
        let data = self.data.borrow();
        if id == NEW_FILES_ID {
            (
                format!("New Files ({})", data.new_files_count),
                "document-open-recent-symbolic",
            )
        } else if id == NEW_FOLDERS_ID {
            (
                format!("New folders ({})", data.unassigned.len()),
                "folder-symbolic",
            )
        } else if let Some(aid) = album_id_of(id) {
            (
                data.albums.get(&aid).map(|a| a.name.clone()).unwrap_or_default(),
                "folder-new-symbolic",
            )
        } else if let Some(fid) = folder_id_of(id) {
            let name = data.folders.get(&fid).map(|f| f.name.clone()).unwrap_or_default();
            let count = data.counts.get(&fid).copied().unwrap_or(0);
            (format!("{name} ({count})"), "image-x-generic-symbolic")
        } else {
            (id.to_string(), "folder-symbolic")
        }
    }

    fn on_selection_changed(&self, sel: &gtk4::MultiSelection) {
        for id in self.selected_ids(sel) {
            if id == NEW_FILES_ID {
                if let Some(state) = self.state() {
                    state.show_new_files();
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

    fn selected_folder_ids(&self) -> Vec<i64> {
        let Some(sel) = self.list_view.model().and_downcast::<gtk4::MultiSelection>() else {
            return Vec::new();
        };
        self.selected_ids(&sel)
            .into_iter()
            .filter_map(|id| folder_id_of(&id))
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

    /// Rebuild the tree from the current database state.
    pub fn reload(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let mut folders = state.lib.folders().unwrap_or_default();
        let counts = state.lib.folder_photo_counts().unwrap_or_default();
        let albums = state.lib.albums().unwrap_or_default();
        let folder_album = state.lib.folder_albums().unwrap_or_default();
        let new_files_count = state
            .lib
            .new_photos_count(super::newfiles::NEW_MAX_AGE_SECS)
            .unwrap_or(0);

        folders.sort_by(|a, b| a.name.cmp(&b.name));

        let mut data = TreeData {
            counts,
            new_files_count,
            ..TreeData::default()
        };
        for a in &albums {
            data.albums.insert(a.id, a.clone());
            data.album_children.entry(a.parent_id).or_default().push(a.id);
        }
        for f in &folders {
            data.folders.insert(f.id, f.clone());
            if let Some(&aid) = folder_album.get(&f.id) {
                data.album_folders.entry(aid).or_default().push(f.id);
            } else {
                data.unassigned.push(f.id);
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
            if !data.unassigned.is_empty() {
                roots.push(NEW_FOLDERS_ID.to_string());
            }
            for &aid in data.album_children.get(&0).into_iter().flatten() {
                roots.push(format!("{ALBUM_PREFIX}{aid}"));
            }
        }
        let n = self.list_root.n_items();
        let root_refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
        self.list_root.splice(0, n, &root_refs);

        self.restore_expansion();
    }

    fn save_expansion(&self) {
        let n = self.tree_model.n_items();
        let mut expanded = self.expanded.borrow_mut();
        for i in 0..n {
            if let Some(row) = self.tree_model.row(i) {
                if row.is_expanded() {
                    if let Some(so) = row.item().and_downcast::<StringObject>() {
                        expanded.insert(so.string().to_string());
                    }
                }
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
            this.reload();
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
        prompt_text(&state, None, "Rename Album", "Album name:", &current, move |name| {
            if let Err(e) = state2.lib.rename_album(id, &name) {
                show_error(&state2, &e.to_string());
                return;
            }
            this.reload();
        });
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
                this.reload();
            },
        );
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

    /// Re-parent `src_album` under `target_album` (drag an album onto an album).
    fn reparent_album(self: &Rc<Self>, src_album: i64, target_album: i64) {
        if src_album == target_album {
            return;
        }
        let Some(state) = self.state() else { return };
        if let Err(e) = state.lib.set_album_parent(src_album, target_album) {
            show_error(&state, &e.to_string());
            return;
        }
        self.mark_expanded(&format!("{ALBUM_PREFIX}{target_album}"));
        self.reload_deferred();
    }

    // --- context menu ---

    fn install_context_menu(self: &Rc<Self>) {
        let group = gio::SimpleActionGroup::new();
        let vt = glib::VariantTy::STRING;

        let add = |name: &str, group: &gio::SimpleActionGroup, f: Rc<dyn Fn(&str)>| {
            let act = gio::SimpleAction::new(name, Some(vt));
            act.connect_activate(move |_, param| {
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
            if album_id_of(&id).is_none() && folder_id_of(&id).is_none() {
                return None;
            }
            let value = id.to_value();
            Some(gdk::ContentProvider::for_value(&value))
        });
        expander.add_controller(src);

        let tgt = gtk4::DropTarget::new(glib::types::Type::STRING, gdk::DragAction::MOVE);
        let this = self.clone();
        let expander_weak = expander.downgrade();
        tgt.connect_drop(move |_, value, _, _| {
            let Some(expander) = expander_weak.upgrade() else {
                return false;
            };
            let target_id = expander.widget_name().to_string();
            let Some(target_album) = album_id_of(&target_id) else {
                return false;
            };
            let dragged: String = match value.get() {
                Ok(s) => s,
                Err(_) => return false,
            };
            if let Some(src_album) = album_id_of(&dragged) {
                this.reparent_album(src_album, target_album);
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
        pop.set_parent(expander);
        pop.set_position(gtk4::PositionType::Right);
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        pop.set_pointing_to(Some(&rect));
        pop.popup();
        *self.menu_pop.borrow_mut() = Some(pop);
    }

    fn build_row_menu(&self, id: &str) -> Option<gio::Menu> {
        let menu = gio::Menu::new();
        let data = self.data.borrow();
        if album_id_of(id).is_some() {
            menu.append(Some("New Sub-Album…"), Some(&detailed("new-subalbum", id)));
            menu.append(Some("Rename Album…"), Some(&detailed("rename-album", id)));
            menu.append(Some("Delete Album"), Some(&detailed("delete-album", id)));
            if !self.selected_folder_ids().is_empty() {
                menu.append(Some("Move selected here"), Some(&detailed("move-to-album", id)));
            }
        } else if folder_id_of(id).is_some() {
            if data.albums.is_empty() {
                menu.append(Some("New Album…"), Some(&detailed("new-album", id)));
            } else {
                let submenu = self.build_move_submenu(&data, 0);
                menu.append_submenu(Some("Move to Album"), &submenu);
            }
            menu.append(Some("Remove from Album"), Some(&detailed("remove-folder", id)));
        } else if id == NEW_FOLDERS_ID {
            menu.append(Some("New Album…"), Some(&detailed("new-album", id)));
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
            let name = data.albums.get(&aid).map(|a| a.name.clone()).unwrap_or_default();
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
