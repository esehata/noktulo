use log::warn;

use super::Key;
use std::collections::hash_map::Iter;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Store {
    key_len: usize,
    store: HashMap<Key, Vec<u8>>,
    store_predicate: Arc<dyn Fn(&[u8]) -> bool + Sync + Send>,
}

impl Store {
    pub fn new(key_len: usize, store_predicate: Arc<dyn Fn(&[u8]) -> bool + Sync + Send>) -> Store {
        Store {
            key_len,
            store: HashMap::new(),
            store_predicate,
        }
    }

    pub fn insert(&mut self, k: Key, v: Vec<u8>) -> Result<(), &'static str> {
        assert_eq!(self.key_len,k.len());
        if (self.store_predicate)(&v) {
            self.store.insert(k, v);
            Ok(())
        } else {
            warn!("Invalid value is tried to insert.");
            Err("Invalid value is tried to insert.")
        }
    }

    pub fn get(&self, k: &Key) -> Option<&Vec<u8>> {
        assert_eq!(self.key_len,k.len());
        self.store.get(k)
    }

    pub fn iter(&self) -> Iter<Key, Vec<u8>> {
        self.store.iter()
    }
}
