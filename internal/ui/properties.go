package ui

import (
	"fmt"
	"path/filepath"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Properties is the right-hand properties panel for the selected photo. It is
// a tabbed panel: the "Pic Info" tab shows file/EXIF details, and the "Tags"
// tab shows and manages AI and user tags.
type Properties struct {
	app *App

	root     *gtk.Notebook
	title    *gtk.Label
	location *gtk.Label
	size     *gtk.Label
	date     *gtk.Label
	dims     *gtk.Label

	// Tags tab.
	tagList  *gtk.Box
	tagEntry *gtk.Entry
	tagNow   *gtk.Button

	current     model.Photo
	haveCurrent bool
}

func newProperties(a *App) *Properties {
	p := &Properties{app: a}
	p.title = boldLabel("Properties")
	p.location = valueLabel()
	p.size = valueLabel()
	p.date = valueLabel()
	p.dims = valueLabel()

	box := gtk.NewBox(gtk.OrientationVertical, 4)
	box.SetMarginTop(8)
	box.SetMarginBottom(8)
	box.SetMarginStart(8)
	box.SetMarginEnd(8)
	box.Append(p.title)
	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	box.Append(field("Location", p.location))
	box.Append(field("File Size", p.size))
	box.Append(field("File Date", p.date))
	box.Append(field("Dimensions", p.dims))

	notebook := gtk.NewNotebook()
	notebook.SetSizeRequest(240, -1)
	notebook.AppendPage(box, gtk.NewLabel("Pic Info"))
	notebook.AppendPage(p.buildTagsTab(), gtk.NewLabel("Tags"))
	p.root = notebook

	p.Clear()
	return p
}

// buildTagsTab builds the Tags tab: an add-tag entry, a per-photo "Tag now"
// button, and a scrollable list of the current photo's tags.
func (p *Properties) buildTagsTab() gtk.Widgetter {
	box := gtk.NewBox(gtk.OrientationVertical, 6)
	box.SetMarginTop(8)
	box.SetMarginBottom(8)
	box.SetMarginStart(8)
	box.SetMarginEnd(8)

	// Add-tag row.
	addRow := gtk.NewBox(gtk.OrientationHorizontal, 4)
	p.tagEntry = gtk.NewEntry()
	p.tagEntry.SetHExpand(true)
	p.tagEntry.SetPlaceholderText("Add a tag…")
	addBtn := gtk.NewButtonFromIconName("list-add-symbolic")
	addBtn.SetTooltipText("Add tag")
	add := func() {
		if !p.haveCurrent {
			return
		}
		name := p.tagEntry.Text()
		if name == "" {
			return
		}
		if err := p.app.lib.AddPhotoTags(p.current.ID, []string{name}, model.TagSourceUser); err != nil {
			p.app.showError(err)
			return
		}
		p.tagEntry.SetText("")
		p.reloadTags()
	}
	p.tagEntry.ConnectActivate(add)
	addBtn.ConnectClicked(add)
	addRow.Append(p.tagEntry)
	addRow.Append(addBtn)
	box.Append(addRow)

	// Per-photo tag-now button.
	p.tagNow = gtk.NewButtonWithLabel("Tag this photo with AI")
	p.tagNow.ConnectClicked(func() {
		if p.haveCurrent {
			p.app.tagOnePhotoNow(p.current)
		}
	})
	box.Append(p.tagNow)

	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))

	p.tagList = gtk.NewBox(gtk.OrientationVertical, 2)
	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetChild(p.tagList)
	box.Append(scroll)

	return box
}

// reloadTags repopulates the tag list for the current photo.
func (p *Properties) reloadTags() {
	// Clear existing rows.
	for {
		child := p.tagList.FirstChild()
		if child == nil {
			break
		}
		p.tagList.Remove(child)
	}
	if !p.haveCurrent {
		return
	}
	tags, err := p.app.lib.PhotoTags(p.current.ID)
	if err != nil {
		return
	}
	if len(tags) == 0 {
		none := gtk.NewLabel("No tags yet.")
		none.SetXAlign(0)
		p.tagList.Append(none)
		return
	}
	for _, t := range tags {
		p.tagList.Append(p.tagRow(t))
	}
}

// tagRow builds a single tag row with a source indicator and remove/confirm
// actions.
func (p *Properties) tagRow(t model.Tag) gtk.Widgetter {
	row := gtk.NewBox(gtk.OrientationHorizontal, 4)

	marker := "•"
	tip := "User tag"
	if t.Source == model.TagSourceAI {
		if t.Confirmed {
			marker = "✓"
			tip = "AI tag (confirmed)"
		} else {
			marker = "◆"
			tip = "AI tag"
		}
	}
	badge := gtk.NewLabel(marker)
	badge.SetTooltipText(tip)

	name := gtk.NewLabel(t.Name)
	name.SetXAlign(0)
	name.SetHExpand(true)
	name.SetWrap(true)

	row.Append(badge)
	row.Append(name)

	// Confirm button for unconfirmed AI tags.
	if t.Source == model.TagSourceAI && !t.Confirmed {
		confirm := gtk.NewButtonFromIconName("object-select-symbolic")
		confirm.SetTooltipText("Confirm this AI tag")
		confirm.SetHasFrame(false)
		tagName := t.Name
		confirm.ConnectClicked(func() {
			if err := p.app.lib.ConfirmPhotoTag(p.current.ID, tagName); err != nil {
				p.app.showError(err)
				return
			}
			p.reloadTags()
		})
		row.Append(confirm)
	}

	remove := gtk.NewButtonFromIconName("edit-delete-symbolic")
	remove.SetTooltipText("Remove tag from this photo")
	remove.SetHasFrame(false)
	tagName := t.Name
	remove.ConnectClicked(func() {
		if err := p.app.lib.RemovePhotoTag(p.current.ID, tagName); err != nil {
			p.app.showError(err)
			return
		}
		p.reloadTags()
	})
	row.Append(remove)

	return row
}

func boldLabel(text string) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.SetMarkup("<b>" + text + "</b>")
	return l
}

func valueLabel() *gtk.Label {
	l := gtk.NewLabel("")
	l.SetXAlign(0)
	l.SetWrap(true)
	l.SetSelectable(true)
	return l
}

func field(caption string, value *gtk.Label) *gtk.Box {
	b := gtk.NewBox(gtk.OrientationVertical, 0)
	b.SetMarginTop(6)
	b.Append(boldLabel(caption))
	b.Append(value)
	return b
}

// Widget returns the properties panel root widget.
func (p *Properties) Widget() gtk.Widgetter { return p.root }

// SetVisible shows or hides the panel.
func (p *Properties) SetVisible(v bool) { p.root.SetVisible(v) }

// Clear resets the panel to an empty state.
func (p *Properties) Clear() {
	p.haveCurrent = false
	p.title.SetMarkup("<b>Properties</b>")
	p.location.SetText("—")
	p.size.SetText("—")
	p.date.SetText("—")
	p.dims.SetText("—")
	if p.tagNow != nil {
		p.tagNow.SetSensitive(false)
	}
	p.reloadTags()
}

// Show populates the panel from a photo.
func (p *Properties) Show(photo model.Photo) {
	p.current = photo
	p.haveCurrent = photo.ID != 0
	p.title.SetMarkup("<b>Properties of " + escapeMarkup(photo.Filename) + "</b>")
	p.location.SetText(filepath.Dir(photo.Path))
	p.size.SetText(humanSize(photo.Size))
	if photo.TakenAt.IsZero() {
		p.date.SetText(photo.ModTime.Format("2006-01-02 15:04:05"))
	} else {
		p.date.SetText(photo.TakenAt.Format("2006-01-02 15:04:05"))
	}
	if photo.Width > 0 && photo.Height > 0 {
		p.dims.SetText(fmt.Sprintf("%d x %d", photo.Width, photo.Height))
	} else {
		p.dims.SetText("—")
	}
	if p.tagNow != nil {
		p.tagNow.SetSensitive(p.haveCurrent)
	}
	p.reloadTags()
}

// CurrentID returns the id of the photo currently shown, or 0 if none.
func (p *Properties) CurrentID() int64 {
	if !p.haveCurrent {
		return 0
	}
	return p.current.ID
}

// humanSize formats a byte count as a human-readable string.
func humanSize(n int64) string {
	const unit = 1024
	if n < unit {
		return fmt.Sprintf("%d B", n)
	}
	div, exp := int64(unit), 0
	for m := n / unit; m >= unit; m /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %cB", float64(n)/float64(div), "KMGTPE"[exp])
}
