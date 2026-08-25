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

// Generate runs a single vision inference: it sends the image bytes and prompt
// to the model and returns the raw text response.
func (c *Client) Generate(ctx context.Context, model, prompt string, image []byte) (string, error) {
	reqBody := map[string]any{
		"model":  model,
		"prompt": prompt,
		"images": []string{base64.StdEncoding.EncodeToString(image)},
		"stream": false,
	}
	buf, err := json.Marshal(reqBody)
	if err != nil {
		return "", err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+"/api/generate", bytes.NewReader(buf))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("ollama /api/generate: status %d", resp.StatusCode)
	}
	var out struct {
		Response string `json:"response"`
		Error    string `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", err
	}
	if out.Error != "" {
		return "", fmt.Errorf("ollama: %s", out.Error)
	}
	return out.Response, nil
}
