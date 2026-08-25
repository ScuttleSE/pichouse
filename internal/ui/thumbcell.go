package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// thumbCell is a tappable thumbnail with a filename caption.
type thumbCell struct {
	widget.BaseWidget

	image    *canvas.Image
	caption  *widget.Label
	onTapped func()
	edge     float32
}

func newThumbCell(edge float32) *thumbCell {
	c := &thumbCell{edge: edge}
	c.image = canvas.NewImageFromResource(theme.FileImageIcon())
	c.image.FillMode = canvas.ImageFillContain
	c.caption = widget.NewLabel("")
	c.caption.Alignment = fyne.TextAlignCenter
	c.caption.Truncation = fyne.TextTruncateEllipsis
	c.ExtendBaseWidget(c)
	return c
}

// setImage replaces the thumbnail image resource.
func (c *thumbCell) setImage(res fyne.Resource) {
	c.image.Resource = res
	c.image.Refresh()
}

// setPlaceholder resets the cell to the generic image icon.
func (c *thumbCell) setPlaceholder() {
	c.image.Resource = theme.FileImageIcon()
	c.image.Refresh()
}

func (c *thumbCell) setCaption(s string) { c.caption.SetText(s) }

// Tapped invokes the tap handler.
func (c *thumbCell) Tapped(_ *fyne.PointEvent) {
	if c.onTapped != nil {
		c.onTapped()
	}
}

// CreateRenderer lays out the image above the caption.
func (c *thumbCell) CreateRenderer() fyne.WidgetRenderer {
	img := c.image
	cap := c.caption
	edge := c.edge
	objects := []fyne.CanvasObject{img, cap}
	return &thumbCellRenderer{cell: c, image: img, caption: cap, edge: edge, objects: objects}
}

type thumbCellRenderer struct {
	cell    *thumbCell
	image   *canvas.Image
	caption *widget.Label
	edge    float32
	objects []fyne.CanvasObject
}

func (r *thumbCellRenderer) Layout(size fyne.Size) {
	capH := r.caption.MinSize().Height
	r.image.Move(fyne.NewPos(0, 0))
	r.image.Resize(fyne.NewSize(size.Width, size.Height-capH))
	r.caption.Move(fyne.NewPos(0, size.Height-capH))
	r.caption.Resize(fyne.NewSize(size.Width, capH))
}

func (r *thumbCellRenderer) MinSize() fyne.Size {
	capH := r.caption.MinSize().Height
	return fyne.NewSize(r.edge, r.edge+capH)
}

func (r *thumbCellRenderer) Refresh()                        { canvas.Refresh(r.cell) }
func (r *thumbCellRenderer) Objects() []fyne.CanvasObject    { return r.objects }
func (r *thumbCellRenderer) Destroy()                        {}
