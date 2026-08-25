package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// ShowSettings opens the settings dialog. It currently manages Library folders;
// it is structured with tabs so more setting groups can be added later.
func (a *App) ShowSettings() {
	tabs := container.NewAppTabs(
		container.NewTabItemWithIcon("Library Folders", theme.FolderIcon(), a.buildFolderSettings()),
	)
	tabs.SetTabLocation(container.TabLocationLeading)

	d := dialog.NewCustom("Settings", "Close", tabs, a.win)
	d.Resize(fyne.NewSize(640, 420))
	d.Show()
}

// buildFolderSettings builds the Library-folders management pane: a list of
// added roots with add/remove controls.
func (a *App) buildFolderSettings() fyne.CanvasObject {
	var folders []model.LibraryFolder
	list := widget.NewList(
		func() int { return len(folders) },
		func() fyne.CanvasObject {
			return container.NewHBox(widget.NewIcon(theme.FolderIcon()), widget.NewLabel("template"))
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			box := obj.(*fyne.Container)
			if id >= 0 && id < len(folders) {
				box.Objects[1].(*widget.Label).SetText(folders[id].Path)
			}
		},
	)

	reload := func() {
		fs, err := a.lib.LibraryFolders()
		if err != nil {
			dialog.ShowError(err, a.win)
			return
		}
		folders = fs
		list.Refresh()
		list.UnselectAll()
	}
	reload()

	var selected int = -1
	list.OnSelected = func(id widget.ListItemID) { selected = id }
	list.OnUnselected = func(widget.ListItemID) { selected = -1 }

	addBtn := widget.NewButtonWithIcon("Add Folder…", theme.ContentAddIcon(), func() {
		dialog.ShowFolderOpen(func(uri fyne.ListableURI, err error) {
			if err != nil || uri == nil {
				return
			}
			a.AddLibraryFolder(uri.Path())
			reload()
		}, a.win)
	})

	removeBtn := widget.NewButtonWithIcon("Remove", theme.DeleteIcon(), func() {
		if selected < 0 || selected >= len(folders) {
			return
		}
		path := folders[selected].Path
		dialog.ShowConfirm("Remove folder",
			"Remove \""+path+"\" from the library?\nScanned entries for this folder will be deleted.",
			func(ok bool) {
				if !ok {
					return
				}
				if err := a.lib.RemoveLibraryFolder(path); err != nil {
					dialog.ShowError(err, a.win)
					return
				}
				a.sidebar.Reload()
				reload()
			}, a.win)
	})
	removeBtn.Importance = widget.DangerImportance

	buttons := container.NewHBox(addBtn, removeBtn)
	help := widget.NewLabel("Folders added here are scanned into your library.")
	help.Wrapping = fyne.TextWrapWord

	return container.NewBorder(
		container.NewVBox(help, buttons, widget.NewSeparator()),
		nil, nil, nil,
		list,
	)
}
