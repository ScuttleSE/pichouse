//! Duplicate finder: background scan and the review/delete flow.
//!
//! The scan runs off the GTK main thread. It backfills any missing perceptual
//! hash, then groups the in-scope photos with `crate::dedup`. Results return to
//! the main thread, which shows the groups in the grid and offers to delete the
//! auto-selected "worse" copies after a confirm dialog.

use std::rc::Rc;
use std::sync::atomic::Ordering;

use gtk4::glib;

use crate::dedup::{self, DupGroup};
use crate::model::Photo;

use super::dialogs::confirm;
use super::state::{show_error, show_message, AppState};

/// A status update posted from the scan thread to the UI thread.
enum Msg {
    Message(String),
    Progress(f64),
    Done(Vec<DupGroup>),
}

/// Which photos the scan covers.
pub enum Scope {
    /// Every photo in a set of folders (already resolved from albums).
    Folders(Vec<i64>, String),
}

/// Start a duplicate scan over `scope` with the given Hamming `threshold`
/// (0..=64). A threshold of `0` finds only exact and visually identical copies.
pub fn find_duplicates(state: &Rc<AppState>, scope: Scope, threshold: u32) {
    if state.dedup_job.running() {
        show_message(state, "Find Duplicates", "A duplicate scan is already running.");
        return;
    }

    let (folder_ids, label) = match scope {
        Scope::Folders(ids, label) => (ids, label),
    };

    let photos = match state.lib.photos_in_folders(&folder_ids) {
        Ok(p) => p,
        Err(e) => {
            show_error(state, &e.to_string());
            return;
        }
    };
    if photos.len() < 2 {
        show_message(state, "Find Duplicates", "Not enough photos to compare.");
        return;
    }

    let cancel = state.dedup_job.begin();
    let status = state.status();
    status.set_scanning(true);
    status.set_message(&format!("Finding duplicates in {label}…"));
    status.set_progress(0.0);

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        let label = label.clone();
        rx.attach(None, move |msg| {
            let status = state.status();
            match msg {
                Msg::Message(m) => status.set_message(&m),
                Msg::Progress(p) => status.set_progress(p),
                Msg::Done(groups) => {
                    status.set_scanning(false);
                    status.set_progress(-1.0);
                    state.dedup_job.finish();
                    present_results(&state, &label, groups);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        // Backfill perceptual hashes that are missing (0) so the near pass has
        // data. This decodes each such image once and stores the result.
        let mut photos = photos;
        let need: Vec<usize> = photos
            .iter()
            .enumerate()
            .filter(|(_, p)| p.phash == 0)
            .map(|(i, _)| i)
            .collect();
        let total = need.len();
        for (done, &i) in need.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(Msg::Done(Vec::new()));
                return;
            }
            let p = &photos[i];
            let ph = crate::phash::dhash_file(std::path::Path::new(&p.path), p.orientation);
            if ph != 0 {
                let _ = lib.set_photo_phash(p.id, ph);
                photos[i].phash = ph;
            }
            if total > 0 && done % 16 == 0 {
                let _ = tx.send(Msg::Progress(done as f64 / total as f64));
                let _ = tx.send(Msg::Message(format!("Hashing {}/{}…", done + 1, total)));
            }
        }

        let _ = tx.send(Msg::Message("Comparing…".into()));
        let groups = dedup::find_duplicates(&photos, threshold, &cancel);
        let _ = tx.send(Msg::Done(groups));
    });
}

/// Show the duplicate groups in the grid with the review UI.
fn present_results(state: &Rc<AppState>, label: &str, groups: Vec<DupGroup>) {
    if groups.is_empty() {
        show_message(state, "Find Duplicates", "No duplicates found.");
        state.status().set_message("No duplicates found");
        return;
    }

    // Build the grid input: within each group the keep copy first, then the
    // candidates. Each entry carries its group id and whether it starts marked
    // (the auto-selected "worse" copies start with the red X).
    let mut entries: Vec<(Photo, i64, bool)> = Vec::new();
    let mut group_id: i64 = 0;
    let mut candidate_count = 0usize;
    for g in &groups {
        group_id += 1;
        for p in &g.photos {
            let mark = p.id != g.keep_id;
            if mark {
                candidate_count += 1;
            }
            entries.push((p.clone(), group_id, mark));
        }
    }

    let title = format!(
        "Duplicates in {label} — {} groups, {} marked",
        groups.len(),
        candidate_count
    );
    let grid = state.grid();
    grid.show_duplicates(&title, &entries);

    // Wire the "Delete marked" button to a confirm-then-delete flow.
    {
        let state = state.clone();
        grid.set_on_dup_delete(move |marked| {
            let reclaim: i64 = marked.iter().map(|p| p.size).sum();
            let detail = format!(
                "Permanently delete {} marked photos from disk?\n\nThis frees about {}.",
                marked.len(),
                human_size(reclaim)
            );
            let state2 = state.clone();
            confirm(&state, None, "Delete marked duplicates?", &detail, move || {
                delete_marked(&state2, &marked)
            });
        });
    }

    state.show_grid();
    state.status().set_message(&title);
}

/// Hard delete the given photos and refresh the grid to the library view.
fn delete_marked(state: &Rc<AppState>, marked: &[Photo]) {
    let mut deleted = 0;
    let mut failed = 0;
    for c in marked {
        match state.lib.delete_photo_hard(c.id, &c.path) {
            Ok(()) => deleted += 1,
            Err(e) => {
                failed += 1;
                log::warn!("delete duplicate {}: {e}", c.path);
            }
        }
    }
    let msg = if failed > 0 {
        format!("Deleted {deleted} duplicates, {failed} failed")
    } else {
        format!("Deleted {deleted} duplicates")
    };
    state.status().set_message(&msg);
    state.grid().exit_dup_mode();
    state.grid().reload_from_source();
}

/// Format a byte count as a short human string.
fn human_size(bytes: i64) -> String {
    let b = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}
