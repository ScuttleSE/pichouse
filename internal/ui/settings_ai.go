package ui

import (
	"context"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/ai"
)

// buildAISettings builds the AI Tagging preferences pane: enable toggle,
// backend host/port, model, concurrency, managed-subprocess options, and a
// live "Test connection" that reports server and model availability.
func (a *App) buildAISettings(parent *gtk.Window) gtk.Widgetter {
	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)

	intro := gtk.NewLabel("Local AI keyword tagging. pichouse talks to a local " +
		"Ollama server; no data leaves your machine and no models are downloaded " +
		"automatically. Install Ollama and pull a vision model (e.g. " +
		"\"ollama pull moondream\").")
	intro.SetXAlign(0)
	intro.SetWrap(true)
	box.Append(intro)

	enable := gtk.NewCheckButtonWithLabel("Enable AI tagging")
	enable.SetActive(a.aiConfig.Enabled)
	enable.ConnectToggled(func() {
		a.aiConfig.Enabled = enable.Active()
		a.lib.SetSetting(keyAIEnabled, boolToStr(enable.Active()))
	})
	box.Append(enable)

	// Host / port.
	hostRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	hostRow.Append(fixedLabel("Host", 90))
	hostEntry := gtk.NewEntry()
	hostEntry.SetText(a.aiConfig.Host)
	hostEntry.SetHExpand(true)
	hostEntry.ConnectChanged(func() {
		a.aiConfig.Host = hostEntry.Text()
		a.lib.SetSetting(keyAIHost, hostEntry.Text())
	})
	hostRow.Append(hostEntry)
	portEntry := gtk.NewEntry()
	portEntry.SetText(strconv.Itoa(a.aiConfig.Port))
	portEntry.SetMaxWidthChars(6)
	portEntry.ConnectChanged(func() {
		if n, err := strconv.Atoi(strings.TrimSpace(portEntry.Text())); err == nil && n > 0 {
			a.aiConfig.Port = n
			a.lib.SetSetting(keyAIPort, strconv.Itoa(n))
		}
	})
	hostRow.Append(portEntry)
	box.Append(hostRow)

	// Model.
	modelRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	modelRow.Append(fixedLabel("Model", 90))
	modelEntry := gtk.NewEntry()
	modelEntry.SetText(a.aiConfig.Model)
	modelEntry.SetHExpand(true)
	modelEntry.ConnectChanged(func() {
		a.aiConfig.Model = modelEntry.Text()
		a.lib.SetSetting(keyAIModel, modelEntry.Text())
	})
	modelRow.Append(modelEntry)
	box.Append(modelRow)

	// Concurrency.
	concRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	concRow.Append(fixedLabel("Concurrency", 90))
	concSpin := gtk.NewSpinButtonWithRange(1, 16, 1)
	concSpin.SetValue(float64(a.aiConfig.Concurrency))
	concSpin.ConnectValueChanged(func() {
		a.aiConfig.Concurrency = int(concSpin.Value())
		a.lib.SetSetting(keyAIConcurrency, strconv.Itoa(a.aiConfig.Concurrency))
	})
	concRow.Append(concSpin)
	box.Append(concRow)

	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))

	// Managed subprocess.
	manage := gtk.NewCheckButtonWithLabel("Let pichouse start Ollama automatically when needed")
	manage.SetActive(a.aiConfig.Manage)
	manage.ConnectToggled(func() {
		a.aiConfig.Manage = manage.Active()
		a.lib.SetSetting(keyAIManage, boolToStr(manage.Active()))
	})
	box.Append(manage)

	binRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	binRow.Append(fixedLabel("ollama path", 90))
	binEntry := gtk.NewEntry()
	binEntry.SetText(a.aiConfig.BinaryPath)
	binEntry.SetPlaceholderText("(search PATH)")
	binEntry.SetHExpand(true)
	binEntry.ConnectChanged(func() {
		a.aiConfig.BinaryPath = binEntry.Text()
		a.lib.SetSetting(keyAIBinary, binEntry.Text())
	})
	binRow.Append(binEntry)
	box.Append(binRow)

	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))

	// Test connection.
	status := gtk.NewLabel("")
	status.SetXAlign(0)
	status.SetWrap(true)
	test := gtk.NewButtonWithLabel("Test Connection")
	test.ConnectClicked(func() {
		cfg := a.aiConfig
		client := ai.NewClient(cfg.Host, cfg.Port)
		status.SetText("Checking…")
		go func() {
			ok, models, err := client.Detect(context.Background())
			onUI(func() {
				if !ok || err != nil {
					status.SetText("No Ollama server found at " + cfg.Host + ":" +
						strconv.Itoa(cfg.Port) + ". Start Ollama or enable managed mode.")
					return
				}
				hasModel := false
				for _, m := range models {
					if m == cfg.Model || strings.HasPrefix(m, cfg.Model+":") {
						hasModel = true
						break
					}
				}
				if hasModel {
					status.SetText("Server OK. Model \"" + cfg.Model + "\" is installed.")
				} else {
					status.SetText("Server OK, but model \"" + cfg.Model +
						"\" is not installed. Run: ollama pull " + cfg.Model)
				}
			})
		}()
	})
	box.Append(test)
	box.Append(status)

	return box
}

// fixedLabel returns a left-aligned label with a fixed width, used for form
// captions.
func fixedLabel(text string, width int) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.SetSizeRequest(width, -1)
	return l
}
