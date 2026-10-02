//! Request-owned immutable dependency nodes. Hashes accelerate lookup; equality
//! retains complete inputs and parents. Recency updates and eviction are O(log N).
use serde::Serialize;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Eq)]
pub(crate) struct Key {
    stage: String,
    data: Vec<u8>,
    parents: Vec<Arc<Key>>,
    hash: u64,
    bytes: usize,
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
            && self.stage == other.stage
            && self.data == other.data
            && self.parents == other.parents
    }
}
impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}
impl Key {
    pub fn new(
        stage: &str,
        data: impl Serialize,
        parents: Vec<Arc<Key>>,
    ) -> Result<Arc<Self>, String> {
        let data = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        stage.hash(&mut hasher);
        data.hash(&mut hasher);
        parents.hash(&mut hasher);
        let bytes = data.capacity()
            + stage.len()
            + 192
            + parents.capacity() * std::mem::size_of::<Arc<Key>>();
        Ok(Arc::new(Self {
            stage: stage.into(),
            data,
            parents,
            hash: hasher.finish(),
            bytes,
        }))
    }
}
struct Entry {
    value: Arc<dyn Any + Send + Sync>,
    bytes: usize,
    tick: u64,
    protected: bool,
}
pub(crate) struct Cache {
    entries: HashMap<Arc<Key>, Entry>,
    order: BTreeMap<u64, Arc<Key>>,
    protected_order: BTreeMap<u64, Arc<Key>>,
    identities: HashMap<Arc<Key>, usize>,
    tick: u64,
    bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}
#[derive(Default, Serialize)]
pub(crate) struct Counts {
    reused: usize,
    computed: usize,
    batches: usize,
}
pub(crate) type Stats = BTreeMap<String, Counts>;
impl Cache {
    pub fn new(max_bytes: usize, max_entries: usize) -> Result<Self, String> {
        if max_bytes > 2 * 1024 * 1024 * 1024 || max_entries > 2_000_000 {
            return Err("native graph cache exceeds admission limit".into());
        }
        Ok(Self {
            entries: HashMap::new(),
            order: BTreeMap::new(),
            protected_order: BTreeMap::new(),
            identities: HashMap::new(),
            tick: 0,
            bytes: 0,
            max_bytes,
            max_entries,
        })
    }
    pub fn info(&self) -> serde_json::Value {
        serde_json::json!({"entries":self.entries.len(),"bytes":self.bytes,"max_bytes":self.max_bytes,
            "max_entries":self.max_entries,"shared_entries":0,"shared_bytes":0,
            "shared_max_bytes":0,"shared_max_entries":0,"compact_entries":self.protected_order.len(),
            "transient_entries":self.order.len(),"identity_entries":self.identities.len()})
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.protected_order.clear();
        self.identities.clear();
        self.bytes = 0;
    }
    fn touch(&mut self) -> u64 {
        // No wrapping generation may alias a recency entry.
        if self.tick == u64::MAX {
            self.clear();
            self.tick = 0;
        }
        self.tick += 1;
        self.tick
    }
    pub fn get<T: Any + Send + Sync>(&mut self, key: &Arc<Key>) -> Option<Arc<T>> {
        let tick = self.touch();
        let key = self.entries.get_key_value(key)?.0.clone();
        let entry = self.entries.get_mut(&key)?;
        let value = entry.value.clone().downcast::<T>().ok()?;
        let order = if entry.protected {
            &mut self.protected_order
        } else {
            &mut self.order
        };
        order.remove(&entry.tick);
        entry.tick = tick;
        order.insert(tick, key.clone());
        Some(value)
    }
    pub fn put<T: Any + Send + Sync>(&mut self, key: Arc<Key>, value: Arc<T>, size: usize) {
        let Some(size) = size.checked_add(256) else {
            return;
        };
        // Admission includes the full transitive identity, but shared market
        // inputs are retained once rather than duplicated per instrument node.
        if self.max_entries == 0 || size + self.additional(&key) > self.max_bytes {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
            if old.protected {
                self.protected_order.remove(&old.tick);
            } else {
                self.order.remove(&old.tick);
            }
            self.release(&key);
        }
        let protected = key.stage == "result";
        while self.bytes + size + self.additional(&key) > self.max_bytes
            || self.entries.len() >= self.max_entries
        {
            let oldest = if let Some((_, k)) = self.order.pop_first() {
                k
            } else if protected {
                // Eviction can release identities shared with the incoming node,
                // increasing its admission cost. An empty cache may still be too
                // small; the caller retains the computed value even if uncached.
                let Some((_, key)) = self.protected_order.pop_first() else {
                    return;
                };
                key
            } else {
                return;
            };
            self.bytes -= self
                .entries
                .remove(&oldest)
                .expect("cache entry invariant")
                .bytes;
            self.release(&oldest);
        }
        let tick = self.touch();
        let key = self.retain(key);
        if protected {
            self.protected_order.insert(tick, key.clone());
        } else {
            self.order.insert(tick, key.clone());
        }
        self.entries.insert(
            key,
            Entry {
                value,
                bytes: size,
                tick,
                protected,
            },
        );
        self.bytes += size;
    }
    fn additional(&self, key: &Arc<Key>) -> usize {
        if self.identities.contains_key(key) {
            0
        } else {
            key.bytes
                + key
                    .parents
                    .iter()
                    .map(|p| self.additional(p))
                    .sum::<usize>()
        }
    }
    fn retain(&mut self, key: Arc<Key>) -> Arc<Key> {
        if let Some((canonical, _)) = self.identities.get_key_value(&key) {
            let canonical = canonical.clone();
            *self.identities.get_mut(&canonical).unwrap() += 1;
            return canonical;
        }
        let mut owned = (*key).clone();
        owned.parents = owned.parents.into_iter().map(|p| self.retain(p)).collect();
        let canonical = Arc::new(owned);
        self.bytes += canonical.bytes;
        self.identities.insert(canonical.clone(), 1);
        canonical
    }
    fn release(&mut self, key: &Arc<Key>) {
        let references = self
            .identities
            .get_mut(key)
            .expect("identity reference invariant");
        *references -= 1;
        if *references == 0 {
            let (key, _) = self.identities.remove_entry(key).unwrap();
            self.bytes -= key.bytes;
            for parent in &key.parents {
                self.release(parent);
            }
        }
    }
    pub fn batch<T: Any + Send + Sync>(
        &mut self,
        stage: &str,
        keys: &[Arc<Key>],
        stats: &mut Stats,
        build: impl FnOnce(&[usize]) -> Result<Vec<T>, String>,
        size: impl Fn(&T) -> usize,
    ) -> Result<Vec<Arc<T>>, String> {
        let counts = stats.entry(stage.into()).or_default();
        let mut out: Vec<_> = keys.iter().map(|k| self.get::<T>(k)).collect();
        let missing: Vec<_> = out
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.is_none().then_some(i))
            .collect();
        counts.reused += keys.len() - missing.len();
        if !missing.is_empty() {
            let values = build(&missing)?;
            if values.len() != missing.len() {
                return Err("native graph batch result shape mismatch".into());
            }
            // Finish the complete batch before admitting any result.
            counts.computed += values.len();
            counts.batches += 1;
            for (i, value) in missing.into_iter().zip(values) {
                let bytes = size(&value);
                let value = Arc::new(value);
                self.put(keys[i].clone(), value.clone(), bytes);
                out[i] = Some(value);
            }
        }
        Ok(out.into_iter().map(Option::unwrap).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_identity_eviction_can_refuse_admission_without_poisoning_cache() {
        let parent = Key::new("market", vec![0.; 200], vec![]).unwrap();
        let key = Key::new("result", 1, vec![parent.clone()]).unwrap();
        let limit = parent.bytes + 256 + 8;
        let mut cache = Cache::new(limit, 8).unwrap();
        cache.put(parent.clone(), Arc::new(1_u64), 8);
        assert_eq!(cache.entries.len(), 1);
        // Initially the parent is shared; after eviction the complete identity
        // cannot fit. Refusal must leave the cache reusable and within budget.
        cache.put(key.clone(), Arc::new(2_u64), 8);
        assert!(cache.get::<u64>(&key).is_none());
        assert!(cache.bytes <= limit);
        cache.put(parent.clone(), Arc::new(3_u64), 8);
        assert_eq!(*cache.get::<u64>(&parent).unwrap(), 3);
    }
    #[test]
    fn compact_results_survive_transient_flow_churn() {
        let mut cache = Cache::new(8192, 10).unwrap();
        let parent = Key::new("market", vec![0.; 200], vec![]).unwrap();
        let key = Key::new("result", 1, vec![parent.clone()]).unwrap();
        cache.put(key.clone(), Arc::new(vec![2.]), 32);
        for i in 0..40 {
            cache.put(
                Key::new("flow", i, vec![parent.clone()]).unwrap(),
                Arc::new(vec![0.; 500]),
                4000,
            );
            assert!(cache.bytes <= cache.max_bytes);
        }
        assert_eq!(cache.get::<Vec<f64>>(&key).unwrap()[0], 2.);
        assert_eq!(cache.protected_order.len(), 1);
        cache.clear();
        assert!(cache.identities.is_empty());
        assert!(cache.protected_order.is_empty());
    }
    #[test]
    fn bounds_recency_failed_batches_and_complete_identity() {
        let mut c = Cache::new(4096, 2).unwrap();
        let mut stats = Stats::new();
        let a = Key::new("flow", [1, 2], vec![]).unwrap();
        let b = Key::new("flow", [2, 1], vec![]).unwrap();
        let retained = c
            .batch(
                "flow",
                &[a.clone(), b.clone()],
                &mut stats,
                |_| Ok(vec![vec![1.], vec![2.]]),
                |_| 8,
            )
            .unwrap();
        c.batch::<Vec<f64>>(
            "flow",
            std::slice::from_ref(&a),
            &mut stats,
            |_| panic!("warm miss"),
            |_| 8,
        )
        .unwrap();
        let d = Key::new("flow", [3, 1], vec![]).unwrap();
        c.batch(
            "flow",
            std::slice::from_ref(&d),
            &mut stats,
            |_| Ok(vec![vec![3.]]),
            |_| 8,
        )
        .unwrap();
        assert!(c.get::<Vec<f64>>(&b).is_none());
        assert!(c.get::<Vec<f64>>(&a).is_some());
        let before = c.info();
        assert!(c
            .batch::<Vec<f64>>("flow", &[b], &mut stats, |_| Err("failed".into()), |_| 8)
            .is_err());
        assert_eq!(c.info(), before);
        assert!(c.bytes <= c.max_bytes);
        c.clear();
        assert_eq!(retained[0][0], 1.);
        assert_eq!(c.bytes, 0);
        let mut collision = Key::new("flow", [9, 9], vec![]).unwrap();
        Arc::get_mut(&mut collision).unwrap().hash = a.hash;
        assert!(a != collision); // Identical hash cannot alias different inputs.
    }
}
