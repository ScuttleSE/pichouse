package ui

import (
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Viewer is the full-image view that replaces the thumbnail grid when a photo
// is opened. It supports left/right navigation and R to rotate 90° clockwise.
type Viewer struct {
	app     *App
	box     *gtk.Box
	picture *gtk.Picture
	header  *gtk.Label

	photos []model.Photo // the set being navigated (grid's current photos)
	index  int
}

func newViewer(a *App) *Viewer {
	v := &Viewer{app: a}

	back := gtk.NewButtonFromIconName("go-previous-symbolic")
	back.SetTooltipText("Back to grid")
	back.ConnectClicked(func() { a.CloseViewer() })

	prev := gtk.NewButtonFromIconName("media-skip-backward-symbolic")
	prev.SetTooltipText("Previous (Left)")
	prev.ConnectClicked(func() { v.navigate(-1) })

	next := gtk.NewButtonFromIconName("media-skip-forward-symbolic")
	next.SetTooltipText("Next (Right)")
	next.ConnectClicked(func() { v.navigate(1) })

	rotate := gtk.NewButtonFromIconName("object-rotate-right-symbolic")
	rotate.SetTooltipText("Rotate 90° (R)")
	rotate.ConnectClicked(func() { v.rotate() })

	v.header = gtk.NewLabel("")
	v.header.SetXAlign(0)
	v.header.SetHExpand(true)
	v.header.SetEllipsize(3) // PANGO_ELLIPSIZE_END

	bar := gtk.NewBox(gtk.OrientationHorizontal, 6)
	bar.SetMarginTop(6)
	bar.SetMarginBottom(6)
	bar.SetMarginStart(6)
	bar.SetMarginEnd(6)
	bar.Append(back)
	bar.Append(prev)
	bar.Append(next)
	bar.Append(rotate)
	bar.Append(v.header)

	v.picture = gtk.NewPicture()
	v.picture.SetCanShrink(true)
	v.picture.SetContentFit(gtk.ContentFitContain)
	v.picture.SetVExpand(true)
	v.picture.SetHExpand(true)

	v.box = gtk.NewBox(gtk.OrientationVertical, 0)
	v.box.Append(bar)
	v.box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	v.box.Append(v.picture)

	return v
}

// HandleKey processes a key press while the viewer is the active view. It is
// driven by a window-level key controller (capture phase) so navigation works
// without the viewer needing keyboard focus. Returns true if the key was
// consumed.
func (v *Viewer) HandleKey(keyval uint) bool {
	switch keyval {
	case gdk.KEY_Left:
		v.navigate(-1)
		return true
	case gdk.KEY_Right:
		v.navigate(1)
		return true
	case gdk.KEY_r, gdk.KEY_R:
		v.rotate()
		return true
	case gdk.KEY_Escape:
		v.app.CloseViewer()
		return true
	}
	return false
}

// Widget returns the viewer root widget.
func (v *Viewer) Widget() gtk.Widgetter { return v.box }

// Open displays the given photos with the initial index selected.
func (v *Viewer) Open(photos []model.Photo, index int) {
	v.photos = photos
	v.index = index
	v.show()
}

func (v *Viewer) navigate(delta int) {
	if len(v.photos) == 0 {
		return
	}
	v.index += delta
	if v.index < 0 {
		v.index = 0
	}
	if v.index >= len(v.photos) {
		v.index = len(v.photos) - 1
	}
	v.show()
}

// rotate rotates the current photo 90° clockwise, persists the orientation in
// the database (never on disk), invalidates cached thumbnails, and redisplays.
func (v *Viewer) rotate() {
	if v.index < 0 || v.index >= len(v.photos) {
		return
	}
	p := &v.photos[v.index]
	p.Orientation = ((p.Orientation + 90) % 360)
	if p.ID != 0 {
		if err := v.app.lib.SetOrientation(p.ID, p.Orientation); err != nil {
			v.app.showError(err)
		}
	}
	if p.Hash != "" {
		v.app.gen.Invalidate(p.Hash)
	}
	v.show()
	// Refresh the grid so the rotated thumbnail is regenerated on return.
	v.app.grid.RefreshVisible()
}

// show renders the current photo at full size with its orientation applied.
func (v *Viewer) show() {
	if v.index < 0 || v.index >= len(v.photos) {
		return
	}
	p := v.photos[v.index]
	v.header.SetText(p.Filename)
	v.app.properties.Show(p)

	go func(path string, rot int) {
		pb, err := loadRotatedPixbuf(path, rot)
		onUI(func() {
			if err != nil || pb == nil {
				v.picture.SetPaintable(nil)
				return
			}
			v.picture.SetPixbuf(pb)
		})
	}(p.Path, p.Orientation)
}

// loadRotatedPixbuf loads an image from disk and applies the given clockwise
// rotation for display only.
func loadRotatedPixbuf(path string, degrees int) (*gdkpixbuf.Pixbuf, error) {
	pb, err := gdkpixbuf.NewPixbufFromFile(path)
	if err != nil {
		return nil, err
	}
	degrees = ((degrees % 360) + 360) % 360
	switch degrees {
	case 90:
		pb = pb.RotateSimple(gdkpixbuf.PixbufRotateClockwise)
	case 180:
		pb = pb.RotateSimple(gdkpixbuf.PixbufRotateUpsidedown)
	case 270:
		pb = pb.RotateSimple(gdkpixbuf.PixbufRotateCounterclockwise)
	}
	return pb, nil
}
