//! A small LRU cache of decoded thumbnail textures, keyed by the cell key
//! (`hash|size|orientation`). It lives on the UI thread and lets the grid skip
//! re-decoding JPEG blobs when scrolling or re-entering a folder.

use std::collections::HashMap;

use gtk4::gdk;

/// A bounded LRU cache of `gdk::Texture` values.
pub struct TextureCache {
    capacity: usize,
    map: HashMap<String, gdk::Texture>,
    /// Keys in least-recently-used-first order.
    order: Vec<String>,
}

impl TextureCache {
    /// Create a cache holding at most `capacity` textures.
    pub fn new(capacity: usize) -> TextureCache {
        TextureCache {
            capacity,
            map: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Get a texture and mark it most-recently-used.
    pub fn get(&mut self, key: &str) -> Option<gdk::Texture> {
        if let Some(t) = self.map.get(key).cloned() {
            self.touch(key);
            Some(t)
        } else {
            None
        }
    }

    /// Insert (or replace) a texture, evicting the least-recently-used entry
    /// when over capacity.
    pub fn put(&mut self, key: String, texture: gdk::Texture) {
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), texture);
            self.touch(&key);
            return;
        }
        self.map.insert(key.clone(), texture);
        self.order.push(key);
        while self.order.len() > self.capacity {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }

    /// Drop all entries.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(pos);
            self.order.push(k);
        }
    }
}
