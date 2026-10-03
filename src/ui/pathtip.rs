//! Grid tooltip that shows more of the path while the user holds Shift.
//!
//! Without Shift, the tooltip shows the filename. When the user presses Shift,
//! the tooltip adds the parent folder immediately. After each step interval,
//! it adds one more folder. It stops at the library root folder.
//!
//! A poll timer runs only while the pointer is on a cell. Each tick reads the
//! Shift state and calls `trigger_tooltip_query` when the depth changes.

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::Overlay;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// The poll interval of the Shift state while the pointer is on a cell.
const POLL_MS: u64 = 100;
/// The default step interval in milliseconds.
pub const DEFAULT_STEP_MS: u32 = 1000;

thread_local! {
    static STEP_MS: Cell<u32> = const { Cell::new(DEFAULT_STEP_MS) };
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static SHIFT_START: Cell<Option<Instant>> = const { Cell::new(None) };
    static TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

/// Set the step interval (milliseconds).
pub fn set_step_ms(ms: u32) {
    STEP_MS.with(|s| s.set(ms.max(100)));
}

/// Read the step interval (milliseconds).
pub fn step_ms() -> u32 {
    STEP_MS.with(|s| s.get())
}

fn shift_held(w: &impl IsA<gtk4::Widget>) -> bool {
    w.display()
        .default_seat()
        .and_then(|s| s.keyboard())
        .map(|k| k.modifier_state().contains(gdk::ModifierType::SHIFT_MASK))
        .unwrap_or(false)
}

fn stop_timer() {
    TIMER.with(|t| {
        if let Some(id) = t.borrow_mut().take() {
            id.remove();
        }
    });
}

/// Compute the current depth from the Shift state. Returns true on a change.
fn update_depth(w: &impl IsA<gtk4::Widget>) -> bool {
    let new = if shift_held(w) {
        let start = SHIFT_START.with(|s| match s.get() {
            Some(t) => t,
            None => {
                let now = Instant::now();
                s.set(Some(now));
                now
            }
        });
        let step = step_ms().max(100) as u128;
        1 + (start.elapsed().as_millis() / step) as usize
    } else {
        SHIFT_START.with(|s| s.set(None));
        0
    };
    DEPTH.with(|d| {
        let changed = d.get() != new;
        d.set(new);
        changed
    })
}

/// Build the tooltip text: the filename plus `depth` parent folders. The
/// parents stop at the library root folder (the root name is included).
pub fn path_text(path: &str, roots: &[String], depth: usize) -> String {
    let p = std::path::Path::new(path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    if depth == 0 {
        return name;
    }
    // The longest root that contains the file.
    let root = roots
        .iter()
        .filter(|r| p.starts_with(r.as_str()))
        .max_by_key(|r| r.len());
    let mut parts: Vec<String> = Vec::new();
    let mut cur = p.parent();
    while let Some(dir) = cur {
        if parts.len() >= depth {
            break;
        }
        let Some(n) = dir.file_name() else { break };
        parts.push(n.to_string_lossy().into_owned());
        if root.map(|r| dir == std::path::Path::new(r)).unwrap_or(false) {
            break;
        }
        cur = dir.parent();
    }
    parts.reverse();
    parts.push(name);
    parts.join("/")
}

/// Wire the tooltip on one grid cell overlay. Call one time in cell setup.
/// `roots` returns the library root folder paths.
pub fn attach(overlay: &Overlay, roots: impl Fn() -> Vec<String> + 'static) {
    overlay.set_has_tooltip(true);
    overlay.connect_query_tooltip(move |w, _, _, _, tip| {
        let path: String =
            unsafe { w.data::<String>("photo-path").map(|p| p.as_ref().clone()) }
                .unwrap_or_default();
        if path.is_empty() {
            return false;
        }
        let missing: bool =
            unsafe { w.data::<bool>("photo-missing").map(|p| *p.as_ref()) }.unwrap_or(false);
        let depth = DEPTH.with(|d| d.get());
        let text = if depth == 0 {
            path_text(&path, &[], 0)
        } else {
            path_text(&path, &roots(), depth)
        };
        let text = if missing {
            if depth == 0 {
                "File missing from disk".to_string()
            } else {
                format!("File missing from disk \u{2014} {text}")
            }
        } else {
            text
        };
        tip.set_text(Some(&text));
        true
    });

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter(|ctrl, _, _| {
        let w = ctrl.widget();
        stop_timer();
        DEPTH.with(|d| d.set(0));
        SHIFT_START.with(|s| s.set(None));
        update_depth(&w);
        let weak = w.downgrade();
        let id = glib::timeout_add_local(Duration::from_millis(POLL_MS), move || {
            let Some(w) = weak.upgrade() else {
                TIMER.with(|t| t.borrow_mut().take());
                return glib::ControlFlow::Break;
            };
            if update_depth(&w) {
                w.trigger_tooltip_query();
            }
            glib::ControlFlow::Continue
        });
        TIMER.with(|t| *t.borrow_mut() = Some(id));
    });
    motion.connect_leave(|_| {
        stop_timer();
        DEPTH.with(|d| d.set(0));
        SHIFT_START.with(|s| s.set(None));
    });
    overlay.add_controller(motion);
}

/// Store the bound photo on the overlay. Call on each bind.
pub fn bind(overlay: &Overlay, path: String, missing: bool) {
    unsafe {
        overlay.set_data("photo-path", path);
        overlay.set_data("photo-missing", missing);
    }
}

#[cfg(test)]
mod tests {
    use super::path_text;

    #[test]
    fn stops_at_root() {
        let roots = vec!["/home/u/Photos".to_string()];
        let p = "/home/u/Photos/2024/Trip/a.jpg";
        assert_eq!(path_text(p, &roots, 0), "a.jpg");
        assert_eq!(path_text(p, &roots, 1), "Trip/a.jpg");
        assert_eq!(path_text(p, &roots, 2), "2024/Trip/a.jpg");
        assert_eq!(path_text(p, &roots, 3), "Photos/2024/Trip/a.jpg");
        assert_eq!(path_text(p, &roots, 9), "Photos/2024/Trip/a.jpg");
    }
}
