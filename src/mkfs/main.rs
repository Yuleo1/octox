use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process;
use mkfs::{fs::*, param::*, stat::*, defs::*};

const NINODES: usize = 200;

// Disk layout:
// [ boot block | sb block | log | inode blocks | free bit map | data blocks ]

const NBITMAP: usize = FSSIZE / (BSIZE * 8) + 1;
const NINODEBLOCKS: usize = NINODES / IPB + 1;
const NLOG: usize = LOGSIZE;
const NMETA: usize = 2 + NLOG + NINODEBLOCKS + NBITMAP;
const NBLOCKS: usize = FSSIZE - NMETA;

static ZEROS: [u8; BSIZE] = [0; BSIZE];

struct FsImg {
    sb: SuperBlock,
    img: File,
    freeinode: usize,
    freeblock: usize,
}

impl FsImg {
    fn new<P: AsRef<Path>>(sb: SuperBlock, path: P) -> Result<Self, std::io::Error> {
        Ok(Self {
            sb,
            img: OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .read(true)
                .open(path)?,
            freeinode: 1,
            freeblock: NMETA, // the first free block that we can allocate
        })
    }

    fn wsect(&mut self, sec: u32, buf: &[u8]) -> Result<(), std::io::Error> {
        let mut writer = BufWriter::new(&mut self.img);
        if writer.seek(SeekFrom::Start((sec as usize * BSIZE) as u64))?
            != (sec as usize * BSIZE) as u64
        {
            die("seek");
        }
        if writer.write(buf)? != BSIZE {
            die("write");
        }
        writer.flush()
    }

    fn winode(&mut self, inum: u32, ip: &DInode) -> Result<(), std::io::Error> {
        let mut buf = [0u8; BSIZE];
        let bn = self.sb.iblock(inum);
        self.rsect(bn, &mut buf)?;
        let (head, dinode_slice, _tail) = unsafe { buf.align_to_mut::<DInode>() };
        assert!(head.is_empty(), "Data was not aligned");
        *dinode_slice.get_mut(inum as usize % IPB).unwrap() = *ip;
        self.wsect(bn, &buf)
    }

    fn rinode(&self, inum: u32, ip: &mut DInode) -> Result<(), std::io::Error> {
        let mut buf = [0u8; BSIZE];
        let bn = self.sb.iblock(inum);
        self.rsect(bn, &mut buf)?;
        let (head, dinode_slice, _tail) = unsafe { buf.align_to::<DInode>() };
        assert!(head.is_empty(), "Data was not aligned");
        *ip = *dinode_slice.get(inum as usize % IPB).unwrap();
        Ok(())
    }

    fn rsect(&self, sec: u32, buf: &mut [u8]) -> Result<(), std::io::Error> {
        let mut reader = BufReader::new(&self.img);
        if reader.seek(SeekFrom::Start((sec as usize * BSIZE) as u64))?
            != (sec as usize * BSIZE) as u64
        {
            die("seek");
        }
        if reader.read(buf)? != BSIZE {
            die("read");
        }
        Ok(())
    }

    fn ialloc(&mut self, itype: IType) -> Result<u32, std::io::Error> {
        let inum = self.freeinode as u32;
        self.freeinode += 1;
        let mut din: DInode = Default::default();
        din.itype = (itype as u16).to_le();
        din.nlink = 1u16.to_le();
        din.size = 0;
        self.winode(inum, &din)?;
        Ok(inum)
    }

    fn balloc(&mut self, used: usize) -> Result<(), std::io::Error> {
        let mut buf = [0u8; BSIZE];

        println!("balloc: first {} blocks have benen allocated", used);
        assert!(used < BSIZE * 8);
        for i in 0..used {
            if let Some(elem) = buf.get_mut(i / 8) {
                *elem = *elem | (0x1 << (i % 8));
            }
        }
        println!("balloc: write bitmap block at sector {}", self.sb.bmapstart);
        self.wsect(u32::from_le(self.sb.bmapstart), &buf)
    }

    fn iappend(&mut self, inum: u32, data: &[u8]) -> Result<(), std::io::Error> {
        let mut din: DInode = Default::default();
        let mut buf = [0u8; BSIZE];
        let mut indirect = [0u32; NINDIRECT];
        let mut x: u32 = 0;
        let mut p: usize = 0;
        let mut n = data.len();

        self.rinode(inum, &mut din)?;
        let mut off = u32::from_le(din.size) as usize;
        println!("append inum {} at off {} sz {}", inum, off, n);

        while n > 0 {
            let fbn = off / BSIZE;
            assert!(fbn < MAXFILE);
            if fbn < NDIRECT {
                if u32::from_le(din.addrs[fbn]) == 0 {
                    din.addrs[fbn] = (self.freeblock as u32).to_le();
                    self.freeblock += 1;
                }
                x = u32::from_le(din.addrs[fbn]);
            } else {
                if u32::from_le(din.addrs[NDIRECT]) == 0 {
                    din.addrs[NDIRECT] = (self.freeblock as u32).to_le();
                    self.freeblock += 1;
                }
                self.rsect(u32::from_le(din.addrs[NDIRECT]), mkfs_as_bytes_mut(&mut indirect))?;
                if u32::from_le(indirect[fbn - NDIRECT]) == 0 {
                    indirect[fbn - NDIRECT] = (self.freeblock as u32).to_le();
                    self.freeblock += 1;
                    self.wsect(u32::from_le(din.addrs[NDIRECT]), mkfs_as_bytes(&indirect))?;
                }
                x = u32::from_le(indirect[fbn - NDIRECT]);
            }
            let n1 = std::cmp::min(n, (fbn + 1) * BSIZE - off);
            self.rsect(x, &mut buf)?;
            buf[off - (fbn * BSIZE)..(off - (fbn * BSIZE) + n1)]
                .copy_from_slice(&data[p..(p + n1)]);
            self.wsect(x, &buf)?;
            n -= n1;
            off += n1;
            p += n1;
        }
        din.size = (off as u32).to_le();
        self.winode(inum, &din)
    }
}

fn main() -> std::io::Result<()> {
    let mut buf = [0u8; BSIZE];

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: mkfs fs.img files...");
        process::exit(1);
    }

    assert!(BSIZE % core::mem::size_of::<DInode>() == 0);
    assert!(BSIZE % core::mem::size_of::<DirEnt>() == 0);

    let sb = SuperBlock {
        magic: FSMAGIC.to_le(),
        size: (FSSIZE as u32).to_le(),
        nblocks: (NBLOCKS as u32).to_le(),
        ninodes: (NINODES as u32).to_le(),
        nlog: (NLOG as u32).to_le(),
        logstart: 2u32.to_le(),
        inodestart: ((2 + NLOG) as u32).to_le(),
        bmapstart: ((2 + NLOG + NINODEBLOCKS) as u32).to_le(),
    };

    let mut fsimg = FsImg::new(sb, &args[1])?;

    println!("nmeta {} (boot, super, log blocks {} inode blocks {}, bitmap blocks {}) blocks {} total {}", NMETA, NLOG, NINODEBLOCKS, NBITMAP, NBLOCKS, FSSIZE);

    for sec in 0..FSSIZE {
        fsimg.wsect(sec as u32, &ZEROS)?;
    }

    let (head, sb_slice, _tail) = unsafe { buf.align_to_mut::<SuperBlock>() };
    assert!(head.is_empty(), "Data was not aligned");
    *sb_slice.get_mut(0).unwrap() = sb;
    fsimg.wsect(1, &buf)?;

    let rootino = fsimg.ialloc(IType::Dir)?;
    assert!(rootino == ROOTINO);

    let mut de: DirEnt = Default::default();
    de.inum = (rootino as u16).to_le();
    de.name[..".".len()].copy_from_slice(mkfs_as_bytes(&"."));
    fsimg.iappend(rootino, mkfs_as_bytes(&de))?;

    let mut de: DirEnt = Default::default();
    de.inum = (rootino as u16).to_le();
    de.name[.."..".len()].copy_from_slice(mkfs_as_bytes(&".."));
    fsimg.iappend(rootino, mkfs_as_bytes(&de))?;

    for path in args[2..]
        .iter()
        .map(|p| Path::new(p))
        .filter(|p| p.exists())
    {
        // Skip leading _ in name when writing to file system.
        // The binaries are named _rm, _cat, etc. to keep the
        // build operating system from trying to execute them
        // in place of system binaries like rm and cat.
        let shortname = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_start_matches("_");
        assert!(shortname.len() < 14);

        let mut fd = File::open(path)?;

        let inum = fsimg.ialloc(IType::File)?;

        let mut de: DirEnt = Default::default();
        de.inum = (inum as u16).to_le();
        de.name[..shortname.len()].copy_from_slice(mkfs_as_bytes(&shortname));
        fsimg.iappend(rootino, mkfs_as_bytes(&de))?;

        while fd.read(&mut buf)? > 0 {
            fsimg.iappend(inum, &buf)?;
        }
    }

    // fix size of root inode dir
    let mut din: DInode = Default::default();
    fsimg.rinode(rootino, &mut din)?;
    let mut off = u32::from_le(din.size) as usize;
    off = (off / BSIZE + 1) * BSIZE;
    din.size = (off as u32).to_le();
    fsimg.winode(rootino, &din)?;

    fsimg.balloc(fsimg.freeblock)
}

fn die(str: &str) -> ! {
    eprintln!("{}", str);
    std::process::exit(1);
}

// On-disk inode structure for mkfs
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct DInode {
    itype: u16,                // File type
    major: u16,                // Major Device Number (T_DEVICE only)
    minor: u16,                // Minor Device Number (T_DEVICE only)
    nlink: u16,                // Number of links to inode in file system
    size: u32,                 // Size of data (bytes)
    addrs: [u32; NDIRECT + 1], // Data block address
}

fn mkfs_as_bytes<T: ?Sized>(refs: &T) -> &[u8] {
    unsafe {
        as_bytes(refs)
    }
}

fn mkfs_as_bytes_mut<T: ?Sized>(refs: &mut T) -> &mut [u8] {
    unsafe {
        as_bytes_mut(refs)
    }
}