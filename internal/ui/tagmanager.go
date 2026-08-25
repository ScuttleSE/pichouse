package ui

import (
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// ShowTagManager opens a window listing all tags with photo counts and lets the
// user rename, merge, and delete tags globally.
func (a *App) ShowTagManager() {
	win := gtk.NewWindow()
	win.SetTitle("Tag Manager")
	win.SetModal(true)
	win.SetTransientFor(&a.win.Window)
	win.SetDefaultSize(420, 520)

	model := gtk.NewStringList(nil)
	// names holds the tag name for each row, in the same order as the model.
	var names []string

	reload := func() {
		tags, err := a.lib.AllTags()
		if err != nil {
			a.showError(err)
			return
		}
		names = names[:0]
		labels := make([]string, 0, len(tags))
		for _, t := range tags {
			names = append(names, t.Name)
			labels = append(labels, t.Name+"  ("+itoa(t.Count)+")")
		}
		model.Splice(0, model.NItems(), labels)
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

	selectedName := func() (string, bool) {
		pos := int(selection.Selected())
		if pos < 0 || pos >= len(names) {
			return "", false
		}
		return names[pos], true
	}

	renameBtn := gtk.NewButtonWithLabel("Rename…")
	renameBtn.ConnectClicked(func() {
		name, ok := selectedName()
		if !ok {
			return
		}
		a.promptText("Rename tag", "New name for \""+name+"\":", name, func(newName string) {
			if err := a.lib.RenameTag(name, newName); err != nil {
				a.showError(err)
				return
			}
			reload()
			a.refreshSelectedTags()
			a.grid.RefreshVisible()
		})
	})

	mergeBtn := gtk.NewButtonWithLabel("Merge into…")
	mergeBtn.ConnectClicked(func() {
		name, ok := selectedName()
		if !ok {
			return
		}
		a.promptText("Merge tag", "Merge \""+name+"\" into which tag?", "", func(dst string) {
			if err := a.lib.MergeTags(name, dst); err != nil {
				a.showError(err)
				return
			}
			reload()
			a.refreshSelectedTags()
			a.grid.RefreshVisible()
		})
	})

	deleteBtn := gtk.NewButtonWithLabel("Delete")
	deleteBtn.AddCSSClass("destructive-action")
	deleteBtn.ConnectClicked(func() {
		name, ok := selectedName()
		if !ok {
			return
		}
		a.confirm("Delete tag", "Delete \""+name+"\" from all photos?", func() {
			if err := a.lib.DeleteTag(name); err != nil {
				a.showError(err)
				return
			}
			reload()
			a.refreshSelectedTags()
			a.grid.RefreshVisible()
		})
	})

	buttons := gtk.NewBox(gtk.OrientationHorizontal, 6)
	buttons.Append(renameBtn)
	buttons.Append(mergeBtn)
	buttons.Append(deleteBtn)

	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)
	box.Append(buttons)
	box.Append(scroll)

	win.SetChild(box)
	win.SetVisible(true)
}
