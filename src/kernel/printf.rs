use crate::console;
use crate::spinlock::Mutex;
use core::fmt;
use core::panic;
use core::sync::atomic::{AtomicBool, Ordering};

pub static PR: Pr = Pr {
    writer: Mutex::new(Writer, "pr"),
    panicked: AtomicBool::new(false),
};

// lock to avoid interleaving concurrent println!'s.
// Pr struct is slightly different, i.e.,
// it is not wrapped in a Mutex
// Because we need another field(locking),
// This trick can make `panic` print something to the console quicker.
pub struct Pr {
    writer: Mutex<Writer>,
    panicked: AtomicBool,
}

impl Pr {
    pub fn panicked(&self) -> &AtomicBool {
        &self.panicked
    }
}

struct Writer;

impl Writer {
    fn print(&self, c: u8) {
        console::putc(c)
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.print(byte);
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments<'_>) {
    use fmt::Write;

    if !PR.panicked.load(Ordering::Relaxed) {
        PR.writer.lock().write_fmt(args).expect("_print: error");
    } else {
        // for panic!
        unsafe {
            PR.writer.get_mut().write_fmt(args).expect("_print: error");
        }
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::printf::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    ($fmt:expr) => {
        print!(concat!($fmt, "\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        print!(concat!($fmt, "\n"), $($arg)*)
    };
}

#[panic_handler]
fn panic(info: &panic::PanicInfo<'_>) -> ! {
    PR.panicked.store(true, Ordering::Relaxed);
    crate::println!("{}", info);
    loop {}
}
