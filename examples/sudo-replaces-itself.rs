use std::env::consts::EXE_EXTENSION;

fn main() {
    let new_executable = std::env::current_exe()
        .unwrap()
        .with_file_name("hello")
        .with_extension(EXE_EXTENSION);

    if !new_executable.is_file() {
        eprintln!("hello does not exist, run cargo build --example hello first.");
        std::process::exit(1);
    }

    println!("Next time I run, I am the hello executable");
    self_replace::sudo_self_replace(&new_executable, true).unwrap();
}
