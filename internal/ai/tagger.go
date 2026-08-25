package ai

import (
	"regexp"
	"strings"
)

// ParseTags turns a model's free-text keyword response into a clean, deduped,
// lowercase tag list, capped at maxTags. It tolerates commas, newlines,
// bullets, numbering, and surrounding prose.
func ParseTags(response string, maxTags int) []string {
	if maxTags <= 0 {
		maxTags = DefaultMaxTags
	}
	// Split on commas and newlines.
	fields := strings.FieldsFunc(response, func(r rune) bool {
		return r == ',' || r == '\n' || r == ';'
	})
	seen := map[string]bool{}
	var out []string
	for _, f := range fields {
		t := cleanTag(f)
		if t == "" || seen[t] {
			continue
		}
		seen[t] = true
		out = append(out, t)
		if len(out) >= maxTags {
			break
		}
	}
	return out
}

var (
	leadingJunk = regexp.MustCompile(`^[\s\-\*\d\.\)\(#>]+`)
	badChars    = regexp.MustCompile(`["'` + "`" + `]`)
)

// cleanTag normalizes a single candidate keyword. Returns "" to reject it.
func cleanTag(s string) string {
	s = strings.ToLower(strings.TrimSpace(s))
	s = leadingJunk.ReplaceAllString(s, "")
	s = badChars.ReplaceAllString(s, "")
	// Strip a leading "label:" prose prefix (e.g. "here are the tags: beach").
	if i := strings.LastIndex(s, ":"); i >= 0 && i < len(s)-1 {
		s = s[i+1:]
	}
	s = strings.TrimSpace(s)
	// Drop trailing punctuation.
	s = strings.TrimRight(s, ".!?:;")
	s = strings.TrimSpace(s)
	if s == "" {
		return ""
	}
	// Reject overly long fragments (likely a sentence) and single letters.
	if len(s) < 2 || len(s) > 40 {
		return ""
	}
	if strings.Count(s, " ") > 3 {
		return ""
	}
	// Reject common non-tag prose openers that survived prefix stripping.
	for _, junk := range []string{"here are", "the image", "this image", "sure", "keywords", "tags"} {
		if s == junk || strings.HasPrefix(s, junk+" ") {
			return ""
		}
	}
	return s
}
