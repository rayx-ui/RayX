use std::collections::BTreeMap;
use std::sync::Arc;

/// Bytes of loaded assets within a byte budget, the least recently used going first.
#[derive(Clone, Debug)]
pub struct AssetByteCache {
    max_bytes: u64,
    current_bytes: u64,
    tick: u64,
    entries: BTreeMap<String, (Arc<[u8]>, u64)>,
}

impl AssetByteCache {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            current_bytes: 0,
            tick: 0,
            entries: BTreeMap::new(),
        }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn current_bytes(&self) -> u64 {
        self.current_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<[u8]>> {
        self.tick += 1;
        let tick = self.tick;
        let (bytes, last_access) = self.entries.get_mut(key)?;
        *last_access = tick;
        Some(Arc::clone(bytes))
    }

    /// Keeps `bytes` at `key`, unless they alone exceed the budget; returns whether it did.
    pub fn insert(&mut self, key: impl Into<String>, bytes: Arc<[u8]>) -> bool {
        let size = bytes.len() as u64;
        if size > self.max_bytes {
            return false;
        }
        let key = key.into();
        self.remove(&key);
        self.tick += 1;
        self.current_bytes += size;
        self.entries.insert(key, (bytes, self.tick));
        self.evict_until_within_budget();
        true
    }

    pub fn remove(&mut self, key: &str) -> Option<Arc<[u8]>> {
        let (bytes, _) = self.entries.remove(key)?;
        self.current_bytes = self.current_bytes.saturating_sub(bytes.len() as u64);
        Some(bytes)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_bytes = 0;
    }

    fn evict_until_within_budget(&mut self) {
        while self.current_bytes > self.max_bytes {
            let Some(victim) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, last_access))| *last_access)
                .map(|(key, _)| key.clone())
            else {
                self.current_bytes = 0;
                return;
            };
            self.remove(&victim);
        }
    }
}
