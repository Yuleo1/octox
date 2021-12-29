use crate::start::start;

#[link_section = ".entry"]
#[no_mangle]
pub unsafe extern "C" fn _entry() -> ! {
    // set up stack for Rust.
    // stack0 is declared in kernel/src/start.rs
    // with a 4096-byte stack per CPU.
    // sp = stack0 + (hartid * 4096)
    asm!(
        "la sp, STACK0",
        "li a0, 4096",
        "csrr a1, mhartid",
        "addi a1, a1, 1",
        "mul a0, a0, a1",
        "add sp, sp, a0",
    );

    start()
}
