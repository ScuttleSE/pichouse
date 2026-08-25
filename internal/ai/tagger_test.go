package ai

import "testing"

func TestParseTags(t *testing.T) {
	cases := []struct {
		in   string
		want []string
	}{
		{"Here are the tags: beach, Sunset, dog", []string{"beach", "sunset", "dog"}},
		{"1. sand\n2. ocean\n- palm tree", []string{"sand", "ocean", "palm tree"}},
		{"beach, beach, BEACH", []string{"beach"}},
		{"\"blue sky\", 'green grass'", []string{"blue sky", "green grass"}},
		{"a, this image shows a cat, dog", []string{"dog"}}, // reject prose + single letter
	}
	for _, c := range cases {
		got := ParseTags(c.in, 25)
		if len(got) != len(c.want) {
			t.Fatalf("ParseTags(%q) = %v, want %v", c.in, got, c.want)
		}
		for i := range got {
			if got[i] != c.want[i] {
				t.Fatalf("ParseTags(%q) = %v, want %v", c.in, got, c.want)
			}
		}
	}
}

func TestParseTagsCap(t *testing.T) {
	got := ParseTags("a1,b2,c3,d4,e5", 3)
	if len(got) != 3 {
		t.Fatalf("cap not applied: %v", got)
	}
}

func TestConfigNormalize(t *testing.T) {
	c := Config{}
	c.Normalize()
	if c.Host != DefaultHost || c.Port != DefaultPort || c.Model != DefaultModel {
		t.Fatalf("defaults not filled: %+v", c)
	}
	if c.Concurrency < 1 || c.MaxTags < 1 || c.MaxSide < 1 {
		t.Fatalf("clamps not applied: %+v", c)
	}
}
