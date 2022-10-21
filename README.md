# octox 

octox is a Unix-like operating system inspired by xv6-riscv.  
octox loosely follows the structure and style of xv6, but is implemented in pure Rust.

## Getting Started

### Requirements
* Install the rust toolchain to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* Install `qemu-system-riscv`
* (option) Install `gdb-multiarch`

### Build and Run

* Clone this project & enter: `git clone ... && cd octox`
* Build: `cargo build`.
* Run: `cargo run`, then qemu will boot octox.  
  To exit, press `Ctrl+a` and `x`.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

This is a learning project for me, and I will not be accepting pull requests until I consider the implementation complete. However, discussions and advice are welcome.
