package ui

import (
	"container/list"
	"sync"
)

// thumbCache is a simple thread-safe LRU cache of encoded JPEG thumbnail bytes,
// keyed by the photo content hash (or path when no hash is available). It avoids
// re-decoding/re-fetching thumbnails while scrolling and when re-entering a
// folder.
type thumbCache struct {
	mu    sync.Mutex
	max   int
	ll    *list.List
	items map[string]*list.Element
}

type cacheEntry struct {
	key  string
	blob []byte
}

func newThumbCache(max int) *thumbCache {
	if max < 1 {
		max = 1
	}
	return &thumbCache{
		max:   max,
		ll:    list.New(),
		items: make(map[string]*list.Element),
	}
}

// Get returns the cached blob for key and marks it most-recently-used.
func (c *thumbCache) Get(key string) ([]byte, bool) {
	if key == "" {
		return nil, false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if el, ok := c.items[key]; ok {
		c.ll.MoveToFront(el)
		return el.Value.(*cacheEntry).blob, true
	}
	return nil, false
}

// Put stores blob under key, evicting the least-recently-used entry if needed.
func (c *thumbCache) Put(key string, blob []byte) {
	if key == "" {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if el, ok := c.items[key]; ok {
		c.ll.MoveToFront(el)
		el.Value.(*cacheEntry).blob = blob
		return
	}
	el := c.ll.PushFront(&cacheEntry{key: key, blob: blob})
	c.items[key] = el
	for c.ll.Len() > c.max {
		old := c.ll.Back()
		if old == nil {
			break
		}
		c.ll.Remove(old)
		delete(c.items, old.Value.(*cacheEntry).key)
	}
}
