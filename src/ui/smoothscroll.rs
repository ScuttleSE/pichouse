//! Smooth momentum scrolling for the mouse wheel.
//!
//! Each wheel step adds speed. The view continues to move after the wheel
//! stops. The speed decreases exponentially until the view stops. Touchpad
//! events go to GTK with no change, because GTK gives them momentum itself.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, EventControllerScroll, EventControllerScrollFlags, PropagationPhase, ScrolledWindow};

/// The smooth-scroll settings.
#[derive(Clone, Copy, Debug)]
pub struct ScrollCfg {
    /// True to use momentum scrolling.
    pub enabled: bool,
    /// The step size multiplier (0.5 to 3.0). 1.0 is the GTK step distance.
    pub step: f64,
    /// The glide time constant in seconds (0.1 to 1.0).
    pub glide: f64,
}

impl Default for ScrollCfg {
    fn default() -> Self {
        ScrollCfg { enabled: true, step: 1.0, glide: 0.35 }
    }
}

thread_local! {
    static CFG: Cell<ScrollCfg> = Cell::new(ScrollCfg::default());
}

/// Gives the current settings.
pub fn cfg() -> ScrollCfg {
    CFG.with(|c| c.get())
}

/// Sets the settings. All attached scrollers use the new values immediately.
pub fn set_cfg(c: ScrollCfg) {
    CFG.with(|x| x.set(c));
}

/// Loads the settings from the library.
pub fn load(lib: &crate::db::Library) {
    use super::prefs;
    let get = |k: &str, d: &str| lib.get_setting(k, d).unwrap_or_else(|_| d.to_string());
    let def = ScrollCfg::default();
    set_cfg(ScrollCfg {
        enabled: get(prefs::KEY_SCROLL_SMOOTH, "1") != "0",
        step: get(prefs::KEY_SCROLL_STEP, "1.0").parse().unwrap_or(def.step).clamp(0.5, 3.0),
        glide: get(prefs::KEY_SCROLL_GLIDE, "0.35").parse().unwrap_or(def.glide).clamp(0.1, 1.0),
    });
}

#[derive(Default)]
struct Motion {
    /// The speed in pixels per second.
    velocity: f64,
    /// The last value this module wrote to the adjustment.
    last_set: f64,
    /// The frame time of the last tick in microseconds.
    last_time: Option<i64>,
    tick: Option<gtk4::TickCallbackId>,
}

/// Adds momentum scrolling to the vertical axis of `scroller`.
pub fn attach(scroller: &ScrolledWindow) {
    let motion: Rc<RefCell<Motion>> = Rc::new(RefCell::new(Motion::default()));
    let ctl = EventControllerScroll::new(
        EventControllerScrollFlags::VERTICAL | EventControllerScrollFlags::DISCRETE,
    );
    ctl.set_propagation_phase(PropagationPhase::Capture);

    let sw = scroller.downgrade();
    ctl.connect_scroll(move |ctl, _dx, dy| {
        let c = cfg();
        if !c.enabled || dy == 0.0 {
            return glib::Propagation::Proceed;
        }
        // Touchpad and other smooth devices keep the GTK behaviour.
        if ctl.unit() != gtk4::gdk::ScrollUnit::Wheel {
            return glib::Propagation::Proceed;
        }
        let Some(sw) = sw.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let adj = sw.vadjustment();
        let page = adj.page_size();
        if adj.upper() - adj.lower() <= page {
            return glib::Propagation::Proceed;
        }
        // GTK uses page^(2/3) as the wheel step distance.
        let dist = page.powf(2.0 / 3.0) * c.step * dy;
        let mut m = motion.borrow_mut();
        // Other code moved the view. Stop the old motion.
        if m.tick.is_some() && (adj.value() - m.last_set).abs() > 1.0 {
            m.velocity = 0.0;
        }
        // A step in the opposite direction stops the motion first.
        if m.velocity * dist < 0.0 {
            m.velocity = 0.0;
        }
        // With v(t) = v0 * exp(-t / tau), the total distance is v0 * tau.
        m.velocity += dist / c.glide;
        if m.tick.is_none() {
            m.last_set = adj.value();
            m.last_time = None;
            let motion2 = motion.clone();
            let id = sw.add_tick_callback(move |w, clock| {
                let sw = w.downcast_ref::<ScrolledWindow>().unwrap();
                let adj = sw.vadjustment();
                let mut m = motion2.borrow_mut();
                let now = clock.frame_time();
                let dt = match m.last_time {
                    Some(t) => ((now - t) as f64 / 1_000_000.0).clamp(0.0, 0.1),
                    None => 1.0 / 60.0,
                };
                m.last_time = Some(now);
                let stop = |m: &mut Motion| {
                    m.velocity = 0.0;
                    m.tick = None;
                    glib::ControlFlow::Break
                };
                if (adj.value() - m.last_set).abs() > 1.0 {
                    return stop(&mut m);
                }
                let tau = cfg().glide;
                // Exact integral of the decay over dt.
                let decay = (-dt / tau).exp();
                let moved = m.velocity * tau * (1.0 - decay);
                m.velocity *= decay;
                let max = adj.upper() - adj.page_size();
                let target = (m.last_set + moved).clamp(adj.lower(), max);
                m.last_set = target;
                adj.set_value(target);
                let at_edge = target <= adj.lower() || target >= max;
                if m.velocity.abs() < 5.0 || at_edge {
                    return stop(&mut m);
                }
                glib::ControlFlow::Continue
            });
            m.tick = Some(id);
        }
        glib::Propagation::Stop
    });
    scroller.add_controller(ctl);
}
