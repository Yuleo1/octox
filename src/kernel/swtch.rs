use crate::proc::Context;

pub extern "C" fn swtch(old: &mut Context, new: &Context) {
    unsafe {
        asm!(
            "sd ra, 0({0})",
            "sd sp, 8({0})",
            "sd s0, 16({0})",
            "sd s1, 24({0})",
            "sd s2, 32({0})",
            "sd s3, 40({0})",
            "sd s4, 48({0})",
            "sd s5, 56({0})",
            "sd s6, 64({0})",
            "sd s7, 72({0})",
            "sd s8, 80({0})",
            "sd s9, 88({0})",
            "sd s10, 96({0})",
            "sd s11, 104({0})",

            "ld ra, 0({1})",
            "ld sp, 8({1})",
            "ld s0, 16({1})",
            "ld s1, 24({1})",
            "ld s2, 32({1})",
            "ld s3, 40({1})",
            "ld s4, 48({1})",
            "ld s5, 56({1})",
            "ld s6, 64({1})",
            "ld s7, 72({1})",
            "ld s8, 80({1})",
            "ld s9, 88({1})",
            "ld s10, 96({1})",
            "ld s11, 104({1})",
            in(reg) old,
            in(reg) new
        );
    }
}
