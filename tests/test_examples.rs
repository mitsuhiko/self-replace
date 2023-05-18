use std::env::consts::EXE_EXTENSION;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::{env, fs};

fn compile_example(name: &str) {
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--example")
        .arg(name)
        .status()
        .unwrap();
}

fn get_executable(name: &str, tempdir: &Path) -> PathBuf {
    let exe = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(name)
        .with_extension(EXE_EXTENSION);
    let final_exe = tempdir.join(exe.file_name().unwrap());
    fs::copy(&exe, &final_exe).unwrap();
    final_exe
}

fn run(path: &Path, expected_output: &str) {
    let output = Command::new(path).output().unwrap();
    assert!(output.status.success());
    #[cfg(windows)]
    {
        // takes a bit
        std::thread::sleep(Duration::from_millis(200));
    }
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert_eq!(stdout.trim(), expected_output);
}

#[test]
fn test_self_delete() {
    let tempdir = tempfile::tempdir().unwrap();
    compile_example("deletes-itself");
    let exe = get_executable("deletes-itself", tempdir.path());
    assert!(exe.is_file());
    run(&exe, "When I finish, I am deleted");
    assert!(!exe.is_file());
}

#[test]
fn test_self_delete_outside_path() {
    let tempdir = tempfile::tempdir().unwrap();
    compile_example("deletes-itself-outside-path");
    let exe = get_executable("deletes-itself-outside-path", tempdir.path());
    assert!(exe.is_file());
    assert!(tempdir.path().is_dir());
    run(&exe, "When I finish, all of my parent folder is gone.");
    assert!(!exe.is_file());
    assert!(!tempdir.path().is_dir());
}

#[test]
fn test_self_replace() {
    let tempdir = tempfile::tempdir().unwrap();
    compile_example("replaces-itself");
    compile_example("hello");

    let exe = get_executable("replaces-itself", tempdir.path());
    let hello = get_executable("hello", tempdir.path());

    assert!(exe.is_file());
    assert!(hello.is_file());

    run(&exe, "Next time I run, I am the hello executable");
    assert!(exe.is_file());
    assert!(hello.is_file());
    run(&exe, "Hello World!");
}
