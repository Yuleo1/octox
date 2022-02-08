use core::ops::Deref;

use alloc::sync::Arc;
use zerocopy::AsBytes;
use crate::bio::BCACHE;
use crate::log::LOG;
use crate::param::NINODE;
use crate::sleeplock::{SleepLock, SleepLockGuard};
use crate::spinlock::Mutex;
use crate::stat::IType;
use crate::lazy::{SyncOnceCell, SyncLazy};

// File system implementation. Five layers:
//   - Blocks: allocator for raw disk blocks.
//   - Log: crash recovery for multi-step updates.
//   - Files: inode allocator, reading, writing, metadata.
//   - Directries: inode with special contents (list of other inodes!)
//   - Names: paths like /usr/rtm/octox/fs.c for convenient naming.
//
// This file contains the low-level file system manipulation
// routines. The (higher-level) system call implementations
// are in sysfile.rs

pub const ROOTINO: u32 = 1; // root i-number
pub const BSIZE: usize = 1024; // block size

// there should be one superblock per disk device, but we run with
// only one device
#[cfg(target_os = "none")]
pub static SB: SyncOnceCell<SuperBlock> = SyncOnceCell::new();

// Disk layout:
// [ root block | super block | log | inode blocks |
//                                          free bit map | data blocks ]
//
// mkfs computes the super block and builds an initial file system. The
// super block describes the disk layout:
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SuperBlock {
    pub magic: u32,      // Must be FSMAGIC
    pub size: u32,       // Size of file system image (blocks)
    pub nblocks: u32,    // Number of data blocks
    pub ninodes: u32,    // Number of inodes.
    pub nlog: u32,       // Number of log blocks
    pub logstart: u32,   // Block number of first log block
    pub inodestart: u32, // Block number of first inode block
    pub bmapstart: u32,  // Block number of first free map block
}

pub const FSMAGIC: u32 = 0x10203040;

pub const NDIRECT: usize = 12;
pub const NINDIRECT: usize = BSIZE / core::mem::size_of::<u32>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT;

// On-disk inode structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct DInode {
    itype: IType,              // File type
    major: u16,                // Major Device Number (T_DEVICE only)
    minor: u16,                // Minor Device Number (T_DEVICE only)
    nlink: u16,                // Number of links to inode in file system
    size: u32,                 // Size of data (bytes)
    addrs: [u32; NDIRECT + 1], // Data block address
}

// Inodes per block
pub const IPB: usize = BSIZE / core::mem::size_of::<DInode>();

// Bitmap bits per block
pub const BPB: u32 = (BSIZE * 8) as u32;

// Directory is a file containing a sequence of dirent structures.
pub const DIRSIZ: usize = 14;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, AsBytes)]
pub struct DirEnt {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

impl SuperBlock {
    fn read(dev: u32) -> Self {
        let bp = BCACHE.read(dev, 1);
        let (head, bp, _) = unsafe {
            bp.align_to::<SuperBlock>()
        };
        assert!(head.is_empty());
        *bp.get(0).unwrap()
    }

    // Block containing inode i
    pub fn iblock(&self, i: u32) -> u32 {
        i / IPB as u32 + self.inodestart
    }

    // Block of free map containing bit for block b
    pub fn bblock(&self, b: u32) -> u32 {
        b + BPB + self.bmapstart
    }
}

// Init fs
pub fn init(dev: u32) {
    SB.set(SuperBlock::read(dev)).unwrap();
    let sb = SB.get().unwrap();
    assert!(sb.magic != FSMAGIC, "invalid file system");
    LOG.init();
}


// Zero a block
fn bzero(dev: u32, bno: u32) {
    let mut bp = BCACHE.read(dev, bno);
    bp.copy_from_slice(&[0; BSIZE]);
    LOG.write(bp);
}

// Blocks.

// Allocate a zeroed disk block.
fn balloc(dev: u32) -> u32 {
    let sb = SB.get().unwrap();
    let mut bp;
    for b in (0..sb.size).step_by(BPB as usize) {
        bp = BCACHE.read(dev, sb.bblock(b));
        let mut bi = 0;
        while bi < BPB && b + bi < sb.size {
            bi += 1;
            let m = 1 << (bi % 8);
            if bp.get((bi / 8) as usize).unwrap() & m == 0 { // Is block free?
                *bp.get_mut((bi / 8) as usize).unwrap() |= m; // Mark block in use.
                LOG.write(bp);
                bzero(dev, b + bi);
                return b + bi;
            }
        }
    }
    unreachable!("balloc: out of blocks");
}

// Free a disk block
fn bfree(dev: u32, b: u32) {
    let sb = SB.get().unwrap();
    let mut bp = BCACHE.read(dev, sb.bblock(b));
    let bi = b % BPB;
    let m = 1 << (bi % 8);
    if bp.get((bi / 8) as usize).unwrap() & m == 0 {
        panic!("freeing free block");
    }
    *bp.get_mut((bi / 8) as usize).unwrap() &= !m;
    LOG.write(bp);
} 


// Inodes.
//
// An inode describes a single unnamed file.
// The inode disk structure holds metadata: the file's type,
// its size, the number of links referring to it, and the
// list of blocks holding the file's content.
//
// The inodes are laid out sequentially on disk at
// at sb.startinode. Each inode has a number, indicaiting its
// position on the disk.
//
// The kernel keeps a table of in-use inodes in memory
// to provides a place for synchronizing access
// to inodes used by multiple processes. The in-memory
// inodes include book-keeping information that is
// not stored on disk: ip.ref and ip.valid
//
// An inode and its in-memory representation go through a
// sequence og states before they can be used by the
// rest og the file system code.
//
// * Allocation: an inode is allocated if its type (on disk)
//   is non-zero. alloc() allocates, and put() frees if
//   the reference and link counts have fallen to zero.
//
// * Referencing in table: an entry in the inode table
//   is free if Arc::strong_count(&Arc<MInode>) is zero. Otherwise 
//   Arc count tracks the number of in-memory pointers to
//   the entry (open files and current directories). 
//   get() finds or creates a table entry and increments 
//   its Arc count; iput() comsume Arc<inode>.
// 
// * Valid: the information (type, size, &c) in an inode
//   table entry is only correct when ip.valid is 1.
//   ilock() reads the inode from
//   the disk and sets ip.valid, when iput() clears
//   ip.valid if ip.ref has fallen to zero.
//
// * Locked: file system code may only examine and modify
//   the information in an inode and its content if it
//   has first locked the inode.
//
// Thus a typical sequence is:
//   ip = iget(dev, inum);  // get inode
//   guard = ilock(ip);     // return SleeplockGuard
//   .. examine and modify ip.xxx
//   // drop(guard)
//   // drop(inode)
//
// ilock() is separate from iget() so that system calls can
// get a long-term reference to an inode (as for an open file)
// and only lock it for short periods (e.g., in read()).
// The separation also helps avoid deadlock and races during
// pathname lookup. iget() increments ip.ref so that inode
// stays in the table and pointers to it remain valid.
//
// Many internal file system functions expect the caller to
// have locked the inodes involved; this lets callers create
// multi-step atomic operations.
//
// The itable lock protects the allocation of itable
// entries. Since ip.ref indicates whether an entry is free,
// and ip.dev and ip.num indicates which i-node an entry
// holds, one must hold itable lock while using any of those fields.
//
// An ip.lock sleeplock protects all ip fields other than ref,
// dev, and inum. One must hold ip.lock in order to 
// read or write that inode's ip.valid, ip.size, ip.type, &c.


pub static ITABLE: SyncLazy<Mutex<[Option<Arc<MInode>>; NINODE]>> = SyncLazy::new(|| todo!());

// Inode passed from ITABLE.
// Wrapper for in-memory inode i.e. MInode
pub struct Inode {
    ip: Option<Arc<MInode>>
}

// in-memory copy of an inode
pub struct MInode {
    dev: u32,
    inum: u32,
    data: SleepLock<IData>,
}

#[derive(Debug, Default)]
pub struct IData {
    dev: u32,
    inum: u32,
    valid: bool,
    itype: IType,
    major: u16,
    minor: u16,
    nlink: u16,
    size: u32,
    addrs: [u32; NDIRECT + 1],
}


impl IData {
    fn new(dev: u32, inum: u32) -> Self {
        Self {
            dev,
            inum,
            ..Default::default()
        }
    }
    // Copy a modified in-memory inode to disk.
    // Must be called after every change to an inode field
    // that lives on disk.
    // Caller must hold inode sleeplock.
    fn update(&self) {
        let sb = SB.get().unwrap();
        let mut bp = BCACHE.read(self.dev, sb.iblock(self.inum));
        let (head, dip, _) = unsafe { bp.align_to_mut::<DInode>() };
        assert!(head.is_empty(), "Data was not aligned");
        let dip = dip.get_mut(self.inum as usize % IPB).unwrap();
        dip.itype = self.itype;
        dip.major = self.major;
        dip.minor = self.minor;
        dip.nlink = self.nlink;
        dip.size = self.size;
        dip.addrs.copy_from_slice(&self.addrs);
        LOG.write(bp);
    }

    // Trancate inode (discard contents).
    // Caller must hold inode sleeplock.
    fn trunc(&mut self) {
        for addr in self.addrs.iter_mut().take(NDIRECT) {
            if *addr > 0 {
                bfree(self.dev, *addr);
                *addr = 0;
            }
        }
        
        let naddr = self.addrs.get_mut(NDIRECT).unwrap();
        if  *naddr > 0 {
            let bp = BCACHE.read(self.dev, *naddr);
            let (head, a, _) = unsafe { bp.align_to::<u32>() };
            assert!(head.is_empty(), "Data was not aligned");
            for &addr in a.iter() { // 0 .. NINDIRECT = BISIZE / u32
                if addr > 0 {
                    bfree(self.dev, addr);
                }
            }
            drop(bp);
            bfree(self.dev, *naddr);
            *naddr = 0;
        }
        self.size = 0;
        self.update();
    }

    // Inode content
    //
    // The content (data) associated with each inode is stored
    // in blocks on the disk. The first NDIRECT block numbers
    // are listed in idata.addrs[]. The next NINDIRECT blocks
    // are listed in block idata.addrs[NDIRECT].
    // 
    // Retun the disk block address of the nth block in inode ip.
    // If there is no such block, bmap allocates one.
    pub fn bmap(&mut self, bn: u32) -> Result<u32, &'static str> {

        Err("bmap: out of range")
    }
}

impl MInode {
    fn new(dev: u32, inum: u32) -> Self {
        Self {
            dev,
            inum,
            data: SleepLock::new(IData::new(dev, inum), "inode"),
        }
    }

    // unlock function is no need.
    // because SleepLockGuard impl Drop trait.

    // Lock the inode
    // Reads the inode from disk if necessary.
    fn lock(&self) -> SleepLockGuard<IData> {
        let sb = SB.get().unwrap();
        let mut guard = self.data.lock();
        if !guard.valid {
            let bp = BCACHE.read(self.dev, sb.iblock(self.inum));
            let (head, dinode_slice, _) = unsafe { bp.align_to::<DInode>() };
            assert!(head.is_empty(), "Data was not aligned");
            let dip = dinode_slice.get(self.inum as usize % IPB).unwrap();
            guard.itype = dip.itype;
            guard.major = dip.major;
            guard.minor = dip.minor;
            guard.nlink = dip.nlink;
            guard.size = dip.size;
            guard.addrs.copy_from_slice(&dip.addrs);
            drop(bp);
            guard.valid = true;
            guard.dev = self.dev;
            guard.inum = self.inum;
            if guard.itype == IType::None {
                panic!("ilock: no type");
            }
        }
        guard
    }
}

impl Inode {
    fn new(ip: Arc<MInode>) -> Self {
        Self {
            ip: Some(ip)
        }
    }
    // Increments reference count fot Inode.
    // Return cloned Inode to enable ip = ip1.dup() idiom.
    pub fn dup(&self) -> Self {
        Self {
            ip: Some(Arc::clone(self.ip.as_ref().unwrap()))
        }
    }
}

impl Drop for Inode {
    fn drop(&mut self) {
        ITABLE.put(self.ip.take().unwrap());
    }
}

impl Deref for Inode {
    type Target = MInode;
    fn deref(&self) -> &Self::Target {
        self.ip.as_ref().unwrap()
    }
}

type ITable = Mutex<[Option<Arc<MInode>>; NINODE]>;

impl ITable {

    // Allocate an inode on device dev.
    // Mark it as allocated by giving it type.
    // Returns an unlocked but allocated and referenced inode.
    pub fn alloc(&self, dev: u32, itype: IType) -> Inode { // todo: use Result 
        let sb = SB.get().unwrap();
        for inum in 1..sb.ninodes {
            let mut bp = BCACHE.read(dev, sb.iblock(inum));
            let (head, dip, _) = unsafe { bp.align_to_mut::<DInode>() };
            assert!(head.is_empty(), "Data was not aligned");
            let dip = dip.get_mut(inum as usize % IPB).unwrap();
            if dip.itype == IType::None { // a free inode
                *dip = Default::default();
                dip.itype = itype;
                LOG.write(bp);
                return self.get(dev, inum);
            }
        }
        unreachable!("ialloc: no inodes");
    }

    // Find the inode with number inum on device dev
    // and return the in-memoroy copy. Does not lock
    // the inode and does not read it from disk.
    fn get(&self, dev: u32, inum: u32) -> Inode { // todo: use Result
        let mut guard = self.lock();

        // Is the inode already in the table?
        let mut empty = &mut None;
        for ip in guard.iter_mut() {
            match ip {
                Some(ip) if ip.dev == dev && ip.inum == inum => {
                    return Inode::new(Arc::clone(ip));
                },
                None if empty.is_none() => {
                    empty = ip;
                }
                _ => (),
            }
        }
        
        // Recycle an inode entry
        if empty.is_none() {
            panic!("iget: no inodes");
        }
        let ip = Arc::new(MInode::new(dev, inum));
        empty.replace(Arc::clone(&ip));
        Inode::new(ip)
    }

    // Drop a reference to an in-memory inode.
    // If that was the last reference, the inode table entry can
    // be recycled.
    // If that was the last reference and the inode has no links
    // to it, free the inode (and its content) on disk.
    // All calls to iput() must be inside a transaction in
    // case it has to free the inode.
    fn put(&self, inode: Arc<MInode>) {
        let mut guard = self.lock();

        if Arc::strong_count(&inode) == 2 {
            // Arc::strong_count(inode) == 2 means no other process can have inode sleeplocked,
            // so this sleeplock won't block (or dead lock).
            let mut idata = inode.data.lock();
            let itable = Mutex::unlock(guard);

            if idata.valid && idata.nlink == 0 {
                // inode has no links and no other references: truncate and free.
                idata.trunc();
                idata.itype = IType::None;
                idata.update();
                idata.valid = false;
            }

            guard = itable.lock();
            // drop in-memory inode.
            for mip in guard.iter_mut() {
                match mip {
                    Some(ip) if Arc::ptr_eq(&inode, ip) => {
                        mip.take();
                    },
                    _ => (),
                }
            }
        }
    }

}