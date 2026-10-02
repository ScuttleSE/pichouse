//! Bottom status bar: message, progress bar, a Stop button, and the background
//! job indicator in the right corner.

use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Button, Label, MenuButton, Orientation, Popover, ProgressBar, Spinner};

use super::controller::Controller;
use super::state::AppState;

/// How often the job indicator reads the job state.
const POLL: Duration = Duration::from_millis(500);

/// A background job that the indicator shows: its display name and a function
/// that returns its `Controller`.
type JobDef = (&'static str, fn(&AppState) -> &Controller);

/// The background jobs that the indicator shows, in display order.
const JOBS: &[JobDef] = &[
    ("Library scan", |s| &s.reconcile_job),
    ("Folder scan", |s| &s.scan),
    ("Thumbnail generation", |s| &s.enrich_job),
    ("AI tagging", |s| &s.ai_job),
    ("Face detection", |s| &s.face_job),
    ("Stylised face detection", |s| &s.style_face_job),
    ("Duplicate scan", |s| &s.dedup_job),
    ("Immich upload", |s| &s.immich_upload),
];

/// Format a duration as "1h 02m 03s", "2m 03s", or "3s".
fn fmt_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Build the job indicator: an animated spinner that shows while a background
/// job runs. A click opens a popover with one row per running job. Each row
/// shows the job status and has a Cancel button.
fn activity_indicator(state: &Rc<AppState>) -> MenuButton {
    let spinner = Spinner::new();
    spinner.set_spinning(true);

    let list = gtk4::Box::new(Orientation::Vertical, 6);
    list.set_margin_top(6);
    list.set_margin_bottom(6);
    list.set_margin_start(6);
    list.set_margin_end(6);

    // One persistent row per job. The poll only changes the text and the
    // visibility, so a click on Cancel never hits a widget that was replaced.
    let mut rows = Vec::new();
    for &(name, ctl) in JOBS {
        let title = Label::new(Some(name));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let detail = Label::new(None);
        detail.set_xalign(0.0);
        detail.add_css_class("dim-label");
        let text = gtk4::Box::new(Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.append(&title);
        text.append(&detail);

        let cancel = Button::with_label("Cancel");
        cancel.set_valign(gtk4::Align::Center);
        {
            let state = state.clone();
            cancel.connect_clicked(move |_| {
                log::info!("job indicator: cancel {name}");
                ctl(&state).stop();
            });
        }

        let row = gtk4::Box::new(Orientation::Horizontal, 12);
        row.append(&text);
        row.append(&cancel);
        row.set_visible(false);
        list.append(&row);
        rows.push((row, detail, cancel, ctl));
    }

    let popover = Popover::new();
    popover.set_child(Some(&list));

    let button = MenuButton::new();
    button.set_child(Some(&spinner));
    button.set_popover(Some(&popover));
    button.add_css_class("flat");
    button.set_tooltip_text(Some("Background jobs"));
    button.set_visible(false);

    {
        let state = state.clone();
        let button = button.clone();
        glib::timeout_add_local(POLL, move || {
            let mut any = false;
            for (row, detail, cancel, ctl) in &rows {
                let ctl = ctl(&state);
                match ctl.elapsed() {
                    Some(elapsed) => {
                        any = true;
                        let stopping = ctl.stopping();
                        let status = if stopping { "Stopping" } else { "Running" };
                        detail.set_text(&format!("{status} for {}", fmt_elapsed(elapsed)));
                        cancel.set_sensitive(!stopping);
                        row.set_visible(true);
                    }
                    None => row.set_visible(false),
                }
            }
            if !any {
                popover.popdown();
            }
            button.set_visible(any);
            glib::ControlFlow::Continue
        });
    }

    button
}

/// The bottom status bar.
pub struct StatusBar {
    root: gtk4::Box,
    message: Label,
    progress: ProgressBar,
    stop: Button,
}

impl StatusBar {
    /// Build the status bar. The Stop button cancels both scan and AI jobs.
    pub fn new(state: &Rc<AppState>) -> Rc<StatusBar> {
        let message = Label::new(Some("Ready"));
        message.set_xalign(0.0);
        message.set_hexpand(true);

        let progress = ProgressBar::new();
        progress.set_size_request(220, -1);
        progress.set_visible(false);

        let stop = Button::with_label("Stop");
        stop.set_tooltip_text(Some("Stop scanning"));
        stop.add_css_class("destructive-action");
        stop.set_visible(false);
        {
            let state = state.clone();
            stop.connect_clicked(move |_| {
                state.scan.stop();
                state.ai_job.stop();
                state.dedup_job.stop();
            });
        }

        let root = gtk4::Box::new(Orientation::Horizontal, 6);
        root.set_margin_top(4);
        root.set_margin_bottom(4);
        root.set_margin_start(6);
        root.set_margin_end(6);
        root.append(&message);
        root.append(&progress);
        root.append(&stop);
        root.append(&activity_indicator(state));

        Rc::new(StatusBar {
            root,
            message,
            progress,
            stop,
        })
    }

    /// The status bar root widget.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Set the status message.
    pub fn set_message(&self, msg: &str) {
        self.message.set_text(msg);
    }

    /// Set the status message, then revert it to "Ready" after `secs` seconds
    /// unless something else (e.g. enrichment progress) has already changed
    /// it in the meantime.
    pub fn set_message_transient(&self, msg: &str, secs: u64) {
        self.set_message(msg);
        let label = self.message.clone();
        let shown = msg.to_string();
        gtk4::glib::source::timeout_add_local_once(
            std::time::Duration::from_secs(secs),
            move || {
                if label.text() == shown {
                    label.set_text("Ready");
                }
            },
        );
    }

    /// Show or hide the Stop button.
    pub fn set_scanning(&self, scanning: bool) {
        self.stop.set_visible(scanning);
    }

    /// Set the progress fraction. A negative value hides the bar.
    pub fn set_progress(&self, v: f64) {
        if v < 0.0 {
            self.progress.set_visible(false);
        } else {
            self.progress.set_visible(true);
            self.progress.set_fraction(v.clamp(0.0, 1.0));
        }
    }
}
