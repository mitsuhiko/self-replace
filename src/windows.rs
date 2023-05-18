use std::env;
use std::fs;
use std::io;
use std::mem;
use std::os::windows::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::ptr;
use std::sync::Once;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_DELETE_ON_CLOSE, FILE_SHARE_DELETE, FILE_SHARE_READ, OPEN_EXISTING,
    SYNCHRONIZE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, WaitForSingleObject, INFINITE,
};

extern "C" {
    fn atexit(cb: unsafe extern "C" fn());
}

static SELFDELETE_SUFFIX: &str = ".__selfdelete__.exe";
static RELOCATED_SUFFIX: &str = ".__relocated__.exe";
static TEMP_SUFFIX: &str = ".__temp__.exe";

/// Utility function that delays the deletion of a file until the process shuts down.
/// The way this works is that it marks the given executable as DELETE_ON_CLOSE, and
/// schedules the spawn on that executable at process shutdown.  This special spawn
/// is picked up by `self_delete_on_init` later.
fn delete_at_exit(tmp_exe: PathBuf, original_exe: PathBuf) -> Result<(), io::Error> {
    static mut HANDLE_AND_EXES: Option<Box<(HANDLE, PathBuf, PathBuf)>> = None;
    static DELETE_ONCE: Once = Once::new();

    unsafe extern "C" fn delete_on_exit() {
        // Safety: if the once is completed, the handle and exe are set and it's safe
        // to access the static.
        if DELETE_ONCE.is_completed() {
            if let Some(ref tup) = HANDLE_AND_EXES {
                respawn_to_self_delete(&tup.1, &tup.2, tup.0).ok();
            }
        }
    }

    let mut prepare_result = None;
    DELETE_ONCE.call_once(|| {
        prepare_result = Some(prepare_exe_for_deletion(&tmp_exe));
        if let Some(Ok(handle)) = prepare_result {
            unsafe {
                HANDLE_AND_EXES = Some(Box::new((handle, tmp_exe, original_exe)));
            }
        }
    });

    // if we get a `prepare_result` back it means that our Once just initialized.
    // In that case if it's a failure, we abort here, otherwise we register an
    // atexit handler.
    match prepare_result {
        Some(Ok(_)) => {
            unsafe {
                atexit(delete_on_exit);
            }
            Ok(())
        }
        Some(Err(err)) => Err(err),
        None => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot delete or replace executable twice",
        )),
    }
}

/// This allows us to register a function that self destroys the process on startup
/// in certain circumstances.  We pick `.CRT$XCV` here because that section is
/// currently used after rust is initialized.
#[used]
#[link_section = ".CRT$XCV"]
static INIT_TABLE_ENTRY: unsafe extern "C" fn() = self_delete_on_init;

/// This is violates some important Rust rules, primarily that there is no life before
/// main, but for our purposes that's good enough.  To make this work better we should
/// probably only use winapi functions directly here.
///
/// The logic of this is heavily inspired by the implementation of rustup which itself
/// is modelled after a blog post that no longer exists.  However a copy pasted version
/// of it can be found here: https://0x00sec.org/t/self-deleting-executables/33702
extern "C" fn self_delete_on_init() {
    if let Ok(module) = env::current_exe() {
        if module
            .file_name()
            .and_then(|x| x.to_str())
            .map_or(false, |x| x.ends_with(SELFDELETE_SUFFIX))
        {
            let real_filename = std::env::args_os().nth(1).unwrap();

            // This is abit odd.  These can fail, but because we are running in an atexit
            // handler there is really nothing we can do any more to report this.
            let failed = !wait_for_parent_shutdown() || fs::remove_file(real_filename).is_err();

            if !failed {
                // hack to make the system pick up on DELETE_ON_CLOSE.  For that purpose we
                // spawn the built-in "ping" executable and make it ping for a second.  That
                // gives us enough time for our handle to stay alive until after the operating
                // system has shut us down.
                Command::new("ping")
                    .arg("127.0.0.1")
                    .arg("-n")
                    .arg("1")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok();
            }

            exit(if failed { 1 } else { 0 })
        }
    }
}

/// Opens the given exec as `DELETE_ON_CLOSE` and returns the handle.
fn prepare_exe_for_deletion(tmp_exe: &Path) -> Result<HANDLE, io::Error> {
    let tmp_exe_win: Vec<_> = tmp_exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let sa = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: 1,
    };

    let tmp_handle = unsafe {
        CreateFileW(
            tmp_exe_win.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            &sa,
            OPEN_EXISTING,
            FILE_FLAG_DELETE_ON_CLOSE,
            0,
        )
    };

    if tmp_handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(tmp_handle)
}

/// Utility function that is executed at shutdown to spawn the given executable
/// which is the copy of the original executable.  Then it gives it 100 milliseconds
/// to shut down, which apparently is needed for this logic to work.
///
/// # Safety
///
/// The `tmp_handle` needs to be valid.
unsafe fn respawn_to_self_delete(
    tmp_exe: &Path,
    original_exe: &Path,
    tmp_handle: HANDLE,
) -> Result<(), io::Error> {
    Command::new(tmp_exe).arg(original_exe).spawn().ok();
    thread::sleep(Duration::from_millis(100));
    CloseHandle(tmp_handle);
    Ok(())
}

/// This waits until the parent shut down.  It will return `true` if the parent was
/// shut down or `false` if an error ocurred.
///
/// This is sadly a bit racy.
fn wait_for_parent_shutdown() -> bool {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }

        let mut entry: PROCESSENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32>() as _;

        if Process32First(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return false;
        }

        while entry.th32ProcessID != GetCurrentProcessId() {
            if Process32Next(snapshot, &mut entry) == 0 {
                CloseHandle(snapshot);
                return false;
            }
        }

        let parent = OpenProcess(SYNCHRONIZE, 0, entry.th32ParentProcessID);
        if parent == 0 {
            CloseHandle(snapshot);
            return true;
        }

        let rv = WaitForSingleObject(parent, INFINITE);
        CloseHandle(snapshot);
        rv == WAIT_OBJECT_0
    }
}

/// Schedules the deleting of the given executable at shutdown.
///
/// The executable to be deleted has to be valid and have the necessary
/// code in it to perform self deletion.
fn schedule_self_deletion_on_shutdown(
    exe: &Path,
    protected_path: Option<&Path>,
) -> Result<(), io::Error> {
    let first_choice = env::temp_dir();
    let relocated_exe = get_temp_executable_name(&first_choice, RELOCATED_SUFFIX);
    if fs::rename(exe, &relocated_exe).is_ok() {
        let tmp_exe = get_temp_executable_name(&first_choice, SELFDELETE_SUFFIX);
        fs::copy(&relocated_exe, &tmp_exe)?;
        delete_at_exit(tmp_exe, relocated_exe)?;
    } else if let Some(protected_path) = protected_path {
        let path = protected_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "protected path has no parent")
        })?;

        let tmp_exe = get_temp_executable_name(path, SELFDELETE_SUFFIX);
        let relocated_exe = get_temp_executable_name(path, RELOCATED_SUFFIX);
        fs::copy(exe, &tmp_exe)?;
        fs::rename(exe, &relocated_exe)?;
        delete_at_exit(tmp_exe, relocated_exe)?;
    } else {
        let tmp_exe = get_temp_executable_name(get_directory_of(exe)?, SELFDELETE_SUFFIX);
        fs::copy(exe, &tmp_exe)?;
        delete_at_exit(tmp_exe, exe.to_path_buf())?;
    }
    Ok(())
}

// This creates a temporary executable with a random name in the given directory and
// the provided suffix.
fn get_temp_executable_name(base: &Path, suffix: &str) -> PathBuf {
    let rng = fastrand::Rng::new();
    let mut file_name = String::new();
    file_name.push('.');

    if let Some(hint) = env::current_exe()
        .ok()
        .as_ref()
        .and_then(|x| x.file_stem())
        .and_then(|x| x.to_str())
    {
        file_name.push_str(hint);
        file_name.push('.');
    }

    for _ in 0..32 {
        file_name.push(rng.lowercase());
    }
    file_name.push_str(suffix);
    base.join(file_name)
}

fn get_directory_of(p: &Path) -> Result<&Path, io::Error> {
    p.parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))
}

/// The logic here is a bit like the following:
///
/// 1. First we create a copy of our executable in a way that we can actually make it
///    spawn.  This means we put it next to the current binary, with a name that is
///    unlikely going to clash and where we can reproduce the original name from.
/// 2. The copied executable itself is marked so it gets deleted when windows no longer
///    needs the file (`DELETE_ON_CLOSE`)
/// 3. Then we spawn the copy of that executable with a flag that we can pick up in
///    `self_delete_on_init`.  All of this logic is delayed until the process
///    actually shuts down.
/// 4. In `self_delete_on_init` spawn a dummy process so that windows deletes the
///    copy too.
pub fn self_delete(protected_path: Option<&Path>) -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    schedule_self_deletion_on_shutdown(&exe, protected_path)?;
    Ok(())
}

/// This is similar to self_delete, but first renames the executable to a new temporary
/// location so that the executable can be updated by the given other one.
pub fn self_replace(new_executable: &Path) -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    let old_exe = get_temp_executable_name(get_directory_of(&exe)?, RELOCATED_SUFFIX);
    fs::rename(&exe, &old_exe)?;
    schedule_self_deletion_on_shutdown(&old_exe, None)?;
    let temp_exe = get_temp_executable_name(get_directory_of(&exe)?, TEMP_SUFFIX);
    fs::copy(new_executable, &temp_exe)?;
    fs::rename(&temp_exe, &exe)?;
    Ok(())
}
