use crate::{
    memlayout::{TRAMPOLINE, TRAPFLAME, UART0_IRQ, VIRTIO0_IRQ},
    plic,
    proc::{Cpus, ProcState, Process, CPUS, PROCS},
    riscv::{intr_get, intr_off, intr_on, r_sstatus, w_sip, w_sstatus, PGSIZE},
    spinlock::Mutex,
    trampoline::trampoline,
    uart::UART,
    virtio_disk::DISK,
};
use riscv::register::*;
use scause::{Exception, Interrupt, Trap};

extern "C" {
    fn uservec();
    fn userret();
    fn kernelvec();
}

#[derive(PartialEq)]
pub enum Intr {
    Timer,
    Device,
}

pub static TICKS: Mutex<usize> = Mutex::new(0, "time");

// set up to take exceptions and traps while in the kernel.
#[no_mangle]
pub fn inithart() {
    unsafe {
        stvec::write(kernelvec as usize, stvec::TrapMode::Direct);
    }
}

//
// handle an interrupt, exception, or system call from user space.
// called from trampoline.rs
//
#[no_mangle]
pub extern "C" fn usertrap() -> ! {
    assert!(
        sstatus::read().spp() == sstatus::SPP::User,
        "usertrap: not from user mode"
    );
    assert!(!intr_get(), "kerneltrap: interrupts enabled");

    // send interrupts and exceptions to kerneltrap().
    // since we're now in the kernel.
    unsafe {
        stvec::write(kernelvec as usize, stvec::TrapMode::Direct);
    }

    let p = CPUS.my_proc().unwrap();
    let data = unsafe { &mut (*p.data.get()) };
    let tf = unsafe { data.trapframe.unwrap().as_mut() };

    // save user program counter
    tf.epc = sepc::read();

    let mut which_dev = None;
    match scause::read().cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            // system call

            if p.inner.lock().killed {
                p.exit(-1);
            }

            // sepc points to the ecall instruction,
            // but we want to return to the next instruction.
            tf.epc += 4;

            // an interrupt will change sstatus &c registers,
            // so don't enable until done with those registers.
            intr_on();

            // todo syscall()
        }
        Trap::Interrupt(intr)
            if {
                which_dev = devintr(intr);
                which_dev.is_some()
            } => {}
        _ => {
            let mut inner = p.inner.lock();
            println!(
                "usertrap(): unexcepted scause {:?}, pid={:?}",
                scause::read().cause(),
                inner.pid
            );
            println!("            sepc={}, stval={}", sepc::read(), stval::read());
            inner.killed = true;
        }
    }

    if p.inner.lock().killed {
        p.exit(-1)
    }

    // give up the CPU if this is a timer interrupt.
    if Some(Intr::Timer) == which_dev {
        p.yielding()
    }

    unsafe { usertrap_ret() }
}

//
// return to user space
//
#[no_mangle]
pub unsafe extern "C" fn usertrap_ret() -> ! {
    let p = CPUS.my_proc().unwrap();

    // we're about to switch the destination of traps from
    // kerneltrap() to usertrap(), so turn off interrupts until
    // we're back in user space, where usertrap() is correct.
    intr_off();

    // send syscalls, interrupts, and exceptions to trampoline.rs
    stvec::write(
        TRAMPOLINE + (uservec as usize - trampoline as usize),
        stvec::TrapMode::Direct,
    );

    let data = &mut *p.data.get();

    // set up trapframe values that uservec will need when
    // the process next re-enters the kernel.
    let tf = data.trapframe.unwrap().as_mut();
    tf.kernel_satp = satp::read().bits();
    tf.kernel_sp = data.kstack + PGSIZE;
    tf.kernel_trap = usertrap as usize;
    tf.kernel_hartid = Cpus::cpu_id();

    // set up the registers that trampoline.rs's sret will use
    // to get to user sapce.

    // set S Previous Priviledge mode to User.
    sstatus::set_spp(sstatus::SPP::User); // clear SPP to 0 for user mode.
    sstatus::set_spie(); // enable interrupts in user mode.

    // set S Exception Program Counter Counter to the saved user pc.
    sepc::write(tf.epc);

    // tell trampoline.rs the user page table to switch to.
    let satp = data.uvm.as_ref().unwrap().as_satp();

    // jump to trampoline.rs at the top of memory, witch
    // switches to the user page table, restores user registers,
    // and switches to user mode with sret.
    let fn_0: usize = TRAMPOLINE + (userret as usize - trampoline as usize);
    let fn_0: extern "C" fn(usize, usize) -> ! = core::mem::transmute(fn_0);
    fn_0(TRAPFLAME, satp)
}

// interrupts and exceptions from kernel code go here via kernelvec,
// on whatever the current kernel stack is.
#[no_mangle]
pub extern "C" fn kerneltrap() {
    let mut which_dev = None;
    let sepc = sepc::read();
    let sstatus = sstatus::read();
    let scause = scause::read();
    let sstatus_bits = r_sstatus();

    assert!(
        sstatus.spp() == sstatus::SPP::Supervisor,
        "not from supervisor mode"
    );
    assert!(!intr_get(), "kerneltrap: interrupts enabled");

    match scause.cause() {
        Trap::Interrupt(intr)
            if {
                which_dev = devintr(intr);
                which_dev.is_none()
            } =>
        {
            println!("scause {:?}", scause.cause());
            println!("sepc={} stval={}", sepc::read(), stval::read());
            panic!("kerneltrap");
        }
        _ => {}
    }

    // give up the CPU if this is a timer interrupt.
    if Some(Intr::Timer) == which_dev {
        if let Some(p) = CPUS.my_proc() {
            if p.inner.lock().state == ProcState::RUNNING {
                p.yielding()
            }
        }
    }

    // the yielding() may have caused some traps to occur.
    // so restore trap registers for use by kernelvec.rs's sepc instruction.
    sepc::write(sepc);
    w_sstatus(sstatus_bits);
}

fn clockintr() {
    let mut ticks = TICKS.lock();
    *ticks += 1;
    PROCS.wakeup(&(*ticks) as *const _ as usize)
}

// check if it's an external interrupt or software interrupt,
// and handle it.
// returns Option<Intr>
// devintr() is safe because it is only called in the non-interruptable
// part of trap.rs.
fn devintr(intr: Interrupt) -> Option<Intr> {
    match intr {
        Interrupt::SupervisorExternal => {
            // this is a supervisor external interrupt, via PLIC.

            // irq indicates which device interrupted.
            let irq = plic::claim();

            if let Some(irq) = irq {
                match irq {
                    UART0_IRQ => UART.intr(),
                    VIRTIO0_IRQ => DISK.intr(),
                    _ => println!("unexpected interrupt irq={}", irq),
                }
                // the PLIC allows each device to raise at most one
                // interrupt at a time; tell the PLIC the device is
                // now allowed to interrupt again.
                plic::complete(irq);
            }

            Some(Intr::Device)
        }
        Interrupt::SupervisorSoft => {
            // software interrupt from a machine-mode timer interrupt,
            // forwarded by timervec in kernelvec.rs.

            if unsafe { Cpus::cpu_id() == 0 } {
                clockintr();
            }

            // acknowledge the software interrupt by clearing
            // the SSIP bit in sip.
            w_sip(sip::read().bits() & !2);
            Some(Intr::Timer)
        }
        _ => None,
    }
}
