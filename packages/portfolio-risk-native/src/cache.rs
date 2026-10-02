//! Bounded immutable market-stage reuse. Keys retain complete input bytes, so
//! hash collisions cannot return another calculation. No instrument results live
//! here. Computation happens outside the lock; only successful values publish.
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
pub(crate) struct Key(Vec<u8>);
impl Key {
    pub fn new(stage: u64) -> Self {
        Self(stage.to_le_bytes().to_vec())
    }
    pub fn floats(mut self, values: &[f64]) -> Self {
        self.0
            .extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            self.0.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        self
    }
    pub fn integers(mut self, values: &[usize]) -> Self {
        self.0
            .extend_from_slice(&(values.len() as u64).to_le_bytes());
        for value in values {
            self.0.extend_from_slice(&(*value as u64).to_le_bytes());
        }
        self
    }
}

struct Entry {
    value: Arc<dyn Any + Send + Sync>,
    bytes: usize,
    touched: u64,
}
struct Store {
    entries: HashMap<Vec<u8>, Entry>,
    bytes: usize,
    limit: usize,
    clock: u64,
    generation: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}
impl Store {
    fn new(limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            limit,
            clock: 0,
            generation: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
    fn get<T: Any + Send + Sync>(&mut self, key: &Key) -> Option<Arc<T>> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&key.0) {
            if let Ok(value) = entry.value.clone().downcast::<T>() {
                entry.touched = self.clock;
                self.hits += 1;
                return Some(value);
            }
        }
        self.misses += 1;
        None
    }
    fn insert<T: Any + Send + Sync>(
        &mut self,
        key: Key,
        value: Arc<T>,
        value_bytes: usize,
        generation: u64,
    ) -> Arc<T> {
        // Clear is also an admission barrier for work already in flight.
        if generation != self.generation {
            return value;
        }
        let Some(bytes) = value_bytes
            .checked_add(key.0.capacity())
            .and_then(|v| v.checked_add(128))
        else {
            return value;
        };
        if bytes > self.limit {
            return value;
        }
        if let Some(entry) = self.entries.get(&key.0) {
            if let Ok(existing) = entry.value.clone().downcast::<T>() {
                return existing;
            }
        }
        while self.bytes + bytes > self.limit || self.entries.len() >= 512 {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone());
            let Some(oldest) = oldest else {
                break;
            };
            self.bytes -= self.entries.remove(&oldest).unwrap().bytes;
            self.evictions += 1;
        }
        self.clock += 1;
        if let Some(previous) = self.entries.insert(
            key.0,
            Entry {
                value: value.clone(),
                bytes,
                touched: self.clock,
            },
        ) {
            self.bytes -= previous.bytes;
        }
        self.bytes += bytes;
        value
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.generation += 1;
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
    }
}
static CACHE: OnceLock<Mutex<Store>> = OnceLock::new();
fn store() -> &'static Mutex<Store> {
    CACHE.get_or_init(|| Mutex::new(Store::new(128 * 1024 * 1024)))
}

pub(crate) fn resolve<T: Any + Send + Sync>(
    key: Key,
    compute: impl FnOnce() -> Result<(T, usize), String>,
) -> Result<Arc<T>, String> {
    let generation = {
        let mut cache = store().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = cache.get(&key) {
            return Ok(value);
        }
        cache.generation
    };
    let (value, bytes) = compute()?;
    Ok(store().lock().unwrap_or_else(|e| e.into_inner()).insert(
        key,
        Arc::new(value),
        bytes,
        generation,
    ))
}

/// Component retention statistics; active borrowers and FFI copies are additional.
pub fn statistics(clear: bool) -> [f64; 5] {
    let mut cache = store().lock().unwrap_or_else(|e| e.into_inner());
    if clear {
        cache.clear();
    }
    [
        cache.entries.len() as f64,
        cache.bytes as f64,
        cache.hits as f64,
        cache.misses as f64,
        cache.evictions as f64,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_identity_lru_admission_and_active_borrowers() {
        let mut cache = Store::new(340);
        let first = cache.insert(Key::new(1), Arc::new(vec![1.]), 8, 0);
        cache.insert(Key::new(2), Arc::new(vec![2.]), 8, 0);
        assert_eq!(*cache.get::<Vec<f64>>(&Key::new(1)).unwrap(), vec![1.]);
        cache.insert(Key::new(3), Arc::new(vec![3.]), 8, 0);
        assert!(cache.get::<Vec<f64>>(&Key::new(2)).is_none());
        assert_eq!(*first, vec![1.]);
        assert!(cache.bytes <= cache.limit);
        let count = cache.entries.len();
        cache.insert(Key::new(4), Arc::new(vec![4.]), 1000, 0);
        assert_eq!(cache.entries.len(), count);
        cache.clear();
        cache.insert(Key::new(5), Arc::new(vec![5.]), 8, 0);
        assert!(cache.entries.is_empty());
        assert_eq!(*first, vec![1.]);
        assert_ne!(
            Key::new(1).floats(&[1., 2.]).0,
            Key::new(1).floats(&[1.]).floats(&[2.]).0
        );
        assert_ne!(Key::new(1).floats(&[0.]).0, Key::new(1).floats(&[-0.]).0);
    }
    #[test]
    fn failures_never_publish() {
        let key = Key::new(9999);
        assert!(resolve::<Vec<f64>>(key, || Err("fit failed".into())).is_err());
        let value = resolve(Key::new(9999), || Ok((vec![9.], 8))).unwrap();
        assert_eq!(*value, vec![9.]);
    }
}
