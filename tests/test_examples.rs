use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Once, RwLock};

static BUILT_EXAMPLES_INIT: Once = Once::new();
static BUILT_EXAMPLES: RwLock<Option<HashMap<String, PathBuf>>> = RwLock::new(None);

/// Advances past any whitespace.
fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while let Some(byte) = bytes.get(*pos) {
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' => *pos += 1,
            _ => break,
        }
    }
}

/// Advances past a quoted string, leaving `pos` right after the closing quote.
fn skip_string(bytes: &[u8], pos: &mut usize) -> Option<()> {
    if bytes.get(*pos) != Some(&b'"') {
        return None;
    }
    *pos += 1;
    loop {
        match bytes.get(*pos) {
            Some(b'\\') => *pos += 2,
            Some(b'"') => {
                *pos += 1;
                return Some(());
            }
            Some(_) => *pos += 1,
            None => return None,
        }
    }
}

/// Advances past an arbitrary JSON value, leaving `pos` right after it.
fn skip_value(bytes: &[u8], pos: &mut usize) -> Option<()> {
    match bytes.get(*pos)? {
        b'"' => skip_string(bytes, pos),
        b'{' | b'[' => {
            let mut depth = 0usize;
            loop {
                match bytes.get(*pos)? {
                    b'"' => skip_string(bytes, pos)?,
                    b'{' | b'[' => {
                        depth += 1;
                        *pos += 1;
                    }
                    b'}' | b']' => {
                        depth -= 1;
                        *pos += 1;
                        if depth == 0 {
                            return Some(());
                        }
                    }
                    _ => *pos += 1,
                }
            }
        }
        // numbers, `true`, `false` and `null` end at the next structural
        // character or whitespace.
        _ => {
            while let Some(byte) = bytes.get(*pos) {
                match byte {
                    b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r' => break,
                    _ => *pos += 1,
                }
            }
            Some(())
        }
    }
}

/// Returns the raw, still encoded value of a top-level key of a JSON object.
///
/// Nested objects and arrays are skipped over as a whole, so a key of the same
/// name further down the document is never mistaken for the one looked for.
fn json_field<'a>(obj: &'a str, key: &str) -> Option<&'a str> {
    let bytes = obj.as_bytes();
    let mut pos = 0;

    skip_ws(bytes, &mut pos);
    if bytes.get(pos) != Some(&b'{') {
        return None;
    }
    pos += 1;

    loop {
        skip_ws(bytes, &mut pos);
        match bytes.get(pos) {
            Some(b',') => {
                pos += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return None,
        }

        let key_start = pos;
        skip_string(bytes, &mut pos)?;
        let this_key = obj.get(key_start + 1..pos - 1)?;

        skip_ws(bytes, &mut pos);
        if bytes.get(pos) != Some(&b':') {
            return None;
        }
        pos += 1;
        skip_ws(bytes, &mut pos);

        let value_start = pos;
        skip_value(bytes, &mut pos)?;
        if this_key == key {
            return obj.get(value_start..pos);
        }
    }
}

/// Reads four hex digits of a `\u` escape.
fn json_hex4(chars: &mut std::str::Chars) -> Option<u32> {
    let mut rv = 0u32;
    for _ in 0..4 {
        rv = rv * 16 + chars.next()?.to_digit(16)?;
    }
    Some(rv)
}

/// Decodes a raw JSON string value.
///
/// Returns `None` for anything that is not a well formed string, which
/// includes the `null` that cargo emits for targets without an executable.
fn json_string(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes.first() != Some(&b'"') || bytes.last() != Some(&b'"') {
        return None;
    }

    let mut rv = String::with_capacity(raw.len() - 2);
    let mut chars = raw[1..raw.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            rv.push(c);
            continue;
        }
        match chars.next()? {
            '"' => rv.push('"'),
            '\\' => rv.push('\\'),
            '/' => rv.push('/'),
            'b' => rv.push('\u{8}'),
            'f' => rv.push('\u{c}'),
            'n' => rv.push('\n'),
            'r' => rv.push('\r'),
            't' => rv.push('\t'),
            'u' => match json_hex4(&mut chars)? {
                high @ 0xd800..=0xdbff => {
                    if chars.next()? != '\\' || chars.next()? != 'u' {
                        return None;
                    }
                    let low = json_hex4(&mut chars)?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return None;
                    }
                    rv.push(char::from_u32(
                        0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00),
                    )?);
                }
                code => rv.push(char::from_u32(code)?),
            },
            _ => return None,
        }
    }

    Some(rv)
}

/// Checks if a raw JSON array contains a given string.
fn json_array_contains(raw: &str, needle: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut pos = 0;

    skip_ws(bytes, &mut pos);
    if bytes.get(pos) != Some(&b'[') {
        return false;
    }
    pos += 1;

    loop {
        skip_ws(bytes, &mut pos);
        match bytes.get(pos) {
            Some(b',') => {
                pos += 1;
                continue;
            }
            Some(b']') | None => return false,
            _ => {}
        }

        let value_start = pos;
        if skip_value(bytes, &mut pos).is_none() {
            return false;
        }
        match raw.get(value_start..pos).and_then(json_string) {
            Some(value) if value == needle => return true,
            _ => {}
        }
    }
}

/// Picks the name and executable path of an example out of one of cargo's
/// JSON artifact messages.
///
/// This deliberately does not use a JSON library: the crate would otherwise
/// need a dev-dependency that has to be kept compatible with the MSRV.
fn parse_example_artifact(line: &str) -> Option<(String, PathBuf)> {
    let target = json_field(line, "target")?;
    if !json_array_contains(json_field(target, "kind")?, "example") {
        return None;
    }
    let name = json_string(json_field(target, "name")?)?;
    let executable = json_string(json_field(line, "executable")?)?;
    Some((name, PathBuf::from(executable)))
}

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
        if let Some((name, executable)) = parse_example_artifact(line) {
            rv.insert(name, executable);
        }
    }

    rv
}

#[test]
fn test_parse_example_artifact() {
    let line = r#"{"reason":"compiler-artifact","package_id":"path+file:///src#self-replace@1.5.0","manifest_path":"/src/Cargo.toml","target":{"kind":["example"],"crate_types":["bin"],"name":"hello","src_path":"/src/examples/hello.rs","edition":"2018","doc":false,"doctest":false,"test":false},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/src/target/debug/examples/hello"],"executable":"/src/target/debug/examples/hello","fresh":false}"#;
    assert_eq!(
        parse_example_artifact(line),
        Some((
            "hello".to_string(),
            PathBuf::from("/src/target/debug/examples/hello")
        ))
    );
}

#[test]
fn test_parse_example_artifact_windows_path() {
    let line = r#"{"reason":"compiler-artifact","target":{"kind":["example"],"name":"hello"},"executable":"C:\\src\\target\\debug\\examples\\hello.exe"}"#;
    assert_eq!(
        parse_example_artifact(line),
        Some((
            "hello".to_string(),
            PathBuf::from("C:\\src\\target\\debug\\examples\\hello.exe")
        ))
    );
}

#[test]
fn test_parse_example_artifact_skips_other_messages() {
    // the library itself has no executable
    let lib = r#"{"reason":"compiler-artifact","target":{"kind":["lib"],"name":"self-replace"},"executable":null}"#;
    assert_eq!(parse_example_artifact(lib), None);

    // build scripts do have one, but they are not examples
    let build_script = r#"{"reason":"compiler-artifact","target":{"kind":["custom-build"],"name":"build-script-build"},"executable":"/src/target/debug/build/x/build-script-build"}"#;
    assert_eq!(parse_example_artifact(build_script), None);

    // and other message kinds have neither
    let finished = r#"{"reason":"build-finished","success":true}"#;
    assert_eq!(parse_example_artifact(finished), None);
}

#[test]
fn test_parse_example_artifact_with_confusing_strings() {
    // structural characters inside strings must not confuse the scanner
    let line = r#"{"manifest_path":"/we{ird\"/\"executable\":\"nope\"/Cargo.toml","target":{"kind":["example"],"name":"hello"},"executable":"/we{ird/hello"}"#;
    assert_eq!(
        parse_example_artifact(line),
        Some(("hello".to_string(), PathBuf::from("/we{ird/hello")))
    );
}

#[test]
fn test_parse_example_artifact_survives_garbage() {
    for line in &[
        "",
        "not json at all",
        "{",
        r#"{"target":{"kind":["example"],"name":"hello"}"#,
        r#"{"target":{"kind":["example"],"name":"hello"},"executable":}"#,
        r#"{"target":{"kind":["example"],"name":"hello"},"executable":"unterminated}"#,
    ] {
        assert_eq!(parse_example_artifact(line), None);
    }
}

#[test]
fn test_parse_example_artifact_with_unexpected_types() {
    // well formed JSON, but the fields are not shaped the way cargo shapes
    // them.  None of these may parse into something or panic.
    for line in &[
        r#"{"target":null,"executable":"/src/target/debug/examples/hello"}"#,
        r#"{"target":"hello","executable":"/src/target/debug/examples/hello"}"#,
        r#"{"target":["example"],"executable":"/src/target/debug/examples/hello"}"#,
        // `kind` is not an array
        r#"{"target":{"kind":"example","name":"hello"},"executable":"/x/hello"}"#,
        // `kind` holds no strings
        r#"{"target":{"kind":[["example"]],"name":"hello"},"executable":"/x/hello"}"#,
        // `name` is not a string
        r#"{"target":{"kind":["example"],"name":42},"executable":"/x/hello"}"#,
        // `executable` is not a string
        r#"{"target":{"kind":["example"],"name":"hello"},"executable":42}"#,
        r#"{"target":{"kind":["example"],"name":"hello"},"executable":["/x/hello"]}"#,
        // the fields are missing altogether
        r#"{"target":{"name":"hello"},"executable":"/x/hello"}"#,
        r#"{"target":{"kind":["example"]},"executable":"/x/hello"}"#,
        r#"{"target":{"kind":["example"],"name":"hello"}}"#,
        r#"{}"#,
    ] {
        assert_eq!(parse_example_artifact(line), None);
    }
}

#[test]
fn test_json_string() {
    assert_eq!(
        json_string(r#""a\/b\nc\t\"d\"""#).as_deref(),
        Some("a/b\nc\t\"d\"")
    );
    assert_eq!(
        json_string(r#""\u00e9\ud83d\ude00""#).as_deref(),
        Some("é😀")
    );
    assert_eq!(json_string("null"), None);
    assert_eq!(json_string(r#""\ud800""#), None);
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
    fs::copy(&exe, &final_exe).unwrap();
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
