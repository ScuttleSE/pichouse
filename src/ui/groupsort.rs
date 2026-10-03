//! The sort order of the "Unidentified" tiles in the Faces and Characters
//! views. The sort changes only the display order. It does not change a group.

use std::collections::HashMap;

use crate::model::UnnamedGroupInfo;

/// One sort order for the unnamed groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSort {
    MostImages,
    FewestImages,
    Newest,
    Oldest,
    Similar,
}

impl GroupSort {
    /// All the sort orders, in menu order.
    pub const ALL: [GroupSort; 5] = [
        GroupSort::MostImages,
        GroupSort::FewestImages,
        GroupSort::Newest,
        GroupSort::Oldest,
        GroupSort::Similar,
    ];

    /// The text in the sort menu.
    pub fn label(self) -> &'static str {
        match self {
            GroupSort::MostImages => "Most images",
            GroupSort::FewestImages => "Fewest images",
            GroupSort::Newest => "Newest group",
            GroupSort::Oldest => "Oldest group",
            GroupSort::Similar => "Similar together",
        }
    }

    /// The value in the settings table.
    pub fn key(self) -> &'static str {
        match self {
            GroupSort::MostImages => "most",
            GroupSort::FewestImages => "fewest",
            GroupSort::Newest => "newest",
            GroupSort::Oldest => "oldest",
            GroupSort::Similar => "similar",
        }
    }

    /// Read a settings value. An unknown value gives `MostImages`.
    pub fn from_key(s: &str) -> GroupSort {
        GroupSort::ALL
            .into_iter()
            .find(|g| g.key() == s)
            .unwrap_or(GroupSort::MostImages)
    }
}

/// Sort the unnamed groups `(cluster_id, count)` with `info`. The noise
/// group (-1) always stays last. A group with no entry in `info` uses a time
/// of 0 and an empty centroid.
pub fn sort_groups(
    groups: Vec<(i64, i64)>,
    info: &[UnnamedGroupInfo],
    sort: GroupSort,
) -> Vec<(i64, i64)> {
    let by_id: HashMap<i64, &UnnamedGroupInfo> = info.iter().map(|g| (g.cluster_id, g)).collect();
    let newest = |id: i64| by_id.get(&id).map(|g| g.newest).unwrap_or(0);
    let (mut v, noise): (Vec<_>, Vec<_>) = groups.into_iter().partition(|(c, _)| *c != -1);
    // Each sort uses the cluster id as the last key, so the order is stable.
    match sort {
        GroupSort::MostImages => v.sort_by_key(|&(c, n)| (-n, c)),
        GroupSort::FewestImages => v.sort_by_key(|&(c, n)| (n, c)),
        GroupSort::Newest => v.sort_by_key(|&(c, _)| (-newest(c), c)),
        GroupSort::Oldest => v.sort_by_key(|&(c, _)| (newest(c), c)),
        GroupSort::Similar => {
            v.sort_by_key(|&(c, n)| (-n, c));
            let cents: Vec<Vec<f32>> = v
                .iter()
                .map(|(c, _)| by_id.get(c).map(|g| g.centroid.clone()).unwrap_or_default())
                .collect();
            let order = order_by_similarity(&cents);
            v = order.into_iter().map(|i| v[i]).collect();
        }
    }
    v.extend(noise);
    v
}

/// Order the items so that similar centroids are next to each other. The
/// chain starts at index 0. Each next item is the unused item nearest to the
/// current item (cosine similarity). An item with an empty centroid goes to
/// the end, in its original order. The result is a permutation of the indices.
pub fn order_by_similarity(centroids: &[Vec<f32>]) -> Vec<usize> {
    let unit: Vec<Option<Vec<f32>>> = centroids
        .iter()
        .map(|c| {
            let n = c.iter().map(|x| x * x).sum::<f32>().sqrt();
            (n > 0.0).then(|| c.iter().map(|x| x / n).collect())
        })
        .collect();
    let mut left: Vec<usize> = (0..unit.len()).filter(|&i| unit[i].is_some()).collect();
    let mut out = Vec::with_capacity(unit.len());
    if !left.is_empty() {
        let mut cur = left.remove(0);
        out.push(cur);
        while !left.is_empty() {
            let a = unit[cur].as_ref().unwrap();
            let mut best = 0;
            let mut best_sim = f32::NEG_INFINITY;
            for (k, &j) in left.iter().enumerate() {
                let b = unit[j].as_ref().unwrap();
                let sim = if a.len() == b.len() {
                    a.iter().zip(b).map(|(x, y)| x * y).sum()
                } else {
                    -2.0
                };
                if sim > best_sim {
                    best_sim = sim;
                    best = k;
                }
            }
            cur = left.remove(best);
            out.push(cur);
        }
    }
    out.extend((0..unit.len()).filter(|&i| unit[i].is_none()));
    out
}

/// The "Unidentified" section header: a horizontal line, a title with the
/// group count, and a sort menu. `on_change` runs when the user picks a new
/// sort. The caller saves the sort and reloads the view.
pub struct UnnamedHeader {
    pub root: gtk4::Box,
    pub title: gtk4::Label,
    dd: gtk4::DropDown,
    /// True while `set_sort` changes the menu. Then `on_change` does not run.
    quiet: std::rc::Rc<std::cell::Cell<bool>>,
}

impl UnnamedHeader {
    pub fn new(initial: GroupSort, on_change: impl Fn(GroupSort) + 'static) -> UnnamedHeader {
        use gtk4::prelude::*;
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.set_margin_top(8);
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let title = gtk4::Label::new(Some("Unidentified"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("heading");
        row.append(&title);
        row.append(&gtk4::Label::new(Some("Sort:")));
        let labels: Vec<&str> = GroupSort::ALL.iter().map(|s| s.label()).collect();
        let dd = gtk4::DropDown::from_strings(&labels);
        dd.set_tooltip_text(Some("Sort the unidentified groups"));
        let idx = GroupSort::ALL.iter().position(|s| *s == initial).unwrap_or(0);
        dd.set_selected(idx as u32);
        let quiet = std::rc::Rc::new(std::cell::Cell::new(false));
        let q = quiet.clone();
        dd.connect_selected_notify(move |d| {
            if q.get() {
                return;
            }
            if let Some(s) = GroupSort::ALL.get(d.selected() as usize) {
                on_change(*s);
            }
        });
        row.append(&dd);
        root.append(&row);
        UnnamedHeader { root, title, dd, quiet }
    }

    /// Show `sort` in the menu. This does not run `on_change`.
    pub fn set_sort(&self, sort: GroupSort) {
        let idx = GroupSort::ALL.iter().position(|s| *s == sort).unwrap_or(0);
        self.quiet.set(true);
        self.dd.set_selected(idx as u32);
        self.quiet.set(false);
    }

    /// Set the group count in the title. Hide the header when `n` is 0.
    pub fn set_count(&self, n: usize) {
        use gtk4::prelude::*;
        self.title.set_text(&format!("Unidentified ({n})"));
        self.root.set_visible(n > 0);
    }
}

/// Build a tile FlowBox with the settings that both face views use.
pub fn tile_flow() -> gtk4::FlowBox {
    use gtk4::prelude::*;
    let flow = gtk4::FlowBox::new();
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_max_children_per_line(8);
    flow.set_min_children_per_line(2);
    flow.set_row_spacing(8);
    flow.set_column_spacing(8);
    flow.set_margin_top(8);
    flow.set_margin_bottom(8);
    flow.set_margin_start(8);
    flow.set_margin_end(8);
    flow.set_valign(gtk4::Align::Start);
    flow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: i64, newest: i64, c: Vec<f32>) -> UnnamedGroupInfo {
        UnnamedGroupInfo { cluster_id: id, newest, centroid: c }
    }

    #[test]
    fn similar_groups_sit_together() {
        // A and C are similar. B is different.
        let c = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.9, 0.1]];
        assert_eq!(order_by_similarity(&c), vec![0, 2, 1]);
    }

    #[test]
    fn empty_centroid_goes_last() {
        let c = vec![vec![], vec![1.0, 0.0], vec![0.0, 1.0]];
        assert_eq!(order_by_similarity(&c), vec![1, 2, 0]);
    }

    #[test]
    fn sorts_and_keeps_noise_last() {
        let groups = vec![(-1, 9), (1, 2), (2, 5), (3, 3)];
        let inf = vec![
            info(1, 30, vec![1.0, 0.0]),
            info(2, 10, vec![0.0, 1.0]),
            info(3, 20, vec![0.1, 0.9]),
        ];
        let ids = |s| {
            sort_groups(groups.clone(), &inf, s)
                .into_iter()
                .map(|(c, _)| c)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(GroupSort::MostImages), vec![2, 3, 1, -1]);
        assert_eq!(ids(GroupSort::FewestImages), vec![1, 3, 2, -1]);
        assert_eq!(ids(GroupSort::Newest), vec![1, 3, 2, -1]);
        assert_eq!(ids(GroupSort::Oldest), vec![2, 3, 1, -1]);
        assert_eq!(ids(GroupSort::Similar), vec![2, 3, 1, -1]);
    }

    #[test]
    fn key_round_trip() {
        for s in GroupSort::ALL {
            assert_eq!(GroupSort::from_key(s.key()), s);
        }
        assert_eq!(GroupSort::from_key("x"), GroupSort::MostImages);
    }
}
