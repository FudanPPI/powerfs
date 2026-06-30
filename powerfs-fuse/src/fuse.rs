use powerfs_common::{
    types::{FileId, FileMetadata, VolumeId, NeedleId},
    utils::{generate_file_id, normalize_path},
    error::{PowerFsError, Result},
};
use powerfs_core::storage::StorageManager;
use fuse_backend_rs::{
    api::{Vfs, VfsOptions},
    server::Server as FuseServer,
    transport::ServerTransport,
};
use std::collections::HashMap;
use std::sync::{RwLock, Arc};
use std::ffi::OsStr;
use std::path::Path;
use std::os::unix::ffi::OsStrExt;
use chrono::Utc;
use libc;
use log::{info, debug, warn};

struct PowerFsVfs {
    storage_manager: Arc<StorageManager>,
    inode_map: RwLock<HashMap<u64, FileMetadata>>,
    path_map: RwLock<HashMap<String, FileId>>,
    next_inode: RwLock<u64>,
}

impl PowerFsVfs {
    fn new(storage_manager: Arc<StorageManager>) -> Self {
        PowerFsVfs {
            storage_manager,
            inode_map: RwLock::new(HashMap::new()),
            path_map: RwLock::new(HashMap::new()),
            next_inode: RwLock::new(2),
        }
    }

    fn allocate_inode(&self) -> u64 {
        let mut next = self.next_inode.write().unwrap();
        let inode = *next;
        *next += 1;
        inode
    }

    fn path_to_inode(&self, path: &str) -> Option<u64> {
        let path_map = self.path_map.read().unwrap();
        let inode_map = self.inode_map.read().unwrap();
        
        path_map.get(path).and_then(|file_id| {
            inode_map.iter().find(|(_, meta)| &meta.file_id == file_id).map(|(inode, _)| *inode)
        })
    }

    fn inode_to_metadata(&self, inode: u64) -> Option<FileMetadata> {
        self.inode_map.read().unwrap().get(&inode).cloned()
    }
}

impl Vfs for PowerFsVfs {
    fn lookup(&self, _parent: u64, name: &OsStr) -> std::result::Result<u64, fuse_backend_rs::error::Error> {
        let name_str = name.to_str().unwrap_or("");
        let path = format!("/{}", name_str);
        
        if let Some(inode) = self.path_to_inode(&path) {
            debug!("Lookup path: {} -> inode: {}", path, inode);
            Ok(inode)
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn getattr(&self, inode: u64) -> std::result::Result<fuse_backend_rs::api::FileAttr, fuse_backend_rs::error::Error> {
        if inode == 1 {
            Ok(fuse_backend_rs::api::FileAttr {
                ino: 1,
                kind: fuse_backend_rs::api::FileType::Directory,
                nlink: 2,
                uid: 0,
                gid: 0,
                size: 0,
                blocks: 0,
                atime: chrono::Utc::now().timestamp(),
                mtime: chrono::Utc::now().timestamp(),
                ctime: chrono::Utc::now().timestamp(),
                blksize: 4096,
            })
        } else if let Some(meta) = self.inode_to_metadata(inode) {
            Ok(fuse_backend_rs::api::FileAttr {
                ino: inode,
                kind: fuse_backend_rs::api::FileType::RegularFile,
                nlink: 1,
                uid: meta.uid,
                gid: meta.gid,
                size: meta.size,
                blocks: (meta.size + 4095) / 4096,
                atime: meta.atime.timestamp(),
                mtime: meta.mtime.timestamp(),
                ctime: meta.ctime.timestamp(),
                blksize: 4096,
            })
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn create(&self, _parent: u64, name: &OsStr, _mode: u32, _flags: u32) -> std::result::Result<(u64, u64), fuse_backend_rs::error::Error> {
        let name_str = name.to_str().unwrap_or("");
        let path = format!("/{}", name_str);
        
        let mut path_map = self.path_map.write().unwrap();
        if path_map.contains_key(&path) {
            return Err(fuse_backend_rs::error::Error::EEXIST);
        }
        
        let inode = self.allocate_inode();
        let file_id = generate_file_id();
        
        let metadata = FileMetadata {
            file_id: file_id.clone(),
            name: name_str.to_string(),
            size: 0,
            mode: 0o644,
            uid: 0,
            gid: 0,
            atime: Utc::now(),
            mtime: Utc::now(),
            ctime: Utc::now(),
            volume_ids: vec![],
            needle_ids: vec![],
        };
        
        path_map.insert(path, file_id);
        
        let mut inode_map = self.inode_map.write().unwrap();
        inode_map.insert(inode, metadata);
        
        debug!("Created file: {} with inode: {}", name_str, inode);
        
        Ok((inode, 0))
    }

    fn open(&self, inode: u64, _flags: u32) -> std::result::Result<u64, fuse_backend_rs::error::Error> {
        if self.inode_to_metadata(inode).is_some() || inode == 1 {
            debug!("Opened inode: {}", inode);
            Ok(inode)
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn read(&self, inode: u64, _fh: u64, offset: u64, size: u32) -> std::result::Result<Vec<u8>, fuse_backend_rs::error::Error> {
        if let Some(meta) = self.inode_to_metadata(inode) {
            if !meta.needle_ids.is_empty() {
                if let Some(volume_id) = meta.volume_ids.first() {
                    if let Some(volume) = self.storage_manager.get_volume(volume_id) {
                        if let Some(needle_id) = meta.needle_ids.first() {
                            if let Ok(data) = volume.read_needle(needle_id) {
                                let start = std::cmp::min(offset as usize, data.len());
                                let end = std::cmp::min(start + size as usize, data.len());
                                return Ok(data.slice(start..end).to_vec());
                            }
                        }
                    }
                }
            }
            Ok(vec![])
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn write(&self, inode: u64, _fh: u64, offset: u64, data: &[u8]) -> std::result::Result<u32, fuse_backend_rs::error::Error> {
        let mut inode_map = self.inode_map.write().unwrap();
        
        if let Some(meta) = inode_map.get_mut(&inode) {
            let data_len = data.len() as u64;
            
            if meta.needle_ids.is_empty() {
                if let Some(volume_id) = self.storage_manager.find_available_volume() {
                    if let Some(volume) = self.storage_manager.get_volume(&volume_id) {
                        if let Ok(needle_info) = volume.write_needle(bytes::Bytes::from(data.to_vec())) {
                            meta.volume_ids.push(volume_id);
                            meta.needle_ids.push(needle_info.id);
                            meta.size = data_len;
                            meta.mtime = Utc::now();
                            return Ok(data.len() as u32);
                        }
                    }
                }
            }
            
            meta.size = std::cmp::max(meta.size, offset + data_len);
            meta.mtime = Utc::now();
            
            Ok(data.len() as u32)
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn release(&self, _inode: u64, _fh: u64) -> std::result::Result<(), fuse_backend_rs::error::Error> {
        Ok(())
    }

    fn unlink(&self, _parent: u64, name: &OsStr) -> std::result::Result<(), fuse_backend_rs::error::Error> {
        let name_str = name.to_str().unwrap_or("");
        let path = format!("/{}", name_str);
        
        let mut path_map = self.path_map.write().unwrap();
        if let Some(file_id) = path_map.remove(&path) {
            let mut inode_map = self.inode_map.write().unwrap();
            inode_map.retain(|_, meta| meta.file_id != file_id);
            Ok(())
        } else {
            Err(fuse_backend_rs::error::Error::ENOENT)
        }
    }

    fn readdir(&self, inode: u64, _fh: u64) -> std::result::Result<Vec<(u64, String)>, fuse_backend_rs::error::Error> {
        if inode != 1 {
            return Err(fuse_backend_rs::error::Error::ENOTDIR);
        }
        
        let inode_map = self.inode_map.read().unwrap();
        let result: Vec<(u64, String)> = inode_map.iter()
            .map(|(inode, meta)| (*inode, meta.name.clone()))
            .collect();
        
        Ok(result)
    }
}

pub struct FuseClient {
    vfs: Arc<PowerFsVfs>,
    mount_point: String,
}

impl FuseClient {
    pub fn new(storage_manager: Arc<StorageManager>, mount_point: &str) -> Self {
        FuseClient {
            vfs: Arc::new(PowerFsVfs::new(storage_manager)),
            mount_point: mount_point.to_string(),
        }
    }

    pub async fn mount(&self) -> Result<()> {
        let options = VfsOptions::new();
        let mut server = FuseServer::new(self.vfs.clone(), options);
        
        let path = Path::new(&self.mount_point);
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }
        
        info!("Mounting PowerFS at: {}", self.mount_point);
        
        server.mount(path).await.map_err(|e| {
            warn!("Failed to mount FUSE: {}", e);
            PowerFsError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;
        
        Ok(())
    }

    pub async fn unmount(&self) -> Result<()> {
        info!("Unmounting PowerFS from: {}", self.mount_point);
        let _ = nix::mount::umount(&self.mount_point);
        Ok(())
    }
}
