package ai

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"time"
)

// Client talks to a local Ollama HTTP server.
type Client struct {
	baseURL string
	http    *http.Client
}

// NewClient builds a client for the given host and port.
func NewClient(host string, port int) *Client {
	if host == "" {
		host = DefaultHost
	}
	if port == 0 {
		port = DefaultPort
	}
	return &Client{
		baseURL: "http://" + host + ":" + strconv.Itoa(port),
		http:    &http.Client{Timeout: httpTimeout},
	}
}

// Detect reports whether the server is reachable and lists installed models.
func (c *Client) Detect(ctx context.Context) (ok bool, models []string, err error) {
	ctx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+"/api/tags", nil)
	if err != nil {
		return false, nil, err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return false, nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return false, nil, fmt.Errorf("ollama /api/tags: status %d", resp.StatusCode)
	}
	var body struct {
		Models []struct {
			Name string `json:"name"`
		} `json:"models"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		return true, nil, err
	}
	for _, m := range body.Models {
		models = append(models, m.Name)
	}
	return true, models, nil
}

// GenOptions holds per-request tuning passed to Ollama.
type GenOptions struct {
	NumThread int    // options.num_thread; 0 = auto
	NumCtx    int    // options.num_ctx; 0 = model default. Smaller = less CPU prefill.
	KeepAlive string // keep_alive, e.g. "10m"; "" = server default
}

// GenResult is the model text plus Ollama's timing breakdown (nanoseconds).
type GenResult struct {
	Response           string
	TotalDuration      int64
	LoadDuration       int64
	PromptEvalDuration int64
	EvalDuration       int64
}

// Generate runs a single vision inference: it sends the image bytes and prompt
// to the model and returns the response text plus timing details.
func (c *Client) Generate(ctx context.Context, model, prompt string, image []byte, opt GenOptions) (GenResult, error) {
	options := map[string]any{}
	if opt.NumThread > 0 {
		options["num_thread"] = opt.NumThread
	}
	if opt.NumCtx > 0 {
		options["num_ctx"] = opt.NumCtx
	}
	reqBody := map[string]any{
		"model":  model,
		"prompt": prompt,
		"images": []string{base64.StdEncoding.EncodeToString(image)},
		"stream": false,
	}
	if len(options) > 0 {
		reqBody["options"] = options
	}
	if opt.KeepAlive != "" {
		reqBody["keep_alive"] = opt.KeepAlive
	}
	buf, err := json.Marshal(reqBody)
	if err != nil {
		return GenResult{}, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+"/api/generate", bytes.NewReader(buf))
	if err != nil {
		return GenResult{}, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return GenResult{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return GenResult{}, fmt.Errorf("ollama /api/generate: status %d", resp.StatusCode)
	}
	var out struct {
		Response           string `json:"response"`
		Error              string `json:"error"`
		TotalDuration      int64  `json:"total_duration"`
		LoadDuration       int64  `json:"load_duration"`
		PromptEvalDuration int64  `json:"prompt_eval_duration"`
		EvalDuration       int64  `json:"eval_duration"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return GenResult{}, err
	}
	if out.Error != "" {
		return GenResult{}, fmt.Errorf("ollama: %s", out.Error)
	}
	return GenResult{
		Response:           out.Response,
		TotalDuration:      out.TotalDuration,
		LoadDuration:       out.LoadDuration,
		PromptEvalDuration: out.PromptEvalDuration,
		EvalDuration:       out.EvalDuration,
	}, nil
}
