# octox 

WIP. octox is an xv6-riscv like OS written in Rust.  

## Getting Started

### Requirements
* Install the rust toolchain in order to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* Add target(`rustup target add riscv64gc-unknown-none-elf`)
* Install qemu (Ubuntu example: `sudo apt install qemu-system-misc`)
* (option) Install gdb (Ubuntu example: `sudo apt install  gdb-multiarch`)

### Cargo

* Build: run `cargo build`.
* Run: run `cargo run`, then qemu will boot octox.  
  To exit, press `Ctrl+A` and `x`.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

This is a learning project for me, and I will not be accepting pull requests until I consider the implementation complete. Discussions and advice are welcome.