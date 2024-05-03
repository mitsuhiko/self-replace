use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

fn load_executable(name: &PathBuf) -> &[u8] {
    let mut f = File::open(name).unwrap();
    let mut buf = Box::<Vec<u8>>::default();
    f.read_to_end(&mut buf).unwrap();
    Box::leak(buf)
}

use std::env::consts::EXE_EXTENSION;

fn main() {
    let exe = std::env::current_exe().unwrap();
    let new_executable = std::fs::read_link(exe.clone())
        .unwrap_or(exe)
        .with_file_name("hello")
        .with_extension(EXE_EXTENSION);

    if !new_executable.is_file() {
        eprintln!("hello does not exist, run cargo build --example hello first.");
        std::process::exit(1);
    }

    let new_executable_content = load_executable(&new_executable);

    println!("Next time I run, I am the hello executable");
    self_replace::self_replace_with(new_executable_content).unwrap();

    if std::env::var("FORCE_EXIT").ok().as_deref() == Some("1") {
        std::process::exit(0);
    }
}
