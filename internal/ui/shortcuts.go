package ui

import (
	"github.com/diamondburned/gotk4/pkg/gdk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// shortcutAction identifies a configurable viewer action.
type shortcutAction string

const (
	actionPrev   shortcutAction = "prev"
	actionNext   shortcutAction = "next"
	actionRotate shortcutAction = "rotate"
	actionClose  shortcutAction = "close"
)

// shortcutDef describes a configurable action: its stable id, the label shown
// in settings, and its default GDK keyval.
type shortcutDef struct {
	action shortcutAction
	label  string
	defKey uint
}

// shortcutDefs lists every configurable viewer shortcut, in display order.
var shortcutDefs = []shortcutDef{
	{actionPrev, "Previous image", gdk.KEY_Left},
	{actionNext, "Next image", gdk.KEY_Right},
	{actionRotate, "Rotate 90°", gdk.KEY_r},
	{actionClose, "Close viewer", gdk.KEY_Escape},
}

// shortcutSettingKey returns the library.db settings key for an action.
func shortcutSettingKey(a shortcutAction) string {
	return "keybind." + string(a)
}

// shortcuts maps a GDK keyval to the action it triggers.
type shortcuts struct {
	byKey map[uint]shortcutAction // keyval -> action
	keys  map[shortcutAction]uint // action -> keyval
}

// loadShortcuts builds the keybinding table from the database, filling defaults.
func loadShortcuts(lib *db.Library) *shortcuts {
	s := &shortcuts{
		byKey: map[uint]shortcutAction{},
		keys:  map[shortcutAction]uint{},
	}
	for _, d := range shortcutDefs {
		key := d.defKey
		if v, _ := lib.GetSetting(shortcutSettingKey(d.action), ""); v != "" {
			if kv := gdk.KeyvalFromName(v); kv != 0 {
				key = kv
			}
		}
		s.set(d.action, key)
	}
	return s
}

// set assigns keyval to action, replacing any previous binding for that action.
func (s *shortcuts) set(action shortcutAction, keyval uint) {
	if old, ok := s.keys[action]; ok {
		delete(s.byKey, old)
	}
	s.keys[action] = keyval
	s.byKey[keyval] = action
}

// action returns the action bound to a keyval, normalizing case for letters so
// that, e.g., 'r' and 'R' (Shift) match the same binding.
func (s *shortcuts) action(keyval uint) (shortcutAction, bool) {
	if a, ok := s.byKey[keyval]; ok {
		return a, true
	}
	lower := gdk.KeyvalToLower(keyval)
	if lower != keyval {
		if a, ok := s.byKey[lower]; ok {
			return a, true
		}
	}
	return "", false
}

// keyval returns the keyval currently bound to an action.
func (s *shortcuts) keyval(action shortcutAction) uint {
	return s.keys[action]
}

// label returns a human-readable name for a keyval.
func keyvalLabel(keyval uint) string {
	if keyval == 0 {
		return "(unset)"
	}
	if name := gdk.KeyvalName(keyval); name != "" {
		return name
	}
	return "?"
}
