//! Export of "baked" copies: apply a photo's non-destructive edits at full
//! resolution and write a new image file. Used by the edit panel and the grid
//! right-click menu.
//!
//! The user picks the format (JPEG/PNG) and JPEG quality once; the choice is
//! remembered in `library.db` settings and pre-filled next time. A single photo
//! opens a Save dialog; several photos open a folder chooser and each file is
//! written as `<stem>-edited.<ext>`.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Label, Orientation, SpinButton, StringList, Window,
};
use image::ImageEncoder;

use crate::model::{Photo, PhotoEdit};

use super::prefs;
use super::state::{show_error, AppState};

/// Export the given `(photo, edit)` pairs. Opens the options dialog first.
pub fn export_photos(state: &Rc<AppState>, items: Vec<(Photo, PhotoEdit)>) {
    let items: Vec<(Photo, PhotoEdit)> = items
        .into_iter()
        .filter(|(p, _)| p.id != 0)
        .collect();
    if items.is_empty() {
        return;
    }
    open_options(state, items);
}

/// The remembered export settings.
struct ExportOpts {
    /// "jpeg" or "png".
    format: String,
    quality: u8,
}

fn load_opts(state: &Rc<AppState>) -> ExportOpts {
    let format = state
        .lib
        .get_setting(prefs::KEY_EXPORT_FORMAT, prefs::DEFAULT_EXPORT_FORMAT)
        .unwrap_or_else(|_| prefs::DEFAULT_EXPORT_FORMAT.to_string());
    let quality = state
        .lib
        .get_setting(
            prefs::KEY_EXPORT_JPEG_QUALITY,
            &prefs::DEFAULT_EXPORT_JPEG_QUALITY.to_string(),
        )
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(prefs::DEFAULT_EXPORT_JPEG_QUALITY)
        .clamp(1, 100) as u8;
    ExportOpts { format, quality }
}

fn save_opts(state: &Rc<AppState>, opts: &ExportOpts) {
    let _ = state.lib.set_setting(prefs::KEY_EXPORT_FORMAT, &opts.format);
    let _ = state
        .lib
        .set_setting(prefs::KEY_EXPORT_JPEG_QUALITY, &opts.quality.to_string());
}

/// Show the format/quality dialog, then run the file chooser and export.
fn open_options(state: &Rc<AppState>, items: Vec<(Photo, PhotoEdit)>) {
    let opts = load_opts(state);

    let format_drop = DropDown::new(
        Some(StringList::new(&["JPEG", "PNG"])),
        gtk4::Expression::NONE,
    );
    format_drop.set_selected(if opts.format == "png" { 1 } else { 0 });

    let quality = SpinButton::with_range(1.0, 100.0, 1.0);
    quality.set_value(opts.quality as f64);
    let quality_row = GtkBox::new(Orientation::Horizontal, 6);
    quality_row.append(&Label::new(Some("JPEG quality")));
    quality_row.append(&quality);
    // Quality only applies to JPEG.
    quality_row.set_sensitive(opts.format != "png");
    {
        let quality_row = quality_row.clone();
        format_drop.connect_selected_notify(move |d| {
            quality_row.set_sensitive(d.selected() == 0);
        });
    }

    let count_lbl = Label::new(Some(&format!(
        "Export {} photo(s) with edits baked in.",
        items.len()
    )));
    count_lbl.set_xalign(0.0);

    let ok = Button::with_label("Choose destination…");
    ok.add_css_class("suggested-action");
    let cancel = Button::with_label("Cancel");
    let btns = GtkBox::new(Orientation::Horizontal, 6);
    btns.set_halign(gtk4::Align::End);
    btns.append(&cancel);
    btns.append(&ok);

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&count_lbl);
    let frow = GtkBox::new(Orientation::Horizontal, 6);
    frow.append(&Label::new(Some("Format")));
    frow.append(&format_drop);
    root.append(&frow);
    root.append(&quality_row);
    root.append(&btns);

    let win = Window::builder()
        .title("Export edited copies")
        .modal(true)
        .default_width(360)
        .child(&root)
        .build();
    if let Some(p) = state.window() {
        win.set_transient_for(Some(&p));
    }
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let state = state.clone();
        let win = win.clone();
        let format_drop = format_drop.clone();
        let quality = quality.clone();
        ok.connect_clicked(move |_| {
            let opts = ExportOpts {
                format: if format_drop.selected() == 1 { "png" } else { "jpeg" }.to_string(),
                quality: quality.value().round().clamp(1.0, 100.0) as u8,
            };
            save_opts(&state, &opts);
            win.close();
            choose_and_export(&state, items.clone(), opts);
        });
    }
    win.set_visible(true);
}

/// Run the file chooser (Save for one, folder for many) and export.
fn choose_and_export(state: &Rc<AppState>, items: Vec<(Photo, PhotoEdit)>, opts: ExportOpts) {
    let ext = if opts.format == "png" { "png" } else { "jpg" };
    if items.len() == 1 {
        let (photo, edit) = items[0].clone();
        let dialog = gtk4::FileChooserDialog::new(
            Some("Export edited copy"),
            state.window().as_ref(),
            gtk4::FileChooserAction::Save,
            &[
                ("Cancel", gtk4::ResponseType::Cancel),
                ("Save", gtk4::ResponseType::Accept),
            ],
        );
        dialog.set_current_name(&format!("{}-edited.{ext}", stem(&photo.filename)));
        let state = state.clone();
        let opts = Rc::new(opts);
        dialog.connect_response(move |d, resp| {
            if resp == gtk4::ResponseType::Accept {
                if let Some(path) = d.file().and_then(|f| f.path()) {
                    if let Err(e) = bake_and_write(&state, &photo, &edit, &path, &opts) {
                        show_error(&state, &e);
                    } else {
                        state.status().set_message("Exported 1 photo.");
                    }
                }
            }
            d.close();
        });
        dialog.set_modal(true);
        dialog.set_visible(true);
    } else {
        let dialog = gtk4::FileChooserDialog::new(
            Some("Export edited copies to folder"),
            state.window().as_ref(),
            gtk4::FileChooserAction::SelectFolder,
            &[
                ("Cancel", gtk4::ResponseType::Cancel),
                ("Select", gtk4::ResponseType::Accept),
            ],
        );
        let state = state.clone();
        let opts = Rc::new(opts);
        let ext = ext.to_string();
        dialog.connect_response(move |d, resp| {
            if resp == gtk4::ResponseType::Accept {
                if let Some(dir) = d.file().and_then(|f| f.path()) {
                    let mut ok = 0usize;
                    let mut fail = 0usize;
                    for (photo, edit) in &items {
                        let path = dir.join(format!("{}-edited.{ext}", stem(&photo.filename)));
                        match bake_and_write(&state, photo, edit, &path, &opts) {
                            Ok(()) => ok += 1,
                            Err(_) => fail += 1,
                        }
                    }
                    let msg = if fail == 0 {
                        format!("Exported {ok} photo(s).")
                    } else {
                        format!("Exported {ok}, {fail} failed.")
                    };
                    state.status().set_message(&msg);
                }
            }
            d.close();
        });
        dialog.set_modal(true);
        dialog.set_visible(true);
    }
}

/// Load the original (local or Immich), apply the 90° orientation then the
/// edits at full resolution, and write the file per `opts`.
fn bake_and_write(
    state: &Rc<AppState>,
    photo: &Photo,
    edit: &PhotoEdit,
    path: &std::path::Path,
    opts: &ExportOpts,
) -> Result<(), String> {
    let img = super::editor::load_image_for_edit(state, photo)
        .ok_or_else(|| format!("Could not read the source image for {}", photo.filename))?;
    let img = rotate_full(img, photo.orientation);
    let out = crate::edit::apply_edits(img, edit);
    write_image(&out, path, opts)
}

fn write_image(
    img: &image::RgbaImage,
    path: &std::path::Path,
    opts: &ExportOpts,
) -> Result<(), String> {
    if opts.format == "png" {
        img.save(path).map_err(|e| e.to_string())
    } else {
        let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::new(file);
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, opts.quality);
        enc.write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())
    }
}

/// Rotate a full-resolution image clockwise by 0/90/180/270 degrees.
pub(crate) fn rotate_full(img: image::RgbaImage, degrees: i32) -> image::RgbaImage {
    let d = ((degrees % 360) + 360) % 360;
    match d {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

fn stem(filename: &str) -> String {
    match filename.rsplit_once('.') {
        Some((s, _)) => s.to_string(),
        None => filename.to_string(),
    }
}
