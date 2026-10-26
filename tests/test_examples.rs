use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Once, RwLock};

static BUILT_EXAMPLES_INIT: Once = Once::new();
static BUILT_EXAMPLES: RwLock<Option<HashMap<String, PathBuf>>> = RwLock::new(None);

fn compile_examples() -> HashMap<String, PathBuf> {
    let mut cmd = Command::new("cargo");
    let output = cmd
        .arg("build")
        .arg("--examples")
        .arg("--message-format=json-render-diagnostics")
        .output()
        .unwrap();

    if !output.status.success() {
        println!("stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        println!("stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        panic!("cargo build --examples failed");
    }

    let mut rv = HashMap::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let message = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(message) => message,
            Err(_) => continue,
        };

        let is_example = message
            .get("target")
            .and_then(|target| target.get("kind"))
            .and_then(|kind| kind.as_array())
            .map_or(false, |kinds| {
                kinds.iter().any(|kind| kind.as_str() == Some("example"))
            });

        let built_name = message
            .get("target")
            .and_then(|target| target.get("name"))
            .and_then(|name| name.as_str());

        let executable = message.get("executable").and_then(|exe| exe.as_str());

        if let (true, Some(built_name), Some(executable)) = (is_example, built_name, executable) {
            rv.insert(built_name.to_string(), PathBuf::from(executable));
        }
    }

    rv
}

fn compile_example(name: &str) -> PathBuf {
    BUILT_EXAMPLES_INIT.call_once(|| {
        *BUILT_EXAMPLES.write().unwrap() = Some(compile_examples());
    });

    BUILT_EXAMPLES
        .read()
        .unwrap()
        .as_ref()
        .and_then(|examples| examples.get(name))
        .cloned()
        .unwrap_or_else(|| panic!("could not locate built executable for example {}", name))
}

fn get_executable(exe: &Path, tempdir: &Path) -> PathBuf {
    let final_exe = tempdir.join(exe.file_name().unwrap());
    fs::copy(exe, &final_exe).unwrap();
    final_exe
}

struct RunOptions<'a> {
    path: &'a Path,
    force_exit: bool,
    scratchspace: &'a Path,
    expected_output: &'a str,
}

fn run(opts: RunOptions) {
    let mut cmd = Command::new(opts.path);
    if opts.force_exit {
        cmd.env("FORCE_EXIT", "1");
    }

    // env::temp_dir is used on windows to place temporaries in some
    // cases.  Put it onto our scratchspace so we can assert that it's
    // left empty behind.
    #[cfg(windows)]
    {
        cmd.env("TMP", opts.scratchspace);
        cmd.env("TEMP", opts.scratchspace);
    }

    // does not actually matter today, but maybe it once will
    #[cfg(unix)]
    {
        cmd.env("TMPDIR", opts.scratchspace);
    }

    let output = cmd.output().unwrap();
    assert!(output.status.success());
    #[cfg(windows)]
    {
        // takes a bit
        use std::time::Duration;
        std::thread::sleep(Duration::from_millis(200));
    }
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert_eq!(stdout.trim(), opts.expected_output);
}

#[test]
fn test_self_delete() {
    let workspace = tempfile::tempdir().unwrap();
    let scratchspace = tempfile::tempdir().unwrap();
    let built_exe = compile_example("deletes-itself");
    let exe = get_executable(&built_exe, workspace.path());
    assert!(exe.is_file());
    run(RunOptions {
        path: &exe,
        force_exit: false,
        scratchspace: scratchspace.path(),
        expected_output: "When I finish, I am deleted",
    });
    assert!(!exe.is_file());
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[test]
fn test_self_delete_force_exit() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let built_exe = compile_example("deletes-itself");
    let exe = get_executable(&built_exe, workspace.path());
    assert!(exe.is_file());
    run(RunOptions {
        path: &exe,
        force_exit: true,
        scratchspace: scratchspace.path(),
        expected_output: "When I finish, I am deleted",
    });
    assert!(!exe.is_file());
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[test]
fn test_self_delete_outside_path() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let built_exe = compile_example("deletes-itself-outside-path");
    let exe = get_executable(&built_exe, workspace.path());
    assert!(exe.is_file());
    assert!(workspace.path().is_dir());
    run(RunOptions {
        path: &exe,
        force_exit: false,
        scratchspace: scratchspace.path(),
        expected_output: "When I finish, all of my parent folder is gone.",
    });
    assert!(!exe.is_file());
    assert!(!workspace.path().is_dir());
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[test]
fn test_self_delete_outside_path_force_exit() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let built_exe = compile_example("deletes-itself-outside-path");
    let exe = get_executable(&built_exe, workspace.path());
    assert!(exe.is_file());
    assert!(workspace.path().is_dir());
    run(RunOptions {
        path: &exe,
        force_exit: true,
        scratchspace: scratchspace.path(),
        expected_output: "When I finish, all of my parent folder is gone.",
    });
    assert!(!exe.is_file());
    assert!(!workspace.path().is_dir());
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[test]
fn test_self_replace() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = scratchspace.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let built_exe = compile_example("replaces-itself");
    let built_hello = compile_example("hello");

    let exe = get_executable(&built_exe, &workspace);
    let hello = get_executable(&built_hello, &workspace);

    assert!(exe.is_file());
    assert!(hello.is_file());

    run(RunOptions {
        path: &exe,
        force_exit: true,
        scratchspace: scratchspace.path(),
        expected_output: "Next time I run, I am the hello executable",
    });
    assert!(exe.is_file());
    assert!(hello.is_file());
    run(RunOptions {
        path: &exe,
        force_exit: false,
        scratchspace: scratchspace.path(),
        expected_output: "Hello World!",
    });

    fs::remove_dir_all(&workspace).unwrap();
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[test]
fn test_self_replace_force_exit() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = scratchspace.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let built_exe = compile_example("replaces-itself");
    let built_hello = compile_example("hello");

    let exe = get_executable(&built_exe, &workspace);
    let hello = get_executable(&built_hello, &workspace);

    assert!(exe.is_file());
    assert!(hello.is_file());

    run(RunOptions {
        path: &exe,
        force_exit: true,
        scratchspace: scratchspace.path(),
        expected_output: "Next time I run, I am the hello executable",
    });
    assert!(exe.is_file());
    assert!(hello.is_file());
    run(RunOptions {
        path: &exe,
        force_exit: false,
        scratchspace: scratchspace.path(),
        expected_output: "Hello World!",
    });

    fs::remove_dir_all(&workspace).unwrap();
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn test_self_replace_through_symlink() {
    let scratchspace = tempfile::tempdir().unwrap();
    let workspace = scratchspace.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let built_exe = compile_example("replaces-itself");
    let built_hello = compile_example("hello");

    let exe = get_executable(&built_exe, &workspace);
    let hello = get_executable(&built_hello, &workspace);

    let exe_symlink = workspace.join("bin").join("symlink");
    fs::create_dir_all(exe_symlink.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&exe, &exe_symlink).unwrap();

    assert!(exe.is_file());
    assert!(hello.is_file());
    assert!(std::fs::symlink_metadata(&exe_symlink)
        .unwrap()
        .file_type()
        .is_symlink());

    run(RunOptions {
        path: &exe_symlink,
        force_exit: true,
        scratchspace: scratchspace.path(),
        expected_output: "Next time I run, I am the hello executable",
    });
    assert!(exe.is_file());
    assert!(hello.is_file());
    assert!(std::fs::symlink_metadata(&exe_symlink)
        .unwrap()
        .file_type()
        .is_symlink());
    run(RunOptions {
        path: &exe_symlink,
        force_exit: false,
        scratchspace: scratchspace.path(),
        expected_output: "Hello World!",
    });

    fs::remove_dir_all(&workspace).unwrap();
    assert!(scratchspace.path().read_dir().unwrap().next().is_none());
}
