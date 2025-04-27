use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
/// offset is DiskNode
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
    node_id: usize,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
        node_id: usize,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
            node_id,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }

    /// let should removed swap with last entry
    fn remove_name_dir_entry(&self, name: &str) -> Option<(usize, u32)> {
        // debug!("remove_name_dir_entry ------");
        let mut find_removed: Option<(usize, u32, usize)> = None;
        self.read_disk_inode(|disk_inode: &DiskInode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            for i in 0..file_count {
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device);
                if dirent.name() == name {
                    find_removed = Some((i, dirent.inode_id(), file_count));
                    break;
                }
            }
        });

        // debug!("find_removed {:?}", find_removed);
        if let Some((removed_index, node_id, file_count)) = find_removed {
            let mut fs = self.fs.lock();
            let (block_id, block_offset) = fs.get_disk_inode_pos(node_id);
            drop(fs);
            let inode = Arc::new(Self::new(
                block_id,
                block_offset,
                self.fs.clone(),
                self.block_device.clone(),
                node_id as usize,
            ));
            let del_content = inode.modify_disk_inode(|node: &mut DiskInode| {
                node.link_size -= 1;
                node.link_size == 0
            });
            if del_content {
                let mut fs = self.fs.lock();
                fs.dealloc_inode(node_id);
                drop(fs);
                inode.clear();
            }

            let mut dirent = DirEntry::empty();
            /// clear dir content
            // debug!("file_count: {}, removed_index: {}", file_count, removed_index);
            self.modify_disk_inode(|disk_inode: &mut DiskInode| {
                if file_count - 1 >= 0 && removed_index >= 0 {
                    let last_entry = file_count - 1;
                    disk_inode.read_at(
                        DIRENT_SZ * last_entry,
                        dirent.as_bytes_mut(),
                        &self.block_device,
                    );

                    disk_inode.write_at(
                        DIRENT_SZ * (removed_index),
                        dirent.as_bytes(),
                        &self.block_device,
                    );
                }
                disk_inode.size = ((file_count - 1) * DIRENT_SZ) as u32;
            });

            block_cache_sync_all();
            // debug!("remove_name_dir_entry ------end11");
            return Some((removed_index, node_id));
        }
        None
    }

    /// Find inode under current inode by name
    /// root inode
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                    inode_id as usize,
                ))
            })
        })
    }

    /// return node id
    pub fn get_file_name_inode_id(&self, name: &str) -> Option<u32> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| self.find_inode_id(name, disk_inode))
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }

    /// make link
    pub fn link_at(&self, old_name: &str, new_name: &str) -> Option<u32> {
        let inode = self.find(old_name);
        let mut fs = self.fs.lock();
        if let Some(node) = inode {
            let node_id = node.node_id;
            // create a new file
            // alloc a inode with an indirect block
            let new_inode_id = node_id;
            // initialize inode
            self.modify_disk_inode(|root_inode| {
                // append file in the dirent
                let file_count = (root_inode.size as usize) / DIRENT_SZ;
                // let new_size = (file_count + 1) * DIRENT_SZ;
                // debug!("xxxx ---- file_count: {}", file_count);
                let new_size = (file_count + 1) * DIRENT_SZ;
                self.increase_size(new_size as u32, root_inode, &mut fs);
                // write dirent
                let dirent = DirEntry::new(new_name, new_inode_id as u32);
                root_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );
            });

            node.modify_disk_inode(|inode| {
                inode.link_size += 1;
            });
            // debug!("xxxx ---- -----2");
            block_cache_sync_all();
            // debug!("xxxx ---- -----3");
            Some(new_inode_id as u32)
        } else {
            None
        }
    }

    /// make link
    /// 1. dir inode read content and remove this name dir entry
    /// 2. resize dir content
    /// 3. clear file content if file's link_size == 0;
    pub fn unlink(&self, name: &str) -> bool {
        let sign = self.remove_name_dir_entry(name);
        sign.is_some()
    }

    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
            new_inode_id as usize,
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }

    pub fn stat_info(&self) -> (u64, bool, u32) {
        let mut is_dir = false;
        let mut link_cnt = 1;
        self.read_disk_inode(|node: &DiskInode| {
            is_dir = node.is_dir();
            link_cnt = node.link_size;
        });
        return (self.node_id as u64, is_dir, link_cnt);
    }
}
