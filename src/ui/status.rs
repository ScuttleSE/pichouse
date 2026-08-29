//! Bottom status bar: message, progress bar, and a Stop button.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Button, Label, Orientation, ProgressBar};

use super::state::AppState;

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
