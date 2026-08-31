//! Rules editor for a virtual album.
//!
//! Lets the user pick an AND/OR match mode and a list of conditions
//! (tag / date-from / date-to / filename / folder). Saving replaces the album's
//! stored rules and its match mode.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Entry, Label, Orientation, ScrolledWindow, StringList, Window,
};

use crate::model::{RuleField, RuleMatch, RuleOp, VirtualRule};

use super::state::{show_error, AppState};

/// Field choices, in dropdown order.
const FIELDS: &[(RuleField, &str)] = &[
    (RuleField::Tag, "Has tag"),
    (RuleField::DateFrom, "Date on or after (YYYY-MM-DD)"),
    (RuleField::DateTo, "Date on or before (YYYY-MM-DD)"),
    (RuleField::Filename, "Filename contains"),
    (RuleField::Folder, "Folder id is"),
    (RuleField::Person, "Contains person"),
];

/// One editable rule row: a field dropdown and a value entry.
struct RuleRow {
    container: GtkBox,
    field: DropDown,
    value: Entry,
}

/// Open the rules editor for a virtual album. `on_saved` runs after a
/// successful save so the caller can refresh the view.
pub fn open_rules_editor<F: Fn() + 'static>(
    state: &Rc<AppState>,
    album_id: i64,
    name: &str,
    on_saved: F,
) {
    let existing = state.lib.virtual_album_rules(album_id).unwrap_or_default();
    let match_mode = state
        .lib
        .virtual_albums()
        .unwrap_or_default()
        .into_iter()
        .find(|a| a.id == album_id)
        .map(|a| a.rule_match)
        .unwrap_or(RuleMatch::Or);

    let heading = Label::new(Some(&format!("Rules for \"{name}\"")));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");

    // Match mode: Any (OR) / All (AND).
    let match_list = StringList::new(&["Match ANY rule (OR)", "Match ALL rules (AND)"]);
    let match_drop = DropDown::new(Some(match_list), gtk4::Expression::NONE);
    match_drop.set_selected(match match_mode {
        RuleMatch::Or => 0,
        RuleMatch::And => 1,
    });

    let rows: Rc<RefCell<Vec<Rc<RuleRow>>>> = Rc::new(RefCell::new(Vec::new()));

    let rows_box = GtkBox::new(Orientation::Vertical, 6);
    rows_box.set_hexpand(true);

    let add_row = {
        let rows = rows.clone();
        let rows_box = rows_box.clone();
        Rc::new(move |rule: Option<&VirtualRule>| {
            let row = build_rule_row(rule);
            rows_box.append(&row.container);
            let row = Rc::new(row);
            // Wire the remove button now that the row is in an Rc.
            if let Some(remove) = row.container.last_child().and_downcast::<Button>() {
                let rows = rows.clone();
                let rows_box = rows_box.clone();
                let row_weak = Rc::downgrade(&row);
                remove.connect_clicked(move |_| {
                    if let Some(row) = row_weak.upgrade() {
                        rows_box.remove(&row.container);
                        rows.borrow_mut().retain(|r| !Rc::ptr_eq(r, &row));
                    }
                });
            }
            rows.borrow_mut().push(row);
        })
    };

    if existing.is_empty() {
        add_row(None);
    } else {
        for r in &existing {
            add_row(Some(r));
        }
    }

    let add_btn = Button::with_label("Add rule");
    add_btn.set_halign(gtk4::Align::Start);
    {
        let add_row = add_row.clone();
        add_btn.connect_clicked(move |_| add_row(None));
    }

    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_min_content_height(200);
    scroll.set_child(Some(&rows_box));

    let save = Button::with_label("Save");
    save.add_css_class("suggested-action");
    let cancel = Button::with_label("Cancel");

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&cancel);
    buttons.append(&save);

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&heading);
    root.append(&match_drop);
    root.append(&scroll);
    root.append(&add_btn);
    root.append(&buttons);

    let window = Window::builder()
        .title("Edit Rules")
        .modal(true)
        .default_width(460)
        .default_height(420)
        .child(&root)
        .build();
    if let Some(p) = state.window() {
        window.set_transient_for(Some(&p));
    }

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        let state = state.clone();
        let rows = rows.clone();
        let match_drop = match_drop.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let mut collected: Vec<VirtualRule> = Vec::new();
            for row in rows.borrow().iter() {
                if let Some(rule) = collect_rule(album_id, row) {
                    collected.push(rule);
                }
            }
            let match_mode = if match_drop.selected() == 1 {
                RuleMatch::And
            } else {
                RuleMatch::Or
            };
            if let Err(e) = state
                .lib
                .set_virtual_album_rules(album_id, match_mode, &collected)
            {
                show_error(&state, &e.to_string());
                return;
            }
            on_saved();
            window.close();
        });
    }

    window.set_visible(true);
}

/// Build an editable rule row, pre-filled from `rule` when given.
fn build_rule_row(rule: Option<&VirtualRule>) -> RuleRow {
    let labels: Vec<&str> = FIELDS.iter().map(|(_, l)| *l).collect();
    let list = StringList::new(&labels);
    let field = DropDown::new(Some(list), gtk4::Expression::NONE);

    let value = Entry::new();
    value.set_hexpand(true);

    if let Some(rule) = rule {
        let idx = FIELDS
            .iter()
            .position(|(f, _)| *f == rule.field)
            .unwrap_or(0);
        field.set_selected(idx as u32);
        value.set_text(&display_value(rule));
    }

    let remove = Button::from_icon_name("list-remove-symbolic");

    let container = GtkBox::new(Orientation::Horizontal, 6);
    container.append(&field);
    container.append(&value);
    container.append(&remove);

    RuleRow {
        container,
        field,
        value,
    }
}

/// The human-facing value for a rule (dates shown as YYYY-MM-DD).
fn display_value(rule: &VirtualRule) -> String {
    match rule.field {
        RuleField::DateFrom | RuleField::DateTo => {
            let ts: i64 = rule.value.parse().unwrap_or(0);
            unix_to_date(ts)
        }
        _ => rule.value.clone(),
    }
}

/// Read one row into a `VirtualRule`, or `None` when the value is empty/invalid.
fn collect_rule(album_id: i64, row: &RuleRow) -> Option<VirtualRule> {
    let idx = row.field.selected() as usize;
    let (field, _) = FIELDS.get(idx).copied()?;
    let raw = row.value.text().to_string();
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (op, value) = match field {
        RuleField::Tag => (RuleOp::Has, raw.to_string()),
        RuleField::DateFrom => (RuleOp::Gte, date_to_unix(raw)?.to_string()),
        RuleField::DateTo => (RuleOp::Lte, date_to_unix(raw)?.to_string()),
        RuleField::Filename => (RuleOp::Contains, raw.to_string()),
        RuleField::Folder => (RuleOp::Eq, raw.parse::<i64>().ok()?.to_string()),
        RuleField::Person => (RuleOp::Has, raw.to_string()),
    };
    Some(VirtualRule {
        id: 0,
        album_id,
        field,
        op,
        value,
    })
}

/// Parse a `YYYY-MM-DD` date into a Unix timestamp (UTC midnight). Returns
/// `None` when the format is wrong.
fn date_to_unix(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// Format a Unix timestamp as `YYYY-MM-DD` (UTC).
fn unix_to_date(ts: i64) -> String {
    let (y, m, d) = civil_from_days(ts.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since the Unix epoch for a civil date. Howard Hinnant's algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date (year, month, day) for a day count since the Unix epoch.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_roundtrip() {
        for s in ["1970-01-01", "2010-05-01", "2024-02-29", "1999-12-31"] {
            let ts = date_to_unix(s).unwrap();
            assert_eq!(unix_to_date(ts), s);
        }
        assert!(date_to_unix("not-a-date").is_none());
        assert!(date_to_unix("2020-13-01").is_none());
    }
}
