package ai

import (
	"context"
	"errors"
	"log"
	"os/exec"
	"time"
)

// Manager optionally launches and supervises a local `ollama serve` process.
// It is safe to use the zero value; call EnsureRunning before inference.
type Manager struct {
	cmd *exec.Cmd
}

// EnsureRunning makes a best effort to guarantee a reachable Ollama server.
// If the client already detects one, it does nothing. Otherwise, when cfg.Manage
// is set and an ollama binary is found, it starts `ollama serve` and waits for
// readiness. It returns an error only when a server could not be made available.
func (m *Manager) EnsureRunning(ctx context.Context, cfg Config, c *Client) error {
	if ok, _, _ := c.Detect(ctx); ok {
		log.Printf("[ai] detected running server at %s", c.baseURL)
		return nil
	}
	if !cfg.Manage {
		return errors.New("no local AI server detected at " + c.baseURL +
			" (start Ollama, or enable managed mode in settings)")
	}
	bin := cfg.BinaryPath
	if bin == "" {
		p, err := exec.LookPath("ollama")
		if err != nil {
			return errors.New("ollama binary not found in PATH; install Ollama or set its path in settings")
		}
		bin = p
	}
	log.Printf("[ai] no server found; launching %s serve", bin)
	cmd := exec.Command(bin, "serve")
	if err := cmd.Start(); err != nil {
		return err
	}
	m.cmd = cmd

	// Wait up to ~15s for readiness.
	deadline := time.Now().Add(15 * time.Second)
	for time.Now().Before(deadline) {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if ok, _, _ := c.Detect(ctx); ok {
			log.Printf("[ai] managed server is ready")
			return nil
		}
		time.Sleep(500 * time.Millisecond)
	}
	return errors.New("started ollama serve but it did not become ready in time")
}

// Stop terminates a managed server process, if one was started.
func (m *Manager) Stop() {
	if m.cmd != nil && m.cmd.Process != nil {
		_ = m.cmd.Process.Kill()
		_, _ = m.cmd.Process.Wait()
		m.cmd = nil
	}
}
