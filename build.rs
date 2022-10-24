#![feature(variant_count)]
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

// get enum SysCallNum
include!("src/kernel/sysctbl.rs");

impl SysCallNum {
    fn into_enum_iter() -> std::vec::IntoIter<SysCallNum> {
        (0..core::mem::variant_count::<SysCallNum>())
            .map(|i| SysCallNum::from_usize(i + 1).unwrap())
            .collect::<Vec<SysCallNum>>()
            .into_iter()
    }
}

fn main() {
    let cargo_path = env::var("CARGO").expect("Missing CARGO environment variable");
    let cargo = Path::new(&cargo_path);

    let manifest_dir_path =
        env::var("CARGO_MANIFEST_DIR").expect("Missing CARGO_MANIFEST_DIR environment variable");
    let manifest_dir = Path::new(&manifest_dir_path);
    let current_dir = env::current_dir().expect("Couldn't get current directory");
    let target_dir_rel = manifest_dir.join("target");
    let target_dir = current_dir.join(target_dir_rel);

    // Build sub projects

    // Build mkfs: target = host
    let mkfs_target_triple = env::var("HOST").expect("missing host triple");
    let mkfs_dir = manifest_dir.join("src").join("mkfs");
    build_subproject(&mkfs_dir, &target_dir, cargo, &mkfs_target_triple);

    // Build user programs: target = riscv64gc-unknown-none-elf
    // generate sysctbl.rs & usys.rs
    // create sysctbl.rs
    let mut sysctbl_rs = File::create(manifest_dir.join("src").join("user").join("sysctbl.rs"))
        .expect("couldn't create src/user/sysctbl.rs");
    sysctbl_rs
        .write_all(
            concat!(
                "// Created by build.rs\n\n\n",
                include_str!("src/kernel/sysctbl.rs")
            )
            .as_bytes(),
        )
        .expect("src/user/sysctbl.rs: write error");
    // create usys.rs
    let mut usys_rs = File::create(manifest_dir.join("src").join("user").join("usys.rs"))
        .expect("cloudn't create src/user/usys.rs");
    usys_rs
        .write_all(
            "// Created by build.rs\n\
            use crate::sysctbl::*;\n\
            use core::arch::asm\n\n\n"
                .as_bytes(),
        )
        .expect("src/user/usys.rs: write error");
    for syscall in SysCallNum::into_enum_iter() {
        let fn_name = format!("{:?}", syscall)
            .strip_prefix("Sys")
            .unwrap()
            .to_lowercase();
        usys_rs
            .write_fmt(format_args!(
                r#"#[naked]
#[no_mangle]
pub fn {} -> ! {{
    unsafe {{
        asm!(
            "li a7, {{syscall}}",
            "ecall",
            "ret",
            syscall = const SysCallNum::{:?} as usize,
            optoins(noreturn),
        );
    }}
}}

"#,
                fn_name, syscall
            ))
            .expect("src/user/usys.rs: write error");
    }

    //let target_triple = env::var("TARGET").expect("missing target triple");
    //let user_dir = manifest_dir.join("user");
    //build_subproject(&user_dir, &target_dir, cargo, &target_triple);
    // let uprogs = todo

    // Build fs.img
    let mut mkfs_cmd = Command::new(
        target_dir
            .join("mkfs")
            .join(mkfs_target_triple)
            .join("release")
            .join("mkfs"),
    );
    mkfs_cmd.current_dir(target_dir);
    mkfs_cmd.arg("fs.img").arg("../README.md");
    // .args(uprogs);
    mkfs_cmd.status().expect("mkfs failed!");

    // linker script for kernel
    println!("cargo:rustc-link-arg=-Tsrc/kernel/kernel.ld");
}

fn build_subproject(subproject_dir: &Path, root_target_dir: &Path, cargo: &Path, triple: &str) {
    println!("cargo:rerun-if-changed={}", &subproject_dir.display());

    let subproject_name = subproject_dir
        .file_stem()
        .expect("Couldn't get subproject name")
        .to_str()
        .expect("Subproject Name is not valid UTF-8");
    let target_dir = root_target_dir.join(&subproject_name);

    let mut build_cmd = Command::new(cargo);
    build_cmd.current_dir(&subproject_dir);
    build_cmd
        .arg("build")
        .arg("--release")
        .arg(format!("--target-dir={}", &target_dir.display()))
        .arg("--target")
        .arg(&triple);
    let build_status = build_cmd.status().expect("Subcrate build failed!");
    assert!(build_status.success(), "Subcrate build failed!");
}
