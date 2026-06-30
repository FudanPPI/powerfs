use std::collections::VecDeque;
use std::sync::RwLock;
use log::{debug, warn};

pub struct MasterRaftStorage {
    entries: RwLock<VecDeque<Vec<u8>>>,
    applied_index: RwLock<u64>,
}

impl MasterRaftStorage {
    pub fn new() -> Self {
        MasterRaftStorage {
            entries: RwLock::new(VecDeque::new()),
            applied_index: RwLock::new(0),
        }
    }

    pub fn append(&self, entry: &[u8]) {
        let mut entries = self.entries.write().unwrap();
        entries.push_back(entry.to_vec());
        debug!("Appended entry, total: {}", entries.len());
    }

    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }
}
