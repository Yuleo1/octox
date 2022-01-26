use std::env;
use std::path::Path;
use std::process::Command;

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
    //let target_triple = env::var("TARGET").expect("missing target triple");
    //let user_dir = manifest_dir.join("user");
    //build_subproject(&user_dir, &target_dir, cargo, &target_triple);
    // let uprogs = todo

    // Build fs.img
    let mut mkfs_cmd = Command::new(target_dir.join("mkfs").join(mkfs_target_triple).join("release").join("mkfs"));
    mkfs_cmd.current_dir(target_dir);
    mkfs_cmd
    .arg("fs.img")
    .arg("../README.md");
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
