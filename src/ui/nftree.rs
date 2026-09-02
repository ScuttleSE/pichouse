//! The "New folders" tree: how unassigned folders group in the sidebar.
//!
//! After a root's first scan, newly discovered folders stay unassigned and
//! appear under the "New folders" section. This module decides the shape of
//! that section. It is pure (no GTK, no DB) so it is unit-testable.
//!
//! Rules:
//! - An unassigned folder whose parent directory is itself an unassigned
//!   folder row nests under that folder node.
//! - Otherwise its chain of ancestor directories that have no folder row and
//!   are not "stopped" become synthetic `nfdir:<abs path>` container nodes.
//!   A synthetic node has no folder row; it exists only in the view.
//! - The chain stops at any ancestor of an assigned (filed) folder, any
//!   library root, or the filesystem root. Everything below the stop point
//!   counts as new, so a folder whose parent is already filed shows
//!   standalone.
//! - Collapse pass: a synthetic container left with exactly one child is
//!   removed and its child promoted, up the whole tree. A single new folder
//!   therefore shows standalone; a directory with several new subfolders
//!   stays as a group.

/// Sidebar node id prefix for synthetic directory containers.
pub const NFDIR_PREFIX: &str = "nfdir:";

/// A node in the New Folders tree. `id` is the sidebar node id (`folder:<fid>`
/// for real folder rows, `nfdir:<path>` for synthetic directories).
#[derive(Debug, Clone, PartialEq)]
pub struct NfNode {
    pub id: String,
    /// Absolute disk path this node represents.
    pub path: String,
    pub children: Vec<NfNode>,
}

impl NfNode {
    /// Depth-first search for a node by sidebar id.
    pub fn find<'a>(roots: &'a [NfNode], id: &str) -> Option<&'a NfNode> {
        for n in roots {
            if n.id == id {
                return Some(n);
            }
            if let Some(found) = NfNode::find(&n.children, id) {
                return Some(found);
            }
        }
        None
    }
}

/// Build the New Folders tree.
///
/// `unassigned` holds `(folder_id, path)` for every folder with no album
/// membership. `assigned_paths` holds the paths of filed folders. `roots` are
/// the library root paths. Returns the top-level nodes, sorted by name.
pub fn build(
    unassigned: &[(i64, String)],
    assigned_paths: &[String],
    roots: &[String],
) -> Vec<NfNode> {
    let ua: std::collections::HashMap<String, i64> = unassigned
        .iter()
        .map(|(id, p)| (normalize(p), *id))
        .collect();

    // Stop set: every ancestor directory of every assigned folder. A path in
    // this set is already represented in the Library tree.
    let mut stop: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in assigned_paths {
        stop.extend(ancestors(p));
    }
    let root_set: std::collections::HashSet<String> =
        roots.iter().map(|r| normalize(r)).collect();

    // id -> node, and id -> parent id (None = top level).
    let mut nodes: std::collections::HashMap<String, NfNode> =
        std::collections::HashMap::new();
    let mut parent_of: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();

    for (fid, path) in unassigned {
        let p = normalize(path);
        let folder_id = format!("folder:{fid}");

        // Walk ancestors bottom-up, collecting the synthetic chain until a
        // stop condition: an unassigned-folder parent, a stop path, a library
        // root, or the filesystem root.
        let mut chain: Vec<String> = Vec::new();
        let mut attach: Option<String> = None;
        let mut cur = p.clone();
        while let Some(parent) = parent_of_path(&cur) {
            if let Some(&pfid) = ua.get(&parent) {
                attach = Some(format!("folder:{pfid}"));
                break;
            }
            if stop.contains(&parent) || root_set.contains(&parent) {
                break;
            }
            chain.push(parent.clone());
            cur = parent;
        }
        chain.reverse(); // top-down: nearest-stop ancestor ... immediate parent

        // Ensure the synthetic chain exists, chained from the attach point.
        let mut parent_id = attach;
        for dir in chain {
            let id = format!("{NFDIR_PREFIX}{dir}");
            nodes.entry(id.clone()).or_insert_with(|| NfNode {
                id: id.clone(),
                path: dir,
                children: Vec::new(),
            });
            // A synthetic dir's parent is path-determined, so the first insert
            // already holds the right parent.
            parent_of.entry(id.clone()).or_insert(parent_id.clone());
            parent_id = Some(id);
        }

        nodes
            .entry(folder_id.clone())
            .or_insert_with(|| NfNode {
                id: folder_id.clone(),
                path: p,
                children: Vec::new(),
            });
        parent_of.insert(folder_id, parent_id);
    }

    // Assemble the forest.
    let mut by_parent: std::collections::HashMap<Option<String>, Vec<String>> =
        std::collections::HashMap::new();
    for (id, par) in &parent_of {
        by_parent.entry(par.clone()).or_default().push(id.clone());
    }

    fn assemble(
        id: &str,
        nodes: &std::collections::HashMap<String, NfNode>,
        by_parent: &std::collections::HashMap<Option<String>, Vec<String>>,
    ) -> NfNode {
        let mut node = nodes[id].clone();
        let kids = by_parent.get(&Some(id.to_string())).cloned().unwrap_or_default();
        for kid in kids {
            node.children.push(assemble(&kid, nodes, by_parent));
        }
        node
    }

    let top = by_parent.get(&None).cloned().unwrap_or_default();
    let mut out: Vec<NfNode> = top.iter().map(|id| assemble(id, &nodes, &by_parent)).collect();
    sort_recursive(&mut out);
    collapse_synthetics(&mut out);
    out
}

/// Trim trailing separators (except a bare "/") so path comparisons are exact.
fn normalize(p: &str) -> String {
    let t = p.trim_end_matches(std::path::MAIN_SEPARATOR);
    if t.is_empty() && p.starts_with(std::path::MAIN_SEPARATOR) {
        std::path::MAIN_SEPARATOR.to_string()
    } else {
        t.to_string()
    }
}

/// The parent path of `p`, or `None` at the filesystem root.
fn parent_of_path(p: &str) -> Option<String> {
    let parent = std::path::Path::new(p).parent()?;
    let s = parent.to_string_lossy();
    if s.is_empty() {
        None
    } else {
        Some(normalize(&s))
    }
}

/// All ancestors of `p`, nearest first, up to (excluding) the filesystem root.
fn ancestors(p: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = normalize(p);
    while let Some(parent) = parent_of_path(&cur) {
        out.push(parent.clone());
        cur = parent;
    }
    out
}

/// Sort siblings by name (case-insensitive), recursively.
fn sort_recursive(nodes: &mut Vec<NfNode>) {
    for n in nodes.iter_mut() {
        sort_recursive(&mut n.children);
    }
    nodes.sort_by(|a, b| {
        let na = basename_lower(&a.path);
        let nb = basename_lower(&b.path);
        na.cmp(&nb).then_with(|| a.path.cmp(&b.path))
    });
}

fn basename_lower(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Remove synthetic containers left with exactly one child, promoting that
/// child, applied to every level. One pass per level suffices: a promoted
/// child was itself already collapsed when its own parent list was processed.
fn collapse_synthetics(nodes: &mut Vec<NfNode>) {
    for n in nodes.iter_mut() {
        collapse_synthetics(&mut n.children);
    }
    let mut out: Vec<NfNode> = Vec::with_capacity(nodes.len());
    for n in nodes.drain(..) {
        if n.id.starts_with(NFDIR_PREFIX) && n.children.len() == 1 {
            out.push(n.children.into_iter().next().unwrap());
        } else {
            out.push(n);
        }
    }
    *nodes = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fids(nodes: &[NfNode]) -> Vec<i64> {
        nodes
            .iter()
            .filter_map(|n| n.id.strip_prefix("folder:").and_then(|s| s.parse().ok()))
            .collect()
    }

    const ROOT: &str = "/pictures";

    fn s(p: &str) -> String {
        p.to_string()
    }

    #[test]
    fn single_folder_shows_standalone() {
        let tree = build(
            &[(1, format!("{ROOT}/single_folder"))],
            &[format!("{ROOT}/beach")],
            &[s(ROOT)],
        );
        assert_eq!(fids(&tree), vec![1]);
    }

    #[test]
    fn folder_with_subfolders_groups_under_synthetic_dir() {
        // Scenario 2: vacation (no folder row, no images) with italy + greece.
        let tree = build(
            &[
                (10, format!("{ROOT}/vacation/italy")),
                (11, format!("{ROOT}/vacation/greece")),
            ],
            &[format!("{ROOT}/beach")],
            &[s(ROOT)],
        );
        assert_eq!(tree.len(), 1, "vacation is the only top entry");
        assert_eq!(tree[0].id, format!("{NFDIR_PREFIX}{ROOT}/vacation"));
        // Children sort by name: greece before italy.
        assert_eq!(fids(&tree[0].children), vec![11, 10]);
    }

    #[test]
    fn parent_dir_already_filed_shows_standalone() {
        // Scenario 3: vacation was filed (italy assigned); spain is new.
        let tree = build(
            &[(12, format!("{ROOT}/vacation/spain"))],
            &[format!("{ROOT}/vacation/italy")],
            &[s(ROOT)],
        );
        assert_eq!(fids(&tree), vec![12]);
        assert!(tree[0].id.starts_with("folder:"));
    }

    #[test]
    fn deep_new_folder_with_subfolders_nests() {
        // Scenario 4: italy filed; winter_vacations (no folder row) is new
        // with three subfolders. It stays as a synthetic group.
        let tree = build(
            &[
                (20, format!("{ROOT}/vacation/italy/winter_vacations/alps")),
                (21, format!("{ROOT}/vacation/italy/winter_vacations/lakes")),
                (22, format!("{ROOT}/vacation/italy/winter_vacations/huts")),
            ],
            &[format!("{ROOT}/vacation/italy")],
            &[s(ROOT)],
        );
        assert_eq!(tree.len(), 1);
        assert_eq!(
            tree[0].id,
            format!("{NFDIR_PREFIX}{ROOT}/vacation/italy/winter_vacations")
        );
        assert_eq!(tree[0].children.len(), 3);
    }

    #[test]
    fn new_folder_row_nests_under_unassigned_parent_folder() {
        // winter_vacations HAS a folder row and its parent italy is also
        // unassigned: it nests under the italy folder node.
        let tree = build(
            &[
                (30, format!("{ROOT}/vacation/italy")),
                (31, format!("{ROOT}/vacation/italy/winter")),
            ],
            &[],
            &[s(ROOT)],
        );
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, "folder:30");
        assert_eq!(fids(&tree[0].children), vec![31]);
    }

    #[test]
    fn synthetic_chain_collapses_single_children() {
        // a/b/c with only c unassigned: synthetic a and a/b collapse away.
        let tree = build(
            &[(40, format!("{ROOT}/a/b/c"))],
            &[format!("{ROOT}/other")],
            &[s(ROOT)],
        );
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, format!("{ROOT}/a/b/c"));
        assert!(tree[0].id.starts_with("folder:"));
    }

    #[test]
    fn nothing_assigned_uses_roots_as_stop() {
        // Only y unassigned under root/x/y: synthetic x collapses.
        let tree = build(&[(50, format!("{ROOT}/x/y"))], &[], &[s(ROOT)]);
        assert_eq!(fids(&tree), vec![50]);
    }

    #[test]
    fn mixed_multi_level_tree_keeps_multi_child_synthetics() {
        let tree = build(
            &[
                (60, format!("{ROOT}/trip/italy")),
                (61, format!("{ROOT}/trip/greece")),
                (62, format!("{ROOT}/trip/italy/rome")),
                (63, format!("{ROOT}/solo")),
            ],
            &[format!("{ROOT}/old")],
            &[s(ROOT)],
        );
        assert_eq!(tree.len(), 2, "trip and solo at top level");
        let trip = tree
            .iter()
            .find(|n| n.id.starts_with(NFDIR_PREFIX))
            .unwrap();
        assert_eq!(trip.path, format!("{ROOT}/trip"));
        assert_eq!(fids(&trip.children), vec![61, 60]);
        let italy = trip
            .children
            .iter()
            .find(|n| n.id == "folder:60")
            .unwrap();
        assert_eq!(fids(&italy.children), vec![62]);
        let solo = tree.iter().find(|n| n.id == "folder:63").unwrap();
        assert!(solo.children.is_empty());
    }

    #[test]
    fn find_locates_nested_node() {
        // Two folders under trip so the synthetic dir survives the collapse.
        let tree = build(
            &[
                (70, format!("{ROOT}/trip/italy")),
                (71, format!("{ROOT}/trip/greece")),
            ],
            &[format!("{ROOT}/other")],
            &[s(ROOT)],
        );
        let id = format!("{NFDIR_PREFIX}{ROOT}/trip");
        assert_eq!(
            NfNode::find(&tree, &id).unwrap().path,
            format!("{ROOT}/trip")
        );
        assert!(NfNode::find(&tree, "folder:70").is_some());
        assert!(NfNode::find(&tree, "folder:999").is_none());
        // A collapsed synthetic id is not findable.
        let single = build(
            &[(72, format!("{ROOT}/a/b/c"))],
            &[format!("{ROOT}/other")],
            &[s(ROOT)],
        );
        assert!(NfNode::find(&single, &format!("{NFDIR_PREFIX}{ROOT}/a")).is_none());
        assert!(NfNode::find(&single, "folder:72").is_some());
    }
}
