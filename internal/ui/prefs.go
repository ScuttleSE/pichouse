package ui

import (
	"strconv"
	"strings"

	"git.hemmalab.se/scuttle/pichouse/internal/ai"
	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// Setting keys stored in library.db.
const (
	keyThumbSizes   = "thumb.sizes"    // comma-separated 4 preset px values
	keyThumbActive  = "thumb.active"   // active preset index (0..3)
	keyRegenOnMove  = "thumb.regen"    // "1" to regenerate thumbnails when moving slider
	keySaveAllSizes = "thumb.save_all" // "1" to cache all sizes on generation
	keyPropsVisible = "ui.props_visible"

	keyAIEnabled     = "ai.enabled"
	keyAIHost        = "ai.host"
	keyAIPort        = "ai.port"
	keyAIModel       = "ai.model"
	keyAIConcurrency = "ai.concurrency"
	keyAIManage      = "ai.manage"
	keyAIBinary      = "ai.binary"
	keyAIPrompt      = "ai.prompt"
	keyAINumThread   = "ai.num_thread"
	keyAINumCtx      = "ai.num_ctx"
	keyAINumPredict  = "ai.num_predict"
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

// loadAIConfig reads the AI tagging configuration from the library database.
func loadAIConfig(lib *db.Library) ai.Config {
	c := ai.DefaultConfig()
	c.Enabled = boolSetting(lib, keyAIEnabled, false)
	if v, _ := lib.GetSetting(keyAIHost, ""); v != "" {
		c.Host = v
	}
	if v, _ := lib.GetSetting(keyAIPort, ""); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			c.Port = n
		}
	}
	if v, _ := lib.GetSetting(keyAIModel, ""); v != "" {
		c.Model = v
	}
	if v, _ := lib.GetSetting(keyAIConcurrency, ""); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			c.Concurrency = n
		}
	}
	c.Manage = boolSetting(lib, keyAIManage, false)
	if v, _ := lib.GetSetting(keyAIBinary, ""); v != "" {
		c.BinaryPath = v
	}
	if v, _ := lib.GetSetting(keyAIPrompt, ""); v != "" {
		c.Prompt = v
	}
	if v, _ := lib.GetSetting(keyAINumThread, ""); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n >= 0 {
			c.NumThread = n
		}
	}
	if v, _ := lib.GetSetting(keyAINumCtx, ""); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n >= 0 {
			c.NumCtx = n
		}
	}
	if v, _ := lib.GetSetting(keyAINumPredict, ""); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			c.NumPredict = n
		}
	}
	c.Normalize()
	return c
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
