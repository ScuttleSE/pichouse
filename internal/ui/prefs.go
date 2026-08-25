package ui

import (
	"strconv"
	"strings"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// Setting keys stored in library.db.
const (
	keyThumbSizes   = "thumb.sizes"    // comma-separated 4 preset px values
	keyThumbActive  = "thumb.active"   // active preset index (0..3)
	keyRegenOnMove  = "thumb.regen"    // "1" to regenerate thumbnails when moving slider
	keySaveAllSizes = "thumb.save_all" // "1" to cache all sizes on generation
	keyPropsVisible = "ui.props_visible"
)

// defaultThumbSizes are the four slider preset sizes in pixels.
var defaultThumbSizes = []int{96, 160, 240, 320}

// prefs holds the user's persisted thumbnail/UI preferences.
type prefs struct {
	sizes        []int // always length 4, ascending
	active       int   // 0..3
	regenOnMove  bool
	saveAllSizes bool
	propsVisible bool
}

// loadPrefs reads preferences from the library database, filling defaults.
func loadPrefs(lib *db.Library) prefs {
	p := prefs{
		sizes:        append([]int(nil), defaultThumbSizes...),
		active:       1,
		regenOnMove:  false,
		saveAllSizes: false,
		propsVisible: true,
	}
	if v, _ := lib.GetSetting(keyThumbSizes, ""); v != "" {
		if s := parseSizes(v); len(s) == 4 {
			p.sizes = s
		}
	}
	if v, _ := lib.GetSetting(keyThumbActive, ""); v != "" {
		if i, err := strconv.Atoi(v); err == nil && i >= 0 && i < 4 {
			p.active = i
		}
	}
	p.regenOnMove = boolSetting(lib, keyRegenOnMove, false)
	p.saveAllSizes = boolSetting(lib, keySaveAllSizes, false)
	p.propsVisible = boolSetting(lib, keyPropsVisible, true)
	return p
}

func boolSetting(lib *db.Library, key string, def bool) bool {
	d := "0"
	if def {
		d = "1"
	}
	v, _ := lib.GetSetting(key, d)
	return v == "1"
}

func parseSizes(s string) []int {
	parts := strings.Split(s, ",")
	out := make([]int, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(strings.TrimSpace(p))
		if err != nil || n < 16 || n > 4096 {
			return nil
		}
		out = append(out, n)
	}
	return out
}

func formatSizes(sizes []int) string {
	parts := make([]string, len(sizes))
	for i, s := range sizes {
		parts[i] = strconv.Itoa(s)
	}
	return strings.Join(parts, ",")
}
