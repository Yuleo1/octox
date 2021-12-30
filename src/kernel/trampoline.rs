use core::hint::unreachable_unchecked;
use core::arch::asm;

// code to switch between user and kernel space.
//
// this code is mapped at same virtual address
// (TRAMPOLINE) in user and kernel space so that
// is continues to work when it switches page tables.
//
// kernel.ld causes this to be aligned
// to a page boundary.
#[link_section = "trampsec"]
#[no_mangle]
pub unsafe extern "C" fn trampoline() -> ! {
    asm!(".align 4");

    #[link_section = "trampsec"]
    #[no_mangle]
    pub unsafe extern "C" fn uservec() -> ! {
        // trap.rs sets stvec to point here, so
        // traps from user space start here,
        // in supervisor mode, but with a
        // user page table.
        //
        // sscratch points to where the process's p->trampframe is
        // mapped into user space, at TRAPFRAME.

        // swap a0 and sscratch
        // so that a0 is TRAPFRAME
        asm!("csrrw a0, sscratch, a0");

        // save the user registers in TRAPFRAME
        asm!(
            "sd ra, 40(a0)",
            "sd sp, 48(a0)",
            "sd gp, 56(a0)",
            "sd tp, 64(a0)",
            "sd t0, 72(a0)",
            "sd t1, 80(a0)",
            "sd t2, 88(a0)",
            "sd s0, 96(a0)",
            "sd s1, 104(a0)",
            "sd a1, 120(a0)",
            "sd a2, 128(a0)",
            "sd a3, 136(a0)",
            "sd a4, 144(a0)",
            "sd a5, 152(a0)",
            "sd a6, 160(a0)",
            "sd a7, 168(a0)",
            "sd s2, 176(a0)",
            "sd s3, 184(a0)",
            "sd s4, 192(a0)",
            "sd s5, 200(a0)",
            "sd s6, 208(a0)",
            "sd s7, 216(a0)",
            "sd s8, 224(a0)",
            "sd s9, 232(a0)",
            "sd s10, 240(a0)",
            "sd s11, 248(a0)",
            "sd t3, 256(a0)",
            "sd t4, 264(a0)",
            "sd t5, 272(a0)",
            "sd t6, 280(a0)",
        );

        // save the user a0 in p->trapframe->a0
        asm!("csrr t0, sscratch", "sd t0, 112(a0)",);

        // restore the kernel stack pointer from p->trapframe->kernel_sp
        asm!("ld sp, 8(a0)");

        // make tp hold the current hartid, from p->trapframe->kernel_hartid
        asm!("ld tp, 32(a0)");

        // load the address of usertrap(), p->trapframe->kernel_trap
        asm!("ld t0, 16(a0)");

        // restore kernel page table from p->trapframe->kernel_satp
        asm!("ld t1, 0(a0)", "csrw satp, t1", "sfence.vma zero, zero",);

        // a0 is no longer valid, since the kernel page
        // table does not specially map p->tf.

        // jump to usertrap(), which does not return
        asm!("jr t0");

        unreachable_unchecked()
    }

    #[link_section = "trampsec"]
    #[no_mangle]
    pub unsafe extern "C" fn userret(trapframe: usize, pagetable: usize) -> ! {
        // userret(TRAPFRAME, pagetable)
        // switch from kernel to user.
        // usertrap_ret() calls here.
        // a0: TRAPFRAME, in user space table.
        // a1: user page table, for satp.

        // switch to the user page table.
        asm!(
            "csrw satp, {0}",
            "sfence.vma zero, zero",
            in(reg) pagetable,
        );

        // put the saved user a0 in sscratch, so we
        // can swap it with our a0 (TRAPFRAME) in the last step.
        asm!(
            "ld t0, 112({0})",
            "csrw sscratch, t0",
            in(reg) trapframe,
        );

        // restore all but a0 from TRAPFRAME
        asm!(
            "ld ra, 40({0})",
            "ld sp, 48({0})",
            "ld gp, 56({0})",
            "ld tp, 64({0})",
            "ld t0, 72({0})",
            "ld t1, 80({0})",
            "ld t2, 88({0})",
            "ld s0, 96({0})",
            "ld s1, 104({0})",
            "ld a1, 120({0})",
            "ld a2, 128({0})",
            "ld a3, 136({0})",
            "ld a4, 144({0})",
            "ld a5, 152({0})",
            "ld a6, 160({0})",
            "ld a7, 168({0})",
            "ld s2, 176({0})",
            "ld s3, 184({0})",
            "ld s4, 192({0})",
            "ld s5, 200({0})",
            "ld s6, 208({0})",
            "ld s7, 216({0})",
            "ld s8, 224({0})",
            "ld s9, 232({0})",
            "ld s10, 240({0})",
            "ld s11, 248({0})",
            "ld t3, 256({0})",
            "ld t4, 264({0})",
            "ld t5, 272({0})",
            "ld t6, 280({0})",
            in(reg) trapframe,
        );

        // restore user a0, and save TRAPFRAME in sscratch
        asm!("csrrw a0, sscratch, a0",);

        // return to user mode and user pc.
        // usertrap_ret() set up sstatus and sepc.
        asm!("sret");
        unreachable_unchecked()
    }

    unreachable_unchecked()
}
