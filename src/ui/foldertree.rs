//! Left sidebar Folders tab: a raw filesystem tree of the library roots.
//!
//! Nodes are directory paths; children are read live from disk on demand.
//! Selecting a directory shows its images in the grid.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Label, ListItem, ListView, Orientation, ScrolledWindow, SignalListItemFactory,
    SingleSelection, StringList, StringObject, TreeExpander, TreeListModel, TreeListRow,
};

use super::state::AppState;

/// The raw filesystem folder tree.
pub struct FolderTree {
    root: GtkBox,
    list_root: StringList,
    state: Rc<AppState>,
}

impl FolderTree {
    /// Build the folder tree.
    pub fn new(state: Rc<AppState>) -> Rc<FolderTree> {
        let list_root = StringList::new(&[]);

        let tree_model = TreeListModel::new(list_root.clone(), false, false, move |item| {
            let so = item.downcast_ref::<StringObject>()?;
            let dir = so.string().to_string();
            let children = subdirs(&dir);
            if children.is_empty() {
                return None;
            }
            let refs: Vec<&str> = children.iter().map(|s| s.as_str()).collect();
            Some(StringList::new(&refs).upcast())
        });

        let selection = SingleSelection::new(Some(tree_model.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
            let expander = TreeExpander::new();
            let label = Label::new(None);
            label.set_xalign(0.0);
            expander.set_child(Some(&label));
            item.set_child(Some(&expander));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
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
            if let Some(label) = expander.child().and_downcast::<Label>() {
                label.set_text(&base_name(&so.string()));
            }
        });

        let list_view = ListView::new(Some(selection.clone()), Some(factory));

        let state_for_sel = state.clone();
        selection.connect_selection_changed(move |sel, _, _| {
            if let Some(row) = sel.selected_item().and_downcast::<TreeListRow>() {
                if let Some(so) = row.item().and_downcast::<StringObject>() {
                    super::app::load_raw_folder_into_grid(&state_for_sel, &so.string());
                }
            }
        });

        let scroll = ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_child(Some(&list_view));

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.append(&scroll);

        Rc::new(FolderTree {
            root,
            list_root,
            state,
        })
    }

    /// The folder-tree root widget.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Refresh the set of root folders from the library.
    pub fn reload(&self) {
        let lfs = self.state.lib.library_folders().unwrap_or_default();
        let paths: Vec<String> = lfs.into_iter().map(|lf| lf.path).collect();
        let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        let n = self.list_root.n_items();
        self.list_root.splice(0, n, &refs);
    }
}

/// The immediate subdirectory paths of `dir`, sorted, skipping hidden dirs.
fn subdirs(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with('.') {
                    out.push(entry.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    out.sort();
    out
}

/// The last path element, falling back to the full path for roots like "/".
fn base_name(p: &str) -> String {
    let base = std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if base.is_empty() {
        p.to_string()
    } else {
        base
    }
}
