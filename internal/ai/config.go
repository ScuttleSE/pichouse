// Package ai provides local, HTTP-based image-to-text tagging via an Ollama
// server running on the user's machine. It never downloads models and never
// sends data off the machine; all inference is local.
package ai

import "time"

// Default connection settings.
const (
	DefaultHost  = "127.0.0.1"
	DefaultPort  = 11434
	DefaultModel = "moondream"
	// DefaultMaxSide is the longest image side (px) sent to the model. Vision
	// models downscale internally; a compact image keeps requests fast.
	DefaultMaxSide = 768
	// DefaultConcurrency bounds parallel inference. For a single local GPU,
	// serial requests are best: parallel vision requests make Ollama spin up a
	// second model runner or do CPU-side prefill/image-embedding in parallel,
	// which pins the CPU without improving throughput. Default to 1.
	DefaultConcurrency = 1
	// DefaultMaxTags caps how many keywords are kept per image.
	DefaultMaxTags = 25
	// DefaultKeepAlive keeps the model resident between requests so it is not
	// reloaded (which causes CPU spikes) during a batch.
	DefaultKeepAlive = "10m"
)

// DefaultPrompt asks the model for a plain comma-separated keyword list.
const DefaultPrompt = "List the main visual keywords for this image as a " +
	"comma-separated list of short lowercase tags (objects, people, scene, " +
	"setting, colors, mood). Do not write sentences. Output only the tags."

// Config holds the AI tagging settings.
type Config struct {
	Enabled     bool
	Host        string
	Port        int
	Model       string
	Prompt      string
	MaxSide     int
	MaxTags     int
	Concurrency int
	// NumThread caps CPU threads Ollama uses for the parts of inference that
	// run on CPU (notably vision image-embedding). 0 lets Ollama decide.
	NumThread int
	// NumCtx sets the context window (options.num_ctx). Smaller windows reduce
	// CPU-side prompt prefill cost. 0 uses the model default.
	NumCtx int
	// KeepAlive keeps the model resident between requests (e.g. "10m") to avoid
	// reload CPU spikes mid-batch.
	KeepAlive string
	// Manage tells pichouse to launch a local `ollama serve` subprocess when no
	// server is already running.
	Manage bool
	// BinaryPath overrides the ollama binary location (empty = search PATH).
	BinaryPath string
}

// DefaultConfig returns a disabled configuration with sensible defaults.
func DefaultConfig() Config {
	return Config{
		Enabled:     false,
		Host:        DefaultHost,
		Port:        DefaultPort,
		Model:       DefaultModel,
		Prompt:      DefaultPrompt,
		MaxSide:     DefaultMaxSide,
		MaxTags:     DefaultMaxTags,
		Concurrency: DefaultConcurrency,
		NumThread:   0,
		NumCtx:      0,
		KeepAlive:   DefaultKeepAlive,
		Manage:      false,
	}
}

// Normalize fills zero fields with defaults and clamps ranges.
func (c *Config) Normalize() {
	if c.Host == "" {
		c.Host = DefaultHost
	}
	if c.Port == 0 {
		c.Port = DefaultPort
	}
	if c.Model == "" {
		c.Model = DefaultModel
	}
	if c.Prompt == "" {
		c.Prompt = DefaultPrompt
	}
	if c.MaxSide <= 0 {
		c.MaxSide = DefaultMaxSide
	}
	if c.MaxTags <= 0 {
		c.MaxTags = DefaultMaxTags
	}
	if c.Concurrency <= 0 {
		c.Concurrency = DefaultConcurrency
	}
	if c.KeepAlive == "" {
		c.KeepAlive = DefaultKeepAlive
	}
	if c.NumThread < 0 {
		c.NumThread = 0
	}
	if c.NumCtx < 0 {
		c.NumCtx = 0
	}
}

// httpTimeout is the per-request ceiling for a single inference.
const httpTimeout = 5 * time.Minute
