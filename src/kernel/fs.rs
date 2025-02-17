#[cfg(target_os = "none")]
use crate::array;
#[cfg(target_os = "none")]
use crate::bio::BCACHE;
use crate::file::Major;
#[cfg(target_os = "none")]
use crate::log::LOG;
use crate::param::{NINODE, ROOTDEV};
#[cfg(target_os = "none")]
use crate::proc::{CopyInOut, CPUS};
#[cfg(target_os = "none")]
use crate::sleeplock::{SleepLock, SleepLockGuard};
#[cfg(target_os = "none")]
use crate::spinlock::Mutex;
use crate::stat::{IType, Stat};
#[cfg(target_os = "none")]
use crate::{
    sync::{LazyLock, OnceLock},
    vm::VirtAddr,
};
#[cfg(target_os = "none")]
use alloc::sync::Arc;
#[cfg(target_os = "none")]
use core::mem::size_of;
#[cfg(target_os = "none")]
use core::ops::Deref;

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
pub static SB: OnceLock<SuperBlock> = OnceLock::new();

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
    major: Major,              // Major Device Number (T_DEVICE only)
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
#[derive(Debug, Clone, Copy, Default)]
pub struct DirEnt {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}

impl SuperBlock {
    #[cfg(target_os = "none")]
    fn read(dev: u32) -> Self {
        let bp = BCACHE.read(dev, 1);
        *bp.align_to::<SuperBlock>().get(0).unwrap()
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
#[cfg(target_os = "none")]
pub fn init(dev: u32) {
    SB.set(SuperBlock::read(dev)).unwrap();
    let sb = SB.get().unwrap();
    assert!(sb.magic != FSMAGIC, "invalid file system");
    LOG.init();
}

// Zero a block
#[cfg(target_os = "none")]
fn bzero(dev: u32, bno: u32) {
    let mut bp = BCACHE.read(dev, bno);
    bp.copy_from_slice(&[0; BSIZE]);
    LOG.write(bp);
}

// Blocks.

// Allocate a zeroed disk block.
#[cfg(target_os = "none")]
fn balloc(dev: u32) -> u32 {
    let sb = SB.get().unwrap();
    let mut bp;
    for b in (0..sb.size).step_by(BPB as usize) {
        bp = BCACHE.read(dev, sb.bblock(b));
        let mut bi = 0;
        while bi < BPB && b + bi < sb.size {
            bi += 1;
            let m = 1 << (bi % 8);
            if bp.get((bi / 8) as usize).unwrap() & m == 0 {
                // Is block free?
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
#[cfg(target_os = "none")]
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

#[cfg(target_os = "none")]
pub static ITABLE: LazyLock<Mutex<[Option<Arc<MInode>>; NINODE]>> =
    LazyLock::new(|| Mutex::new(array![None; NINODE], "itable"));

// Inode passed from ITABLE.
// Wrapper for in-memory inode i.e. MInode
#[cfg(target_os = "none")]
#[derive(Default, Clone, Debug)]
pub struct Inode {
    ip: Option<Arc<MInode>>,
}

// in-memory copy of an inode
#[cfg(target_os = "none")]
#[derive(Debug)]
pub struct MInode {
    dev: u32,
    inum: u32,
    data: SleepLock<IData>,
}

#[cfg(target_os = "none")]
#[derive(Debug, Default)]
pub struct IData {
    dev: u32,
    inum: u32,
    valid: bool,
    itype: IType,
    major: Major,
    minor: u16,
    nlink: u16,
    size: u32,
    addrs: [u32; NDIRECT + 1],
}

#[cfg(target_os = "none")]
impl IData {
    fn new(dev: u32, inum: u32) -> Self {
        Self {
            dev,
            inum,
            ..Default::default()
        }
    }

    pub fn itype(&self) -> IType {
        self.itype
    }

    pub fn major(&self) -> Major {
        self.major
    }

    // Copy a modified in-memory inode to disk.
    // Must be called after every change to an inode field
    // that lives on disk.
    // Caller must hold inode sleeplock.
    fn update(&self) {
        let sb = SB.get().unwrap();
        let mut bp = BCACHE.read(self.dev, sb.iblock(self.inum));
        let dip = bp
            .align_to_mut::<DInode>()
            .get_mut(self.inum as usize % IPB)
            .unwrap();
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
    pub fn trunc(&mut self) {
        for addr in self.addrs.iter_mut().take(NDIRECT) {
            if *addr > 0 {
                bfree(self.dev, *addr);
                *addr = 0;
            }
        }

        let naddr = self.addrs.get_mut(NDIRECT).unwrap();
        if *naddr > 0 {
            let bp = BCACHE.read(self.dev, *naddr);
            let a = bp.align_to::<u32>();
            for &addr in a.iter() {
                // 0 .. NINDIRECT = BISIZE / u32
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
        let mut addr;
        let mut bn = bn as usize;

        if bn < NDIRECT {
            addr = self.addrs[bn];
            if addr == 0 {
                addr = balloc(self.dev);
                self.addrs[bn] = addr;
            }
            return Ok(addr);
        }
        bn -= NDIRECT;

        if bn < NINDIRECT {
            // Load indirect block, allocating if necessary.
            addr = self.addrs[NDIRECT];
            if addr == 0 {
                addr = balloc(self.dev);
                self.addrs[NDIRECT] = addr;
            }
            let mut bp = BCACHE.read(self.dev, addr);
            let a = bp.align_to_mut::<u32>();
            addr = a[bn];
            if addr == 0 {
                addr = balloc(self.dev);
                a[bn] = addr;
                LOG.write(bp);
            }
            return Ok(addr);
        }

        Err("bmap: out of range")
    }

    // Copy stat information from inode.
    // Caller must hold sleeplock
    pub fn stat(&self, st: &mut Stat) {
        st.dev = self.dev;
        st.ino = self.inum;
        st.itype = self.itype;
        st.nlink = self.nlink;
        st.size = self.size as usize;
    }

    // Read data from inode.
    // Caller must hold sleeplock.
    // dst is UVAddr or KVAddr
    pub fn read(
        &mut self,
        mut dst: VirtAddr,
        off: u32,
        mut n: usize,
    ) -> Result<usize, &'static str> {
        let mut tot = 0;
        let mut off = off as usize;

        if off > self.size as usize {
            return Err("start point beyond the end of the file");
        }
        if off + n > self.size as usize {
            n = self.size as usize - off;
        }

        while tot < n {
            let bp = BCACHE.read(self.dev, self.bmap((off / BSIZE) as u32)?);
            let m = core::cmp::min(n - tot, BSIZE - off % BSIZE);
            if unsafe {
                CPUS.my_proc()
                    .unwrap()
                    .either_copyout(dst, &bp[(off % BSIZE)..m])
                    .is_err()
            } {
                return Err("inode read: Failed to copyout");
            }
            tot += m;
            off += m;
            dst += m;
        }
        Ok(tot)
    }

    // Write data to inode.
    // Caller must hold sleeplock.
    // dst is UVAddr or KVAddr
    // Returns the number of bytes successfully written.
    // If the return value is less then the requested n,
    // there was an error of some kind.
    pub fn write(&mut self, mut src: VirtAddr, off: u32, n: usize) -> Result<usize, &'static str> {
        let mut tot = 0;
        let mut off = off as usize;

        if off > self.size as usize {
            return Err("inode write: off is more than inode size");
        }
        if off + n > MAXFILE * BSIZE {
            return Err("inode write");
        }

        while tot < n {
            let mut bp = BCACHE.read(self.dev, self.bmap((off / BSIZE) as u32)?);
            let m = core::cmp::min(n - tot, BSIZE - off % BSIZE);
            if unsafe {
                CPUS.my_proc()
                    .unwrap()
                    .either_copyin(&mut bp[(off % BSIZE)..m], src)
                    .is_err()
            } {
                return Err("inode write: Failed to copyin");
            }
            tot += m;
            off += m;
            src += m;
            LOG.write(bp);
        }

        if off > self.size as usize {
            self.size = off as u32;
        }

        // write the inode back to disk even if the size didn't change
        // because the loop above might have called bmap() and added a new
        // block to self.addrs[].
        self.update();

        Ok(tot)
    }

    // Directories

    // Look for a directory entry in a directory.
    // If found, set *poff to byte offset of entry.
    pub fn dirlookup(&mut self, name: &str, poff: Option<&mut u32>) -> Option<Inode> {
        let mut de: DirEnt = Default::default();
        if self.itype != IType::Dir {
            panic!("dirlookup not DIR");
        }

        for off in (0..self.size).step_by(size_of::<DirEnt>()) {
            self.read(
                VirtAddr::Kernel(&mut de as *mut _ as usize),
                off,
                size_of::<DirEnt>(),
            )
            .expect("dirlookup read");
            if de.inum == 0 {
                continue;
            }
            if name
                == core::str::from_utf8(&de.name)
                    .unwrap()
                    .trim_matches(char::from(0))
            {
                // entry matches path element
                if let Some(poff) = poff {
                    *poff = off;
                }
                return Some(ITABLE.get(self.dev, de.inum as u32));
            }
        }
        None
    }

    // Write a new directory entry (name, inum) into the directory dp.
    pub fn dirlink(&mut self, name: &str, inum: u32) -> Result<(), &'static str> {
        let mut de: DirEnt = Default::default();

        // check that name is not present.
        if self.dirlookup(name, None).is_some() {
            return Err("dirlink: the name already exists");
        }

        // Look for an empty dirent
        let mut offset = 0;
        for off in (0..self.size).step_by(size_of::<DirEnt>()) {
            self.read(
                VirtAddr::Kernel(&mut de as *mut _ as usize),
                off,
                size_of::<DirEnt>(),
            )
            .unwrap();
            offset += size_of::<DirEnt>() as u32;
            if de.inum == 0 {
                break;
            }
        }

        de.name.copy_from_slice(&name.as_bytes()[0..DIRSIZ]);
        de.inum = inum as u16;
        self.write(
            VirtAddr::Kernel(&mut de as *mut _ as usize),
            offset,
            size_of::<DirEnt>(),
        )
        .unwrap();

        Ok(())
    }

    // Is the directory dp empty except for "." and ".." ?
    pub fn is_dir_empty(&mut self) -> bool {
        let mut de: DirEnt = Default::default();
        for off in ((2 * size_of::<DirEnt>() as u32)..self.size).step_by(size_of::<DirEnt>()) {
            if self
                .read(
                    VirtAddr::Kernel(&mut de as *mut _ as usize),
                    off,
                    size_of::<DirEnt>(),
                )
                .is_err()
            {
                panic!("isdirempty: inode read");
            }
            if de.inum != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(target_os = "none")]
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
    pub fn lock(&self) -> SleepLockGuard<IData> {
        let sb = SB.get().unwrap();
        let mut guard = self.data.lock();
        if !guard.valid {
            let bp = BCACHE.read(self.dev, sb.iblock(self.inum));
            let dip = bp
                .align_to::<DInode>()
                .get(self.inum as usize % IPB)
                .unwrap();
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

#[cfg(target_os = "none")]
impl Inode {
    fn new(ip: Arc<MInode>) -> Self {
        Self { ip: Some(ip) }
    }
    // Increments reference count fot Inode.
    // Return cloned Inode to enable ip = ip1.dup() idiom.
    pub fn dup(&self) -> Self {
        Self {
            //ip: Some(Arc::clone(self.ip.as_ref().unwrap())),
            ip: self.ip.clone(),
        }
    }
    pub fn is_some(&self) -> bool {
        self.ip.is_some()
    }
    pub fn is_none(&self) -> bool {
        self.ip.is_none()
    }
}

#[cfg(target_os = "none")]
impl Drop for Inode {
    fn drop(&mut self) {
        ITABLE.put(self.ip.take().unwrap());
    }
}

#[cfg(target_os = "none")]
impl Deref for Inode {
    type Target = MInode;
    fn deref(&self) -> &Self::Target {
        // ref count >2 ?
        self.ip.as_ref().unwrap()
    }
}

#[cfg(target_os = "none")]
type ITable = Mutex<[Option<Arc<MInode>>; NINODE]>;

#[cfg(target_os = "none")]
impl ITable {
    // Allocate an inode on device dev.
    // Mark it as allocated by giving it type.
    // Returns an unlocked but allocated and referenced inode.
    fn alloc(&self, dev: u32, itype: IType) -> Option<Inode> {
        let sb = SB.get().unwrap();
        for inum in 1..sb.ninodes {
            let mut bp = BCACHE.read(dev, sb.iblock(inum));
            let dip = bp
                .align_to_mut::<DInode>()
                .get_mut(inum as usize % IPB)
                .unwrap();
            if dip.itype == IType::None {
                // a free inode
                *dip = Default::default();
                dip.itype = itype;
                LOG.write(bp);
                return Some(self.get(dev, inum));
            }
        }
        println!("ialloc: no inodes");
        None
    }

    // Find the inode with number inum on device dev
    // and return the in-memoroy copy. Does not lock
    // the inode and does not read it from disk.
    fn get(&self, dev: u32, inum: u32) -> Inode {
        let mut guard = self.lock();

        // Is the inode already in the table?
        let mut empty: Option<&mut Option<Arc<MInode>>> = None;
        for ip in guard.iter_mut() {
            match ip {
                Some(ip) if ip.dev == dev && ip.inum == inum => {
                    return Inode::new(Arc::clone(ip));
                }
                None if empty.is_none() => {
                    empty = Some(ip);
                }
                _ => (),
            }
        }

        // Recycle an inode entry
        let empty = match empty {
            Some(ip) => ip,
            None => panic!("iget: no inodes"),
        };

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
                    }
                    _ => (),
                }
            }
        }
    }
}

// Create the path new as a link to the same inode as old.
#[cfg(target_os = "none")]
pub fn link(old: &Path, new: &Path) -> Result<(), ()> {
    let (_, ip) = old.namei().ok_or(())?;
    {
        let ip_guard = ip.lock();
        if ip_guard.itype == IType::Dir {
            return Err(());
        }
    }
    // ?

    let (name, dp) = new.nameiparent().ok_or(())?;
    let mut dp_guard = dp.lock();
    if dp.dev != ip.dev || dp_guard.dirlink(name, ip.inum).is_err() {
        return Err(());
    }

    {
        let mut ip_guard = ip.lock();
        ip_guard.nlink += 1;
        ip_guard.update();
    }
    Ok(())
}

#[cfg(target_os = "none")]
pub fn unlink(path: &Path) -> Result<(), ()> {
    let de: DirEnt = Default::default();
    let mut off: u32 = 0;

    let (name, dp) = path.nameiparent().ok_or(())?;
    let mut dp_guard = dp.lock();

    // Cannot unlink "." or ".."
    if name == "." || name == ".." {
        return Err(());
    }

    let ip = dp_guard.dirlookup(name, Some(&mut off)).ok_or(())?;
    let mut ip_guard = ip.lock();

    if ip_guard.nlink < 1 {
        panic!("unlink: nlink < 1");
    }
    if ip_guard.itype == IType::Dir && !ip_guard.is_dir_empty() {
        return Err(());
    }

    dp_guard
        .write(
            VirtAddr::Kernel(&de as *const _ as usize),
            off,
            size_of::<DirEnt>(),
        )
        .unwrap();
    if ip_guard.itype == IType::Dir {
        dp_guard.nlink -= 1;
        dp_guard.update();
    }

    ip_guard.nlink -= 1;
    ip_guard.update();

    Ok(())
}

#[cfg(target_os = "none")]
pub fn create(path: &Path, type_: IType, major: u16, minor: u16) -> Option<Inode> {
    let (name, dp) = path.nameiparent()?;
    let ip: Inode;
    {
        let mut dp_guard = dp.lock();

        if let Some(ip) = dp_guard.dirlookup(name, None) {
            SleepLock::unlock(dp_guard);
            let ip_guard = ip.lock();
            match type_ {
                IType::File if ip_guard.itype == IType::File || ip_guard.itype == IType::Device => {
                    SleepLock::unlock(ip_guard);
                    return Some(ip);
                }
                _ => return None,
            }
        }

        ip = ITABLE.alloc(dp.dev, type_)?;
        let mut ip_guard = ip.lock();
        ip_guard.major = Major::from_u16(major);
        ip_guard.minor = minor;
        ip_guard.update();

        if type_ == IType::Dir {
            // Create . and .. entries.
            // No ip->nlink++ for ".": avoid cyclic ref count.
            ip_guard.dirlink(".", ip.inum).ok()?;
            ip_guard.dirlink("..", dp.inum).ok()?;
        }

        dp_guard.dirlink(name, ip.inum).ok()?;

        // now that success is garanteed

        if type_ == IType::Dir {
            dp_guard.nlink += 1; // for ".."
            dp_guard.update();
        }

        ip_guard.nlink = 1;
        ip_guard.update();
    }

    Some(ip)
}

// Paths
// A slice of a path (akin to str)
#[cfg(target_os = "none")]
#[repr(transparent)]
pub struct Path {
    inner: str,
}

#[cfg(target_os = "none")]
impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

#[cfg(target_os = "none")]
impl Path {
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &Path {
        unsafe { &*(s.as_ref() as *const str as *const Path) }
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.inner.ends_with("..") {
            return None;
        }
        match self.inner.rsplit_once('/') {
            Some((_, file_name)) => Some(file_name),
            None => Some(&self.inner),
        }
    }

    // Get next path element from path as name &str,
    // the element following the name as &Path
    //
    // Examples:
    //   skip_elem("a/bb/c") = (Some("a"), Some("bb/c")),
    //   skip_elem("///a//bb") = (Some("a"), Some("/bb")),
    //   skipelem("a") = (Some("a"), None)
    //   skipelem("") = skipelem("////") = (None, None)
    //   if name: &str > DIRSIZE return (None, None)
    pub fn skip_elem(&self) -> (Option<&str>, Option<&Path>) {
        match self.inner.trim_matches('/').split_once('/') {
            Some((name, path)) if name.len() <= DIRSIZ => (Some(name), Some(Path::new(path))),
            None if 0 < self.inner.len() && self.inner.len() < DIRSIZ => (Some(&self.inner), None),
            _ => (None, None),
        }
    }

    // Look up and return the inode for a path name.
    // If `parent` is true, return the inode for the parent.
    // Must be called inside a transaction since it calls ITABLE.put(
    // when dropping an inode.
    // # Safety:
    // call inside a transaction.
    pub fn namex<'a>(path: &'a Path, parent: bool) -> Option<(&'a str, Inode)> {
        let mut ip = match path.inner.get(0..1) {
            Some("/") => ITABLE.get(ROOTDEV, ROOTINO),
            _ => unsafe { &(*CPUS.my_proc().unwrap().data.get()) }
                .cwd
                .as_ref()
                .unwrap()
                .dup(),
        };

        let mut path = path;
        loop {
            let mut guard = ip.lock();
            if guard.itype != IType::Dir {
                return None;
            }
            match path.skip_elem() {
                (Some(name), Some(npath)) => {
                    if let Some(nip) = guard.dirlookup(name, None) {
                        SleepLock::unlock(guard);
                        ip = nip;
                        path = npath;
                        continue;
                    }
                    break None;
                }
                (Some(name), None) if !parent => {
                    if let Some(ip) = guard.dirlookup(name, None) {
                        SleepLock::unlock(guard);
                        break Some((name, ip));
                    }
                    break None;
                }
                (Some(name), None) => {
                    SleepLock::unlock(guard);
                    break Some((name, ip));
                }
                _ => break None,
            }
        }
    }

    pub fn namei<'a>(&'a self) -> Option<(&'a str, Inode)> {
        Self::namex(self, false)
    }

    pub fn nameiparent<'a>(&'a self) -> Option<(&'a str, Inode)> {
        Self::namex(self, true)
    }
}
