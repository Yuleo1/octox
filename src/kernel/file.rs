#[cfg(target_os = "none")]
use crate::array;
#[cfg(target_os = "none")]
use crate::fcntl::OMode;
#[cfg(target_os = "none")]
use crate::fs::{create, IData, Inode, Path, BSIZE};
#[cfg(target_os = "none")]
use crate::lazy::{SyncLazy, SyncOnceCell};
#[cfg(target_os = "none")]
use crate::log::LOG;
#[cfg(target_os = "none")]
use crate::param::{MAXOPBLOCKS, NDEV, NFILE};
#[cfg(target_os = "none")]
use crate::pipe::Pipe;
#[cfg(target_os = "none")]
use crate::proc::{CopyInOut, CPUS};
#[cfg(target_os = "none")]
use crate::sleeplock::{SleepLock, SleepLockGuard};
#[cfg(target_os = "none")]
use crate::spinlock::Mutex;
#[cfg(target_os = "none")]
use crate::stat::{IType, Stat};
#[cfg(target_os = "none")]
use crate::vm::VirtAddr;
#[cfg(target_os = "none")]
use alloc::sync::Arc;
#[cfg(target_os = "none")]
use core::cell::UnsafeCell;
#[cfg(target_os = "none")]
use core::ops::Deref;

#[cfg(target_os = "none")]
pub static DEVSW: DevSW = DevSW::new();
#[cfg(target_os = "none")]
pub static FTABLE: SyncLazy<FTable> = SyncLazy::new(|| todo!());

#[cfg(target_os = "none")]
type FTable = Mutex<[Option<Arc<VFile>>; NFILE]>;

#[cfg(target_os = "none")]
#[derive(Default, Clone)]
pub struct File {
    f: Option<Arc<VFile>>,
    readable: bool,
    writable: bool,
}

#[cfg(target_os = "none")]
#[derive(Debug)]
pub enum VFile {
    Device(DNod),
    Inode(FNod),
    Pipe(Pipe),
    None,
}

// Device Node
#[cfg(target_os = "none")]
#[derive(Debug)]
pub struct DNod {
    driver: &'static dyn Device,
    ip: Inode,
}

// Device functions, map this trait using dyn
#[cfg(target_os = "none")]
pub trait Device: Send + Sync {
    fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()>;
    fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()>;
    fn major(&self) -> Major;
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for dyn Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Device fn {:?}", self.major())
    }
}

#[cfg(target_os = "none")]
impl Deref for DNod {
    type Target = dyn Device;
    fn deref(&self) -> &Self::Target {
        self.driver
    }
}

// File & directory Node
#[cfg(target_os = "none")]
#[derive(Debug)]
pub struct FNod {
    off: UnsafeCell<u32>,
    ip: Inode,
}

#[cfg(target_os = "none")]
impl FNod {
    pub fn new(ip: Inode) -> Self {
        Self {
            off: UnsafeCell::new(0),
            ip,
        }
    }
    fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        let mut ip = self.ip.lock();
        let off = unsafe { &mut *self.off.get() };

        match ip.read(dst, *off, n) {
            // inode lock is held
            Ok(r) => {
                *off += r as u32;
                Ok(r)
            }
            Err(_) => Err(()),
        }
    }
    fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()> {
        // write a few blocks at a time to avoid exceeding the maximum
        // log transaction size, including i-node, indirect block,
        // allocation blocks, and 2 blocks of slop for non-aligned
        // writes. this really belongs lower down, since inode write()
        // might be writing a device like the console.
        let max = ((MAXOPBLOCKS - 1 - 1 - 2) / 2) * BSIZE;
        let mut i: usize = 0;
        let off = unsafe { &mut *self.off.get() };
        while i < n {
            let mut r: usize = 0;
            let mut n1 = n - i;
            if n1 > max {
                n1 = max
            }

            {
                LOG.begin_op();
                let mut guard = self.ip.lock();
                if let Ok(wbytes) = guard.write(src, *off, n1) {
                    *off += wbytes as u32;
                    r = wbytes;
                }
                LOG.end_op();
            }

            if r != n1 {
                // error from inode write
                break;
            }
            i += r;
        }

        if i == n {
            Ok(n)
        } else {
            Err(())
        }
    }
}

#[cfg(target_os = "none")]
impl VFile {
    fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        match self {
            VFile::Device(d) => d.read(dst, n),
            VFile::Inode(f) => f.read(dst, n),
            VFile::Pipe(p) => p.read(dst, n),
            _ => panic!("file read"),
        }
    }
    fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()> {
        match self {
            VFile::Device(d) => d.write(src, n),
            VFile::Inode(f) => f.write(src, n),
            VFile::Pipe(p) => p.write(src, n),
            _ => panic!("file write"),
        }
    }
    // Get metadata about file.
    // addr pointing to a struct stat.
    pub fn stat(&self, addr: VirtAddr) -> Result<(), ()> {
        let p = CPUS.my_proc().unwrap();
        let mut stat: Stat = Default::default();

        match self {
            VFile::Device(DNod { driver: _, ref ip }) | VFile::Inode(FNod { off: _, ref ip }) => {
                {
                    ip.lock().stat(&mut stat);
                }
                unsafe { p.either_copyout(addr, &stat) }
            }
            _ => Err(()),
        }
    }
}

#[cfg(target_os = "none")]
impl File {
    // Read from file.
    pub fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        if !self.readable {
            return Err(());
        }
        self.f.as_ref().unwrap().read(dst, n)
    }

    // Write to file.
    pub fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()> {
        if !self.writable {
            return Err(());
        }
        self.f.as_ref().unwrap().write(src, n)
    }
}

#[cfg(target_os = "none")]
impl Deref for File {
    type Target = Arc<VFile>;
    fn deref(&self) -> &Self::Target {
        self.f.as_ref().unwrap()
    }
}

#[cfg(target_os = "none")]
impl Drop for File {
    fn drop(&mut self) {
        let f = self.f.take().unwrap();
        if Arc::strong_count(&f) < 2 {
            panic!("file drop");
        }

        if Arc::strong_count(&f) == 2 {
            let mut guard = FTABLE.lock();
            // drop arc<vfile> in table
            for ff in guard.iter_mut() {
                match ff {
                    Some(vff) if Arc::ptr_eq(&f, vff) => {
                        ff.take(); // drop ref in table. ref count = 1;
                    }
                    _ => (),
                }
            }
        }

        // if ref count == 1
        match Arc::try_unwrap(f) {
            Ok(VFile::Inode(FNod { off: _, ip }) | VFile::Device(DNod { driver: _, ip })) => {
                LOG.begin_op();
                drop(ip);
                LOG.end_op();
            }
            _ => (),
        }
    }
}

// File Allocation Type Source
#[cfg(target_os = "none")]
pub enum FType<'a> {
    Node(&'a Path),
    Pipe(Pipe),
}

#[cfg(target_os = "none")]
impl FTable {
    // Allocate a file structure
    // Must be called inside transaction if FType == FType::Node.
    pub fn alloc<'a, 'b>(&'a self, opts: OMode, ftype: FType<'b>) -> Option<File> {
        let inner: Arc<VFile> = Arc::new(match ftype {
            FType::Node(path) => {
                let ip: Inode;
                let mut ip_guard: SleepLockGuard<'_, IData>;

                if opts.is_create() {
                    ip = create(path, IType::File, 0, 0)?;
                    ip_guard = ip.lock();
                } else {
                    (_, ip) = path.namei()?;
                    ip_guard = ip.lock();
                    if ip_guard.itype() == IType::Dir && !opts.is_rdonly() {
                        return None;
                    }
                }
                // todo
                match ip_guard.itype() {
                    IType::Device
                        if ip_guard.major() != Major::Invalid
                            && ip_guard.major() != Major::Null =>
                    {
                        let driver = DEVSW.get(ip_guard.major()).unwrap();
                        SleepLock::unlock(ip_guard);
                        VFile::Device(DNod { driver, ip })
                    }
                    IType::Dir | IType::File => {
                        if opts.is_trunc() && ip_guard.itype() == IType::File {
                            ip_guard.trunc();
                        }
                        SleepLock::unlock(ip_guard);
                        VFile::Inode(FNod::new(ip))
                    }
                    _ => return None,
                }
            }
            FType::Pipe(pi) => VFile::Pipe(pi),
        });

        let mut guard = self.lock();

        let mut empty: Option<&mut Option<Arc<VFile>>> = None;
        for f in guard.iter_mut() {
            match f {
                None if empty.is_none() => {
                    empty = Some(f);
                    break;
                }
                _ => continue,
            }
        }

        let f = empty?;
        f.replace(inner);
        Some(File {
            f: f.clone(), // ref count = 2
            readable: opts.is_read(),
            writable: opts.is_write(),
        })
    }
}

#[cfg(target_os = "none")]
pub struct DevSW {
    table: [SyncOnceCell<&'static dyn Device>; NDEV],
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for DevSW {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        for (count, v) in self.table.iter().enumerate() {
            if count != 0 {
                write!(f, ", ")?;
            }
            if let Some(&v) = v.get() {
                write!(f, "{:?}", v)?;
            } else {
                write!(f, "None")?;
            }
        }
        write!(f, "]")
    }
}

#[cfg(target_os = "none")]
impl DevSW {
    pub const fn new() -> Self {
        Self {
            table: array![SyncOnceCell::new(); NDEV],
        }
    }
    pub fn set(
        &self,
        devnum: Major,
        dev: &'static dyn Device,
    ) -> Result<(), &'static (dyn Device + 'static)> {
        self.table[devnum as usize].set(dev)
    }

    pub fn get(&self, devnum: Major) -> Option<&'static dyn Device> {
        match self.table[devnum as usize].get() {
            Some(&dev) => Some(dev),
            None => None,
        }
    }
}

// Device Major Number
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Major {
    Null = 0,
    Console = 1,
    Invalid,
}
impl Default for Major {
    fn default() -> Self {
        Self::Null
    }
}

impl Major {
    pub fn from_u16(bits: u16) -> Major {
        match bits {
            0 => Major::Null,
            1 => Major::Console,
            _ => Major::Invalid,
        }
    }
}
