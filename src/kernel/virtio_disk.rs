use crate::{
    bio::Buf,
    fs::BSIZE,
    memlayout::VIRTIO0,
    proc::{Process, CPUS, PROCS},
    riscv::{PGSHIFT, PGSIZE},
    spinlock::Mutex,
};
use alloc::sync::Arc;
use bitflags::bitflags;
use core::{
    convert::TryInto,
    sync::atomic::{fence, Ordering},
};

//
// driver for qemu's virtio disk device.
// uses qemu's mmio interface to virtio.
// qemu presents a "legacy" virtio interface.
//
// qemu ... -drive file=fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

pub static DISK: Mutex<Disk> = Mutex::new(Disk::new(), "virtio_disk");

// Memory mapped IO registers.
#[repr(usize)]
enum VirtioMMIO {
    // 0x74726976
    MagicValue = 0x00,
    // version; 1 is legacy
    Version = 0x004,
    // device type; 1 is net, 2 is disk
    DeviceId = 0008,
    // 0x554d4552
    VenderId = 0x00c,
    DeviceFeatures = 0x010,
    DriverFeatures = 0x020,
    // page size for PFN, write-only
    GuestPageSize = 0x028,
    // select queue, write-only
    QueueSel = 0x030,
    // max size of current queue, read-only
    QueueNumMax = 0x034,
    // size of current queue, write-only
    QueueNum = 0x038,
    // physical page number of queue, read/write
    QueuePfn = 0x040,
    // write-only
    QueueNotify = 0x050,
    // read-only
    InterruptStatus = 0x060,
    // write-only
    InterruptAck = 0x064,
    // read/write
    Status = 0x070,
}

impl VirtioMMIO {
    fn read(self) -> u32 {
        unsafe { core::ptr::read_volatile((VIRTIO0 + self as usize) as *const u32) }
    }
    unsafe fn write(self, data: u32) {
        core::ptr::write_volatile((VIRTIO0 + self as usize) as *mut u32, data);
    }
}

bitflags! {
    // Status register bits, from qemu virtio_config.h
    struct VirtioStatus: u32 {
        const ACKNOWLEDGE = 0b0001;
        const DRIVER = 0b0010;
        const DRIVER_OK = 0b0100;
        const FEATURES_OK = 0b1000;
    }
}

bitflags! {
    // Device feature bits
    struct VirtioFeatures: u32 {
        // Disk is read-only
        const BLK_F_RO = 1 << 5;
        // Supports scsi command passthru
        const BLK_F_SCSI = 1 << 7;
        // Writeback mode available in config
        const BLK_F_CONFIG_WCE = 1 << 11;
        // support more than one vq
        const BLK_F_MQ = 1 << 12;
        const F_ANY_LAYOUT = 1 << 27;
        const RING_F_INDIRECT_DESC = 1 << 28;
        const RING_F_EVENT_IDX = 1 << 29;
    }
}

// this many virtio descriptors.
// must be a power of 2.
const NUM: usize = 8;

#[repr(C, align(4096))]
pub struct Disk {
    // This struct consists of two contiguous pages of page-aligned
    // physical memory, and is divided into three regions (descriptors,
    // avail, and used), as explained in Section 2.6 of the vertio
    // specification for the legacy interface.
    // https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf

    // first page
    pad1: PadPGA,
    // the first region of the first page is a set (not a ring) of
    // DMA descriptors, with which driver tells the device where to
    // read and write individual disk operations. there are NUM
    // descriptors. most commands consist of a "chain" (a linked list)
    // of a cuple of these descriptors.
    desc: [VirtqDesc; NUM],
    // next is a ring in which the driver writes descriptor number
    // that the driver would like the device to process. it only
    // includes the head descriptor of each chain. the ring has
    // NUM elements.
    avail: VirtqAvail,

    // second page
    pad2: PadPGA,
    // finally a ring in which the device writes descriptor numbers that
    // the device has finished processing (just the head of each chain).
    // there are NUM used ring entries
    used: VirtqUsed,

    // end
    pad3: PadPGA,
    // our own book-keeping
    free: [bool; NUM], // is a descriptor free ?
    used_idx: u16,     // we've looked this far in used[2..NUM].

    // track info aboud in-flight operations,
    // for use when completion interrupt arrives.
    // indexed by first descriptor index of chain.
    info: [Info; NUM],

    // disk command handlers.
    // one-for-one with descriptors, for convenience.
    ops: [VirtioBlkReq; NUM],
}

// zero-sized struct for a page of page-aligned physical memory.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(4096))]
struct PadPGA();

impl PadPGA {
    const fn new() -> Self {
        Self()
    }
}

// a single descriptor, from the spc
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: VirtqDescFlags,
    next: u16,
}

bitflags! {
    struct VirtqDescFlags: u16 {
        const FREED = 0b00;
        // chained with another descriptor
        const NEXT = 0b01;
        // device writes (vs read)
        const WRITE = 0b10;
    }
}

impl VirtqDesc {
    const fn new() -> Self {
        Self {
            addr: 0,
            len: 0,
            flags: VirtqDescFlags::FREED,
            next: 0,
        }
    }
}

// the (entire) avail ring, from the spc
#[derive(Debug, Clone, Copy)]
#[repr(C, align(2))]
struct VirtqAvail {
    flags: u16,       // always zero,
    idx: u16,         // driver will write ring[idx] next
    ring: [u16; NUM], // descriptor numbers of chain heads
    unused: u16,
}

impl VirtqAvail {
    const fn new() -> Self {
        Self {
            flags: 0,
            idx: 0,
            ring: [0; NUM],
            unused: 0,
        }
    }
}

// one entry in the "used" ring, with which the
// device tells the driver about completed requests.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct VirtqUsedElem {
    id: u32, // index of start of completes descriptor chain
    len: u32,
}

impl VirtqUsedElem {
    const fn new() -> Self {
        Self { id: 0, len: 0 }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, align(4))]
struct VirtqUsed {
    flags: u16, // always zero
    idx: u16,   // device increments when it adds a ring[] entry
    ring: [VirtqUsedElem; NUM],
}

impl VirtqUsed {
    const fn new() -> Self {
        Self {
            flags: 0,
            idx: 0,
            ring: [VirtqUsedElem::new(); NUM],
        }
    }
}

// track info aboud in-flight operations,
// for use when completion interrupt arrives.
// indexed by first descriptor index of chain.
#[derive(Clone, Copy)]
#[repr(C)]
struct Info {
    buf: Option<&'static Arc<Buf>>,
    status: u8,
}

impl Info {
    const fn new() -> Self {
        Self {
            buf: None,
            status: 0,
        }
    }
}

// these are specific to virtio block devices, e.g. disks,
// described in Section 5.2 of the spec.

pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;

// the format of the first descriptor in a disk request.
// to be followed by two more descriptors containing
// the block, and a one-byte status.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct VirtioBlkReq {
    type_: u32, // VIRTIO_BLK_T_IN or ..._OUT
    reserved: u32,
    sector: u64,
}

impl VirtioBlkReq {
    const fn new() -> Self {
        Self {
            type_: 0,
            reserved: 0,
            sector: 0,
        }
    }
}

impl Disk {
    const fn new() -> Self {
        Self {
            pad1: PadPGA::new(),
            desc: [VirtqDesc::new(); NUM],
            avail: VirtqAvail::new(),
            pad2: PadPGA::new(),
            used: VirtqUsed::new(),
            pad3: PadPGA::new(),
            free: [false; NUM],
            used_idx: 0,
            info: [Info::new(); NUM],
            ops: [VirtioBlkReq::new(); NUM],
        }
    }
    unsafe fn init(&mut self) {
        let mut status: VirtioStatus = VirtioStatus::empty();

        if VirtioMMIO::MagicValue.read() != 0x74726976
            || VirtioMMIO::Version.read() != 1
            || VirtioMMIO::DeviceId.read() != 2
            || VirtioMMIO::VenderId.read() != 0x554d4551
        {
            panic!("could not find virtio disk");
        }

        status.insert(VirtioStatus::ACKNOWLEDGE);
        VirtioMMIO::Status.write(status.bits());
        status.insert(VirtioStatus::DRIVER);
        VirtioMMIO::Status.write(status.bits());

        // negotiate features
        let features = VirtioFeatures::from_bits_truncate(VirtioMMIO::DeviceFeatures.read())
            - (VirtioFeatures::BLK_F_RO
                | VirtioFeatures::BLK_F_SCSI
                | VirtioFeatures::BLK_F_CONFIG_WCE
                | VirtioFeatures::BLK_F_MQ
                | VirtioFeatures::F_ANY_LAYOUT
                | VirtioFeatures::RING_F_EVENT_IDX
                | VirtioFeatures::RING_F_INDIRECT_DESC);
        VirtioMMIO::DriverFeatures.write(features.bits());

        // tell device that feature negotiation is complete.
        status.insert(VirtioStatus::FEATURES_OK);
        VirtioMMIO::Status.write(status.bits());

        // tell device we're completely ready.
        status.insert(VirtioStatus::DRIVER_OK);
        VirtioMMIO::Status.write(status.bits());

        VirtioMMIO::GuestPageSize.write(PGSIZE as _);

        // initialize queue 0.
        VirtioMMIO::QueueSel.write(0);
        let max = VirtioMMIO::QueueNumMax.read();
        assert!(max != 0, "virtio disk has no queue 0");
        assert!(max >= NUM as u32, "virtio disk max queue too short");
        VirtioMMIO::QueueNum.write(NUM as _);
        VirtioMMIO::QueuePfn.write((self as *const _ as usize >> PGSHIFT) as _);

        // all NUM descriptors start out unused.
        self.free.iter_mut().for_each(|f| *f = true);

        // plic.rs and trap.rs arrange for interrupts from VIRTIO0_IRQ.
    }

    // find a free descriptor, mark it non-free, return its index.
    fn alloc_desc(&mut self) -> Option<usize> {
        self.free
            .iter_mut()
            .enumerate()
            .filter(|(_, v)| **v)
            .take(1)
            .map(|(i, v)| {
                *v = false;
                i
            })
            .next()
    }

    // mark a descriptor as free.
    fn free_desc(&mut self, i: usize) {
        assert!(i < NUM, "free_dec 1");
        assert!(!self.free[i], "free_desc 2");
        self.desc[i].addr = 0;
        self.desc[i].len = 0;
        self.desc[i].flags = VirtqDescFlags::empty();
        self.desc[i].next = 0;
        self.free[i] = true;
        PROCS.wakeup(&self.free[0] as *const _ as usize);
    }

    // free a chain of descriptors.
    fn free_chain(&mut self, mut i: usize) {
        loop {
            let desc = self.desc.get(i).unwrap();
            let flag = desc.flags;
            let nxt = desc.next;
            self.free_desc(i);
            if !(flag & VirtqDescFlags::NEXT).is_empty() {
                i = nxt as usize;
            } else {
                break;
            }
        }
    }

    // allocate three descriptors (they need not be contiguous).
    // disk transfers always use three descriptors.
    fn alloc3_desc(&mut self, idx: &mut [usize; 3]) -> Result<(), ()> {
        for (i, idxi) in idx.iter_mut().enumerate() {
            match self.alloc_desc() {
                Some(ix) => *idxi = ix,
                None => {
                    for j in 0..i {
                        self.free_desc(j)
                    }
                    return Err(());
                }
            }
        }
        Ok(())
    }
}

impl Mutex<Disk> {
    pub fn rw(&self, b: &'static Arc<Buf>, raw_data: *const [u8; BSIZE], write: bool) {
        let sector = b.ctrl.read().blockno as usize * (BSIZE / 512);

        let mut guard = self.lock();
        let p = CPUS.my_proc().unwrap();

        // the spec's Section 5.2 says that legacy block operations use
        // three descriptors: one for type/reserved/sector, one for the
        // data, one for a 1-byte status result.

        // allocate the three descriptors.
        let mut idx: [usize; 3] = [0; 3];
        loop {
            if guard.alloc3_desc(&mut idx).is_ok() {
                break;
            }
            guard = p.sleep(&guard.free[0] as *const _ as usize, guard);
        }

        // format the three descriptors.
        // qemu's virtio-blk.c reads them.

        let buf0 = guard.ops.get_mut(idx[0]).unwrap();
        buf0.type_ = if write {
            VIRTIO_BLK_T_OUT // write the disk
        } else {
            VIRTIO_BLK_T_IN // read the disk
        };
        buf0.reserved = 0;
        buf0.sector = sector as u64;

        guard.desc[idx[0]].addr = buf0 as *mut _ as u64;
        guard.desc[idx[0]].len = core::mem::size_of::<VirtioBlkReq>().try_into().unwrap();
        guard.desc[idx[0]].flags = VirtqDescFlags::NEXT;
        guard.desc[idx[0]].next = idx[1].try_into().unwrap();

        guard.desc[idx[1]].addr = raw_data as u64;
        guard.desc[idx[1]].len = BSIZE.try_into().unwrap();
        guard.desc[idx[1]].flags = if write {
            VirtqDescFlags::empty() // device reads b->data
        } else {
            VirtqDescFlags::WRITE // device writes b->data
        };
        guard.desc[idx[1]].flags |= VirtqDescFlags::NEXT;
        guard.desc[idx[1]].next = idx[2].try_into().unwrap();

        guard.info[idx[0]].status = 0xff; // device writes 0 on success
        guard.desc[idx[2]].addr = &mut guard.info[idx[0]].status as *mut _ as u64;
        guard.desc[idx[2]].len = 1;
        guard.desc[idx[2]].flags = VirtqDescFlags::WRITE; // device write the status
        guard.desc[idx[2]].next = 0;

        // record struct buf for intr()
        b.ctrl.write().disk = true;
        guard.info[idx[0]].buf.replace(b);

        // tell the device the first index in our chain of decriptors.
        let i = guard.avail.idx as usize % NUM;
        guard.avail.ring[i] = idx[0].try_into().unwrap();

        fence(Ordering::SeqCst);

        // tell the device another avail ring entry is available.
        guard.avail.idx += 1; // not % NUM ...

        fence(Ordering::SeqCst);

        unsafe {
            VirtioMMIO::QueueNotify.write(0); // value is queue number
        }

        // wait for intr() to say request has finished.
        while b.ctrl.read().disk {
            guard = p.sleep(Arc::as_ptr(&b) as usize, guard);
        }

        guard.info[idx[0]].buf.take();
        guard.free_chain(idx[0]);
    }

    pub fn intr(&self) {
        let mut guard = self.lock();
        // the device won't raise another interrupt until we tell it
        // we've seen this interrupt, which the following line does.
        // this may race with the device writing new entries to
        // the "used" ring, in which case we may process the new
        // completion entries in this interrupt, and have nothing to do
        // in the next interrup, whish is harmless.
        let intr_stat = VirtioMMIO::InterruptStatus.read();
        unsafe {
            VirtioMMIO::InterruptAck.write(intr_stat & 0x3);
        }

        fence(Ordering::SeqCst);

        // the device increments disk.used.idx when it
        // adds an entry to the used ring.
        while guard.used_idx != guard.used.idx {
            fence(Ordering::SeqCst);
            let id = guard.used.ring[guard.used_idx as usize % NUM].id as usize;

            if guard.info[id].status != 0 {
                panic!("disk intr status");
            }

            let b = guard.info[id].buf.unwrap();
            b.ctrl.write().disk = false; // disk is done with buf
            PROCS.wakeup(Arc::as_ptr(&b) as usize);

            guard.used_idx += 1;
        }
    }
}

pub fn init() {
    unsafe {
        DISK.get_mut().init();
    }
}
