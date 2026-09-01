//! Rules editor for a virtual album.
//!
//! Lets the user pick an AND/OR match mode and a list of conditions
//! (tag / date-from / date-to / filename / path / folder / person / character
//! — person and character pick from a dropdown of known names), plus
//! optional rule groups that combine a subset of conditions with their own
//! AND/OR mode as a single term of the album's top-level match. Groups do not
//! nest. Saving replaces the album's stored rules, groups, and match mode.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Entry, Frame, Label, Orientation, ScrolledWindow, StringList,
    Window,
};

use crate::model::{RuleField, RuleGroup, RuleMatch, RuleOp, VirtualRule};

use super::state::{show_error, AppState};

/// Field choices, in dropdown order.
const FIELDS: &[(RuleField, &str)] = &[
    (RuleField::Tag, "Has tag"),
    (RuleField::DateFrom, "Date on or after (YYYY-MM-DD)"),
    (RuleField::DateTo, "Date on or before (YYYY-MM-DD)"),
    (RuleField::Filename, "Filename contains"),
    (RuleField::Path, "Path contains"),
    (RuleField::Folder, "Folder id is"),
    (RuleField::Person, "Contains person"),
    (RuleField::Character, "Contains character"),
];

/// The value-entry widget for one rule row: free text, or (for Person/
/// Character) a dropdown over the known names.
enum ValueInput {
    Text(Entry),
    Picker { drop: DropDown, choices: Vec<String> },
}

/// One editable rule row: a field dropdown and a value input that swaps
/// between free text and a name picker depending on the selected field.
struct RuleRow {
    container: GtkBox,
    field: DropDown,
    value_holder: GtkBox,
    value: RefCell<ValueInput>,
}

/// One editable rule group: its own AND/OR mode plus a list of rule rows,
/// combining as a single term of the album's top-level match. Groups do not
/// nest.
struct GroupBox {
    container: Frame,
    match_drop: DropDown,
    remove_btn: Button,
    rows: Rc<RefCell<Vec<Rc<RuleRow>>>>,
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
    let existing_groups = state
        .lib
        .virtual_album_rule_groups(album_id)
        .unwrap_or_default();
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
    let groups: Rc<RefCell<Vec<Rc<GroupBox>>>> = Rc::new(RefCell::new(Vec::new()));

    let rows_box = GtkBox::new(Orientation::Vertical, 6);
    rows_box.set_hexpand(true);

    let add_row = {
        let state = state.clone();
        let rows = rows.clone();
        let rows_box = rows_box.clone();
        Rc::new(move |rule: Option<&VirtualRule>| {
            add_row_to(&state, &rows_box, &rows, rule);
        })
    };

    let add_group = {
        let state = state.clone();
        let groups = groups.clone();
        let rows_box = rows_box.clone();
        Rc::new(move |group: Option<&RuleGroup>| {
            let gbox = build_group_box(&state, group);
            rows_box.append(&gbox.container);
            let gbox = Rc::new(gbox);
            // Wire "Remove group" now that the box is in an Rc.
            let groups2 = groups.clone();
            let rows_box2 = rows_box.clone();
            let gbox_weak = Rc::downgrade(&gbox);
            gbox.remove_btn.connect_clicked(move |_| {
                if let Some(gbox) = gbox_weak.upgrade() {
                    rows_box2.remove(&gbox.container);
                    groups2.borrow_mut().retain(|g| !Rc::ptr_eq(g, &gbox));
                }
            });
            groups.borrow_mut().push(gbox);
        })
    };

    if existing.is_empty() && existing_groups.is_empty() {
        add_row(None);
    } else {
        for r in &existing {
            add_row(Some(r));
        }
        for g in &existing_groups {
            add_group(Some(g));
        }
    }

    let btn_row = GtkBox::new(Orientation::Horizontal, 6);
    btn_row.set_halign(gtk4::Align::Start);
    let add_btn = Button::with_label("Add rule");
    let add_group_btn = Button::with_label("Add group");
    btn_row.append(&add_btn);
    btn_row.append(&add_group_btn);
    {
        let add_row = add_row.clone();
        add_btn.connect_clicked(move |_| add_row(None));
    }
    {
        let add_group = add_group.clone();
        add_group_btn.connect_clicked(move |_| add_group(None));
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
    root.append(&btn_row);
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
        let groups = groups.clone();
        let match_drop = match_drop.clone();
        let on_saved = Rc::new(on_saved);
        save.connect_clicked(move |_| {
            let mut collected: Vec<VirtualRule> = Vec::new();
            for row in rows.borrow().iter() {
                if let Some(rule) = collect_rule(album_id, row) {
                    collected.push(rule);
                }
            }
            let mut collected_groups: Vec<RuleGroup> = Vec::new();
            for gbox in groups.borrow().iter() {
                let rule_match = if gbox.match_drop.selected() == 1 {
                    RuleMatch::And
                } else {
                    RuleMatch::Or
                };
                let rules: Vec<VirtualRule> = gbox
                    .rows
                    .borrow()
                    .iter()
                    .filter_map(|row| collect_rule(album_id, row))
                    .collect();
                // Drop empty groups client-side; the query builder also
                // ignores them defensively.
                if !rules.is_empty() {
                    collected_groups.push(RuleGroup {
                        id: 0,
                        rule_match,
                        rules,
                    });
                }
            }
            let match_mode = if match_drop.selected() == 1 {
                RuleMatch::And
            } else {
                RuleMatch::Or
            };
            if let Err(e) = state.lib.set_virtual_album_rules_grouped(
                album_id,
                match_mode,
                &collected,
                &collected_groups,
            ) {
                show_error(&state, &e.to_string());
                return;
            }
            on_saved();
            window.close();
        });
    }

    window.set_visible(true);
}

/// Build one rule row and wire its remove button and field-swap handling,
/// appending it into `rows_box` and pushing it into `rows`.
fn add_row_to(
    state: &Rc<AppState>,
    rows_box: &GtkBox,
    rows: &Rc<RefCell<Vec<Rc<RuleRow>>>>,
    rule: Option<&VirtualRule>,
) {
    let row = build_rule_row(state, rule);
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
    // Swap the value widget (text vs. picker) when the field kind changes.
    {
        let state = state.clone();
        let row_weak = Rc::downgrade(&row);
        row.field.connect_selected_notify(move |field_dd| {
            if let Some(row) = row_weak.upgrade() {
                let idx = field_dd.selected() as usize;
                let new_field = FIELDS.get(idx).map(|(f, _)| *f).unwrap_or(RuleField::Tag);
                set_value_widget(&row.value_holder, &row.value, &state, new_field, None);
            }
        });
    }
    rows.borrow_mut().push(row);
}

/// Build a rule group's editor box: header (match-mode dropdown + remove
/// button), its own rows, and an "Add rule to group" button.
fn build_group_box(state: &Rc<AppState>, group: Option<&RuleGroup>) -> GroupBox {
    let match_list = StringList::new(&["Match ANY (OR)", "Match ALL (AND)"]);
    let match_drop = DropDown::new(Some(match_list), gtk4::Expression::NONE);
    match_drop.set_selected(match group.map(|g| g.rule_match).unwrap_or(RuleMatch::Or) {
        RuleMatch::Or => 0,
        RuleMatch::And => 1,
    });

    let remove_btn = Button::from_icon_name("list-remove-symbolic");

    let header = GtkBox::new(Orientation::Horizontal, 6);
    header.append(&Label::new(Some("Group:")));
    header.append(&match_drop);
    header.append(&remove_btn);

    let rows_box = GtkBox::new(Orientation::Vertical, 6);
    rows_box.set_hexpand(true);

    let add_row_btn = Button::with_label("Add rule to group");
    add_row_btn.set_halign(gtk4::Align::Start);

    let inner = GtkBox::new(Orientation::Vertical, 6);
    inner.set_margin_start(12);
    inner.set_margin_top(6);
    inner.set_margin_bottom(6);
    inner.set_margin_end(6);
    inner.append(&header);
    inner.append(&rows_box);
    inner.append(&add_row_btn);

    let container = Frame::new(None);
    container.set_child(Some(&inner));

    let rows: Rc<RefCell<Vec<Rc<RuleRow>>>> = Rc::new(RefCell::new(Vec::new()));

    match group {
        Some(g) if !g.rules.is_empty() => {
            for r in &g.rules {
                add_row_to(state, &rows_box, &rows, Some(r));
            }
        }
        _ => add_row_to(state, &rows_box, &rows, None),
    }

    {
        let state = state.clone();
        let rows_box = rows_box.clone();
        let rows = rows.clone();
        add_row_btn.connect_clicked(move |_| {
            add_row_to(&state, &rows_box, &rows, None);
        });
    }

    GroupBox {
        container,
        match_drop,
        remove_btn,
        rows,
    }
}

/// The dropdown choices for a field that uses a name picker (person/
/// character), or `None` for a free-text field.
fn choices_for_field(state: &Rc<AppState>, field: RuleField) -> Option<Vec<String>> {
    match field {
        RuleField::Person => Some(
            state
                .lib
                .persons()
                .unwrap_or_default()
                .into_iter()
                .map(|(p, _)| p.name)
                .collect(),
        ),
        RuleField::Character => Some(
            state
                .lib
                .characters()
                .unwrap_or_default()
                .into_iter()
                .map(|(c, _)| c.name)
                .collect(),
        ),
        _ => None,
    }
}

/// Replace `value_holder`'s child with the widget appropriate for `field`
/// (a name picker for Person/Character, free text otherwise), pre-selected or
/// pre-filled from `preset` when given.
fn set_value_widget(
    value_holder: &GtkBox,
    value: &RefCell<ValueInput>,
    state: &Rc<AppState>,
    field: RuleField,
    preset: Option<&str>,
) {
    while let Some(child) = value_holder.first_child() {
        value_holder.remove(&child);
    }
    match choices_for_field(state, field) {
        Some(choices) => {
            let label_refs: Vec<&str> = choices.iter().map(|s| s.as_str()).collect();
            let sl = StringList::new(&label_refs);
            let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
            drop.set_hexpand(true);
            if let Some(v) = preset {
                if let Some(idx) = choices.iter().position(|c| c == v) {
                    drop.set_selected(idx as u32);
                }
            }
            value_holder.append(&drop);
            *value.borrow_mut() = ValueInput::Picker { drop, choices };
        }
        None => {
            let entry = Entry::new();
            entry.set_hexpand(true);
            if let Some(v) = preset {
                entry.set_text(v);
            }
            value_holder.append(&entry);
            *value.borrow_mut() = ValueInput::Text(entry);
        }
    }
}

/// Build an editable rule row, pre-filled from `rule` when given.
fn build_rule_row(state: &Rc<AppState>, rule: Option<&VirtualRule>) -> RuleRow {
    let labels: Vec<&str> = FIELDS.iter().map(|(_, l)| *l).collect();
    let list = StringList::new(&labels);
    let field = DropDown::new(Some(list), gtk4::Expression::NONE);

    let initial_field = rule.map(|r| r.field).unwrap_or(FIELDS[0].0);
    if let Some(rule) = rule {
        let idx = FIELDS
            .iter()
            .position(|(f, _)| *f == rule.field)
            .unwrap_or(0);
        field.set_selected(idx as u32);
    }

    let value_holder = GtkBox::new(Orientation::Horizontal, 0);
    value_holder.set_hexpand(true);
    let value = RefCell::new(ValueInput::Text(Entry::new()));
    let preset = rule.map(display_value);
    set_value_widget(
        &value_holder,
        &value,
        state,
        initial_field,
        preset.as_deref(),
    );

    let remove = Button::from_icon_name("list-remove-symbolic");

    let container = GtkBox::new(Orientation::Horizontal, 6);
    container.append(&field);
    container.append(&value_holder);
    container.append(&remove);

    RuleRow {
        container,
        field,
        value_holder,
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
    let raw = match &*row.value.borrow() {
        ValueInput::Text(entry) => entry.text().to_string(),
        ValueInput::Picker { drop, choices } => choices
            .get(drop.selected() as usize)
            .cloned()
            .unwrap_or_default(),
    };
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let (op, value) = match field {
        RuleField::Tag => (RuleOp::Has, raw.clone()),
        RuleField::DateFrom => (RuleOp::Gte, date_to_unix(&raw)?.to_string()),
        RuleField::DateTo => (RuleOp::Lte, date_to_unix(&raw)?.to_string()),
        RuleField::Filename => (RuleOp::Contains, raw.clone()),
        RuleField::Path => (RuleOp::Contains, raw.clone()),
        RuleField::Folder => (RuleOp::Eq, raw.parse::<i64>().ok()?.to_string()),
        RuleField::Person => (RuleOp::Has, raw.clone()),
        RuleField::Character => (RuleOp::Has, raw.clone()),
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
