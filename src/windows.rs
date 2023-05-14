use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::mem;
use std::os::windows::prelude::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::ptr;
use std::sync::Mutex;
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

static SELFDELETE_SUFFIX: &str = ".__selfdelete__.exe";

/// Utility function that delays the deletion of a file until the process shuts down.
/// The way this works is that it marks the given executable as DELETE_ON_CLOSE, and
/// schedules the spawn on that executable at process shutdown.  This special spawn
/// is picked up by `self_delete_on_init` later.
fn delete_at_exit(tmp_exe: PathBuf) -> Result<(), io::Error> {
    static TO_DELETE: Mutex<Option<(PathBuf, HANDLE)>> = Mutex::new(None);

    let mut guard = TO_DELETE.lock().unwrap();
    if guard.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot delete or replace executable twice",
        ));
    }

    let handle = unsafe { prepare_exe_for_deletion(&tmp_exe)? };
    *guard = Some((tmp_exe, handle));

    extern "C" {
        fn atexit(cb: unsafe extern "C" fn());
    }

    unsafe extern "C" fn schedule_delete() {
        if let Ok(guard) = TO_DELETE.lock() {
            if let Some((ref file, handle)) = *guard {
                respawn_to_self_delete(file, handle).ok();
            }
        }
    }

    unsafe {
        atexit(schedule_delete);
    }

    Ok(())
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
unsafe extern "C" fn self_delete_on_init() {
    if let Ok(module) = env::current_exe() {
        if module
            .file_name()
            .and_then(|x| x.to_str())
            .map_or(false, |x| x.ends_with(SELFDELETE_SUFFIX))
        {
            let tmp_filename = module.file_name().unwrap();
            let real_filename = tmp_filename
                .encode_wide()
                .skip(1)
                .take(tmp_filename.len() - (SELFDELETE_SUFFIX.len() + 1))
                .collect::<Vec<_>>();

            // This is abit odd.  These can fail, but because we are running in an atexit
            // handler there is really nothing we can do any more to report this.
            let failed = wait_for_parent_shutdown().is_err()
                || fs::remove_file(module.with_file_name(OsString::from_wide(&real_filename)))
                    .is_err();

            if !failed {
                // hack to make the system pick up on DELETE_ON_CLOSE.  For that purpose we
                // spawn the built-in "net" executable that just exits quickly with a message.
                Command::new("net")
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
unsafe fn prepare_exe_for_deletion(tmp_exe: &Path) -> Result<HANDLE, io::Error> {
    let tmp_exe_win: Vec<_> = tmp_exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let sa = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: 1,
    };

    let tmp_handle = CreateFileW(
        tmp_exe_win.as_ptr(),
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_DELETE,
        &sa,
        OPEN_EXISTING,
        FILE_FLAG_DELETE_ON_CLOSE,
        0,
    );

    if tmp_handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(tmp_handle)
}

/// Utility function that is executed at shutdown to spawn the given executable
/// which is the copy of the original executable.  Then it gives it 100 milliseconds
/// to shut down, which apparently is needed for this logic to work.
unsafe fn respawn_to_self_delete(tmp_exe: &Path, tmp_handle: HANDLE) -> Result<(), io::Error> {
    Command::new(tmp_exe).spawn().ok();
    thread::sleep(Duration::from_millis(100));
    CloseHandle(tmp_handle);
    Ok(())
}

/// This waits until the parent shut down.
///
/// This is sadly a bit racy.
unsafe fn wait_for_parent_shutdown() -> Result<(), io::Error> {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let mut entry: PROCESSENTRY32 = mem::zeroed();
    entry.dwSize = mem::size_of::<PROCESSENTRY32>() as _;

    if Process32First(snapshot, &mut entry) == 0 {
        CloseHandle(snapshot);
        return Err(io::Error::last_os_error());
    }

    while entry.th32ProcessID != GetCurrentProcessId() {
        if Process32Next(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return Err(io::Error::last_os_error());
        }
    }

    let parent = OpenProcess(SYNCHRONIZE, 0, entry.th32ParentProcessID);
    if parent == 0 {
        CloseHandle(snapshot);
        return Ok(());
    }

    let rv = WaitForSingleObject(parent, INFINITE);
    CloseHandle(snapshot);
    if rv != WAIT_OBJECT_0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Schedules the deleting of the given executable at shutdown.
///
/// The executable to be deleted has to be valid and have the necessary
/// code in it to perform self deletion.
fn schedule_self_deletion_on_shutdown(exe: &Path) -> Result<(), io::Error> {
    let tmp_exe = get_temp_executable_name(exe, SELFDELETE_SUFFIX);
    fs::copy(exe, &tmp_exe)?;
    delete_at_exit(tmp_exe)?;
    Ok(())
}

/// Takes an already existing path but prepends a `.` to the filename and
/// adds a new suffix to it.
fn get_temp_executable_name(exe: &Path, suffix: &str) -> PathBuf {
    let mut file_name = OsString::new();
    file_name.push(OsStr::new("."));
    file_name.push(exe.file_name().unwrap());
    file_name.push(OsStr::new(suffix));
    exe.with_file_name(file_name)
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
pub fn self_delete() -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    schedule_self_deletion_on_shutdown(&exe)?;
    Ok(())
}

/// This is similar to self_delete, but first renames the executable to a new temporary
/// location so that the executable can be updated by the given other one.
pub fn self_replace(new_executable: &Path) -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    let old_exe = get_temp_executable_name(&exe, ".__old__.exe");
    fs::rename(&exe, &old_exe)?;
    schedule_self_deletion_on_shutdown(&old_exe)?;
    fs::copy(new_executable, &exe)?;
    Ok(())
}
