package ui

import (
	"context"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// ShowSettings opens the settings window. It currently manages Library folders;
// it is structured with a stack so more setting groups can be added later.
func (a *App) ShowSettings() {
	win := gtk.NewWindow()
	win.SetTitle("Settings")
	win.SetModal(true)
	win.SetTransientFor(&a.win.Window)
	win.SetDefaultSize(640, 420)

	stack := gtk.NewStack()
	stack.SetVExpand(true)
	stack.AddTitled(a.buildFolderSettings(win), "folders", "Library Folders")
	stack.AddTitled(a.buildThumbSettings(win), "thumbs", "Thumbnails")
	stack.AddTitled(a.buildAISettings(win), "ai", "AI Tagging")
	stack.AddTitled(a.buildStorageSettings(win), "storage", "Data Location")
	stack.AddTitled(a.buildShortcutSettings(win), "shortcuts", "Shortcuts")

	switcher := gtk.NewStackSidebar()
	switcher.SetStack(stack)

	body := gtk.NewBox(gtk.OrientationHorizontal, 0)
	body.Append(switcher)
	body.Append(stack)

	win.SetChild(body)
	win.SetVisible(true)
}

// buildFolderSettings builds the Library-folders management pane.
func (a *App) buildFolderSettings(parent *gtk.Window) gtk.Widgetter {
	model := gtk.NewStringList(nil)

	reload := func() {
		fs, err := a.lib.LibraryFolders()
		if err != nil {
			a.showError(err)
			return
		}
		paths := make([]string, 0, len(fs))
		for _, lf := range fs {
			paths = append(paths, lf.Path)
		}
		model.Splice(0, model.NItems(), paths)
	}
	reload()

	selection := gtk.NewSingleSelection(&model.ListModel)
	factory := gtk.NewSignalListItemFactory()
	factory.ConnectSetup(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		label := gtk.NewLabel("")
		label.SetXAlign(0)
		item.SetChild(label)
	})
	factory.ConnectBind(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		label, _ := item.Child().(*gtk.Label)
		so, _ := item.Item().Cast().(*gtk.StringObject)
		if label != nil && so != nil {
			label.SetText(so.String())
		}
	})
	list := gtk.NewListView(selection, &factory.ListItemFactory)

	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetChild(list)

	addBtn := gtk.NewButtonWithLabel("Add Folder…")
	addBtn.ConnectClicked(func() {
		dialog := gtk.NewFileDialog()
		dialog.SetTitle("Add Library Folder")
		dialog.SelectFolder(context.Background(), parent, func(res gio.AsyncResulter) {
			file, err := dialog.SelectFolderFinish(res)
			if err != nil || file == nil {
				return
			}
			a.AddLibraryFolder(file.Path())
			reload()
		})
	})

	removeBtn := gtk.NewButtonWithLabel("Remove")
	removeBtn.AddCSSClass("destructive-action")
	removeBtn.ConnectClicked(func() {
		pos := selection.Selected()
		obj := model.Item(pos)
		if obj == nil {
			return
		}
		so, ok := obj.Cast().(*gtk.StringObject)
		if !ok {
			return
		}
		path := so.String()
		a.confirm("Remove folder",
			"Remove \""+path+"\" from the library? Scanned entries for this folder will be deleted.",
			func() {
				if err := a.lib.RemoveLibraryFolder(path); err != nil {
					a.showError(err)
					return
				}
				a.sidebar.Reload()
				a.folderTree.Reload()
				reload()
			})
	})

	buttons := gtk.NewBox(gtk.OrientationHorizontal, 6)
	buttons.Append(addBtn)
	buttons.Append(removeBtn)

	help := gtk.NewLabel("Folders added here are scanned into your library.")
	help.SetXAlign(0)
	help.SetWrap(true)

	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)
	box.Append(help)
	box.Append(buttons)
	box.Append(scroll)
	return box
}

// buildThumbSettings builds the Thumbnails preferences pane: the four preset
// resolutions, regenerate-on-move, save-all-sizes, and a clear-cache button.
func (a *App) buildThumbSettings(parent *gtk.Window) gtk.Widgetter {
	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)

	intro := gtk.NewLabel("Thumbnail sizes for the four slider positions (longest side, pixels).")
	intro.SetXAlign(0)
	intro.SetWrap(true)
	box.Append(intro)

	labels := []string{"Smallest", "Small", "Large", "Largest"}
	spins := make([]*gtk.SpinButton, 4)
	for i := 0; i < 4; i++ {
		row := gtk.NewBox(gtk.OrientationHorizontal, 8)
		name := gtk.NewLabel(labels[i])
		name.SetXAlign(0)
		name.SetSizeRequest(90, -1)
		spin := gtk.NewSpinButtonWithRange(32, 2048, 16)
		spin.SetValue(float64(a.prefs.sizes[i]))
		spins[i] = spin
		row.Append(name)
		row.Append(spin)
		box.Append(row)
	}

	apply := gtk.NewButtonWithLabel("Apply Sizes")
	apply.ConnectClicked(func() {
		for i := 0; i < 4; i++ {
			a.prefs.sizes[i] = int(spins[i].Value())
		}
		a.lib.SetSetting(keyThumbSizes, formatSizes(a.prefs.sizes))
		a.applyThumbPrefs()
		a.grid.SetThumbSize(a.prefs.sizes[a.prefs.active])
	})
	box.Append(apply)

	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))

	regen := gtk.NewCheckButtonWithLabel("Regenerate thumbnails when moving the slider")
	regen.SetActive(a.prefs.regenOnMove)
	regen.ConnectToggled(func() {
		a.prefs.regenOnMove = regen.Active()
		a.lib.SetSetting(keyRegenOnMove, boolToStr(regen.Active()))
	})
	box.Append(regen)

	saveAll := gtk.NewCheckButtonWithLabel("Save all thumbnail sizes (faster switching, more storage)")
	saveAll.SetActive(a.prefs.saveAllSizes)
	saveAll.ConnectToggled(func() {
		a.prefs.saveAllSizes = saveAll.Active()
		a.lib.SetSetting(keySaveAllSizes, boolToStr(saveAll.Active()))
		a.applyThumbPrefs()
	})
	box.Append(saveAll)

	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))

	clear := gtk.NewButtonWithLabel("Clear Thumbnail Cache")
	clear.AddCSSClass("destructive-action")
	clear.ConnectClicked(func() {
		a.confirm("Clear thumbnail cache",
			"Delete all cached thumbnails? They will be regenerated on demand.",
			func() {
				if err := a.gen.ClearAll(); err != nil {
					a.showError(err)
					return
				}
				a.thumbCache = newThumbCache(512)
				a.grid.RefreshVisible()
				a.showInfo("Thumbnails", "Thumbnail cache cleared.")
			})
	})
	box.Append(clear)

	return box
}

func boolToStr(b bool) string {
	if b {
		return "1"
	}
	return "0"
}

// buildStorageSettings lets the user choose where the database files are stored.
// This is the only setting kept in the config file (~/.config/pichouse/config);
// all other settings live in library.db. Changing it takes effect on restart.
func (a *App) buildStorageSettings(parent *gtk.Window) gtk.Widgetter {
	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)

	intro := gtk.NewLabel("Location of the pichouse database files. Changing this takes effect after restarting the application.")
	intro.SetXAlign(0)
	intro.SetWrap(true)
	box.Append(intro)

	current, _ := db.DataDir()
	pathLabel := gtk.NewLabel(current)
	pathLabel.SetXAlign(0)
	pathLabel.SetSelectable(true)
	pathLabel.SetWrap(true)
	box.Append(pathLabel)

	choose := gtk.NewButtonWithLabel("Choose Folder…")
	choose.ConnectClicked(func() {
		dialog := gtk.NewFileDialog()
		dialog.SetTitle("Choose Data Folder")
		dialog.SelectFolder(context.Background(), parent, func(res gio.AsyncResulter) {
			file, err := dialog.SelectFolderFinish(res)
			if err != nil || file == nil {
				return
			}
			if err := db.WriteConfiguredDataDir(file.Path()); err != nil {
				a.showError(err)
				return
			}
			pathLabel.SetText(file.Path())
			a.showInfo("Data Location", "Data location updated. Restart pichouse for it to take effect.")
		})
	})
	box.Append(choose)

	return box
}

// buildShortcutSettings builds the image-viewer keyboard-shortcut pane. Each
// action shows its current key and a button that captures a new key press.
func (a *App) buildShortcutSettings(parent *gtk.Window) gtk.Widgetter {
	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)

	intro := gtk.NewLabel("Keyboard shortcuts used in the image viewer.")
	intro.SetXAlign(0)
	intro.SetWrap(true)
	box.Append(intro)

	grid := gtk.NewGrid()
	grid.SetRowSpacing(6)
	grid.SetColumnSpacing(12)
	box.Append(grid)

	for row, d := range shortcutDefs {
		def := d // capture

		name := gtk.NewLabel(def.label)
		name.SetXAlign(0)

		keyLabel := gtk.NewLabel(keyvalLabel(a.shortcuts.keyval(def.action)))
		keyLabel.SetXAlign(0)
		keyLabel.SetSizeRequest(120, -1)

		change := gtk.NewButtonWithLabel("Change…")
		change.ConnectClicked(func() {
			a.captureShortcut(parent, def, func(keyval uint) {
				a.shortcuts.set(def.action, keyval)
				a.lib.SetSetting(shortcutSettingKey(def.action), gdk.KeyvalName(keyval))
				keyLabel.SetText(keyvalLabel(keyval))
				a.viewer.RefreshTooltips()
			})
		})

		grid.Attach(name, 0, row, 1, 1)
		grid.Attach(keyLabel, 1, row, 1, 1)
		grid.Attach(change, 2, row, 1, 1)
	}

	reset := gtk.NewButtonWithLabel("Reset to Defaults")
	reset.ConnectClicked(func() {
		for _, d := range shortcutDefs {
			a.shortcuts.set(d.action, d.defKey)
			a.lib.SetSetting(shortcutSettingKey(d.action), gdk.KeyvalName(d.defKey))
		}
		a.viewer.RefreshTooltips()
		// Rebuild the settings window so labels refresh.
		parent.Destroy()
		a.ShowSettings()
	})
	box.Append(reset)

	return box
}

// captureShortcut opens a small modal that captures the next key press and
// reports it via onKey. Escape cancels without changing the binding.
func (a *App) captureShortcut(parent *gtk.Window, def shortcutDef, onKey func(uint)) {
	dlg := gtk.NewWindow()
	dlg.SetTitle("Set shortcut")
	dlg.SetModal(true)
	dlg.SetTransientFor(parent)
	dlg.SetDefaultSize(320, 120)

	msg := gtk.NewLabel("Press a key for \"" + def.label + "\"\n(Escape to cancel)")
	msg.SetJustify(gtk.JustifyCenter)
	msg.SetVExpand(true)
	msg.SetHExpand(true)

	body := gtk.NewBox(gtk.OrientationVertical, 0)
	body.Append(msg)
	dlg.SetChild(body)

	keys := gtk.NewEventControllerKey()
	keys.ConnectKeyPressed(func(keyval, keycode uint, state gdk.ModifierType) bool {
		if keyval == gdk.KEY_Escape {
			dlg.Destroy()
			return true
		}
		onKey(keyval)
		dlg.Destroy()
		return true
	})
	dlg.AddController(keys)

	dlg.SetVisible(true)
}
