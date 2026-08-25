package db

import (
	"os"
	"path/filepath"
	"strings"
)

// configPath returns the path to the pichouse config file
// (~/.config/pichouse/config), honoring XDG_CONFIG_HOME. The config file's sole
// purpose is to record where the database files are stored, so the user can
// relocate the data directory. All other settings live in library.db.
func configPath() (string, error) {
	var base string
	if dir, ok := os.LookupEnv("XDG_CONFIG_HOME"); ok && dir != "" {
		base = dir
	} else {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		base = filepath.Join(home, ".config")
	}
	return filepath.Join(base, "pichouse", "config"), nil
}

// defaultDataDir returns the built-in default data directory
// (~/.local/share/pichouse), honoring XDG_DATA_HOME.
func defaultDataDir() (string, error) {
	var base string
	if dir, ok := os.LookupEnv("XDG_DATA_HOME"); ok && dir != "" {
		base = dir
	} else {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		base = filepath.Join(home, ".local", "share")
	}
	return filepath.Join(base, "pichouse"), nil
}

// readConfiguredDataDir reads the data directory path from the config file. It
// returns ("", nil) when the config file does not exist.
func readConfiguredDataDir() (string, error) {
	cp, err := configPath()
	if err != nil {
		return "", err
	}
	data, err := os.ReadFile(cp)
	if err != nil {
		if os.IsNotExist(err) {
			return "", nil
		}
		return "", err
	}
	return strings.TrimSpace(string(data)), nil
}

// WriteConfiguredDataDir persists the data directory path to the config file,
// creating the config directory if necessary.
func WriteConfiguredDataDir(dir string) error {
	cp, err := configPath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(cp), 0o755); err != nil {
		return err
	}
	return os.WriteFile(cp, []byte(strings.TrimSpace(dir)+"\n"), 0o644)
}

// DataDir returns the pichouse data directory, creating it if necessary. If the
// config file specifies a directory it is used; otherwise the default
// (~/.local/share/pichouse) is used.
func DataDir() (string, error) {
	dir, err := readConfiguredDataDir()
	if err != nil {
		return "", err
	}
	if dir == "" {
		dir, err = defaultDataDir()
		if err != nil {
			return "", err
		}
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}
