use raft::storage::{Storage, InitialState, Entry, Snapshot, HardState};
use raft::eraftpb::ConfState;
use std::collections::VecDeque;
use std::sync::RwLock;
use log::{debug, warn};

pub struct MasterRaftStorage {
    entries: RwLock<VecDeque<Entry>>,
    snapshot: RwLock<Option<Snapshot>>,
    hard_state: RwLock<Option<HardState>>,
    applied_index: RwLock<u64>,
}

impl MasterRaftStorage {
    pub fn new() -> Self {
        MasterRaftStorage {
            entries: RwLock::new(VecDeque::new()),
            snapshot: RwLock::new(None),
            hard_state: RwLock::new(None),
            applied_index: RwLock::new(0),
        }
    }
}

impl Storage for MasterRaftStorage {
    fn initial_state(&self) -> raft::Result<InitialState> {
        let hard_state = self.hard_state.read().unwrap().clone();
        let conf_state = ConfState {
            nodes: vec![1],
            learners: vec![],
            ..Default::default()
        };
        
        Ok(InitialState {
            hard_state: hard_state.unwrap_or_default(),
            conf_state,
        })
    }

    fn entries(&self, low: u64, high: u64) -> raft::Result<Vec<Entry>> {
        let entries = self.entries.read().unwrap();
        let start = low as usize;
        let end = high as usize;
        
        if start >= entries.len() {
            return Ok(vec![]);
        }
        
        let end = std::cmp::min(end, entries.len());
        let result: Vec<Entry> = entries.range(start..end).cloned().collect();
        
        debug!("Read entries from {} to {}, got {} entries", low, high, result.len());
        
        Ok(result)
    }

    fn term(&self, idx: u64) -> raft::Result<u64> {
        let entries = self.entries.read().unwrap();
        
        if idx as usize >= entries.len() {
            return Err(raft::Error::Store(raft::StorageError::Compacted));
        }
        
        Ok(entries[idx as usize].term)
    }

    fn first_index(&self) -> raft::Result<u64> {
        let entries = self.entries.read().unwrap();
        
        if entries.is_empty() {
            Ok(1)
        } else {
            Ok(entries[0].index)
        }
    }

    fn last_index(&self) -> raft::Result<u64> {
        let entries = self.entries.read().unwrap();
        
        if entries.is_empty() {
            Ok(0)
        } else {
            Ok(entries.back().unwrap().index)
        }
    }

    fn snapshot(&self, _request_index: u64) -> raft::Result<Option<Snapshot>> {
        Ok(self.snapshot.read().unwrap().clone())
    }

    fn set_hard_state(&self, hs: &HardState) -> raft::Result<()> {
        let mut hard_state = self.hard_state.write().unwrap();
        *hard_state = Some(hs.clone());
        debug!("Set hard state: {:?}", hs);
        Ok(())
    }

    fn apply_snapshot(&self, snapshot: &Snapshot) -> raft::Result<()> {
        let mut snap = self.snapshot.write().unwrap();
        *snap = Some(snapshot.clone());
        debug!("Applied snapshot");
        Ok(())
    }

    fn create_snapshot(&self, _index: u64, _cs: &ConfState, _data: &[u8]) -> raft::Result<Snapshot> {
        let snapshot = Snapshot {
            metadata: Some(raft::eraftpb::SnapshotMeta {
                index: _index,
                term: 1,
                conf_state: Some(_cs.clone()),
                ..Default::default()
            }),
            data: _data.to_vec(),
        };
        
        let mut snap = self.snapshot.write().unwrap();
        *snap = Some(snapshot.clone());
        
        Ok(snapshot)
    }

    fn compact(&self, compact_index: u64) -> raft::Result<()> {
        let mut entries = self.entries.write().unwrap();
        
        while let Some(entry) = entries.front() {
            if entry.index <= compact_index {
                entries.pop_front();
            } else {
                break;
            }
        }
        
        let mut applied_index = self.applied_index.write().unwrap();
        *applied_index = compact_index;
        
        debug!("Compacted entries up to index {}", compact_index);
        
        Ok(())
    }

    fn append(&self, entries: &[Entry]) -> raft::Result<()> {
        let mut entries_lock = self.entries.write().unwrap();
        
        for entry in entries {
            entries_lock.push_back(entry.clone());
        }
        
        debug!("Appended {} entries, total: {}", entries.len(), entries_lock.len());
        
        Ok(())
    }
}
