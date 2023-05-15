//! `self-replace` is a crate that allows binaries to replace themselves with newer
//! versions or to uninstall themselves.  On Unix systems this is a simple feat, but
//! on Windows a few hacks are needed which is why this crate exists.
//!
//! This is a useful operation when working with single-executable utilties that
//! want to implement a form of self updating or self uninstallation.
//!
//! ## Self Deletion
//!
//! The [`self_delete`] function schedules a binary for self deletion.  On Unix the
//! file system entry is immediately deleted, on Windows the file is deleted after the
//! process shuts down.  Note that you should not use this function to be followed up
//! by a replacement operation, for that use [`self_replace`] as on Windows the file
//! will still be locked.
//!
//! ```
//! # fn foo() -> Result<(), std::io::Error> {
//! self_replace::self_delete()?;
//! # Ok(()) }
//! ```
//!
//! ## Self Replacing
//!
//! This replaces the binary with another binary.  The provided path is copied over and
//! if the function successfully completes, you can delete the source binary.
//!
//! ```
//! use std::fs;
//!
//! # fn foo() -> Result<(), std::io::Error> {
//! let new_binary = "/path/to/new/binary";
//! self_replace::self_replace(&new_binary)?;
//! fs::remove_file(&new_binary)?;
//! # Ok(()) }
//! ```
use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

// the implementation to use
#[cfg(unix)]
use crate::unix as imp;
#[cfg(windows)]
use crate::windows as imp;

/// Deletes the executable in a platform independent manner.
///
/// The deletion on windows is delayed until the process shuts down.  For updating
/// instead of deleting, use [`self_replace`] instead.  Not that on Windows you can
/// only call this function once during the execution of the program.
///
/// ```
/// # fn foo() -> Result<(), std::io::Error> {
/// self_replace::self_delete()?;
/// # Ok(()) }
/// ```
pub fn self_delete() -> Result<(), io::Error> {
    imp::self_delete()
}

/// Replaces the running executable with a differnet one.
///
/// This replaces the binary with another binary.  The provided path is copied over and
/// if the function successfully completes, you can delete the source binary.
///
/// ```
/// use std::fs;
///
/// # fn foo() -> Result<(), std::io::Error> {
/// let new_binary = "/path/to/new/binary";
/// self_replace::self_replace(&new_binary)?;
/// fs::remove_file(&new_binary)?;
/// # Ok(()) }
/// ```
///
/// Note that after this function concludes, the new executable is already placed at the
/// old location, and the previous executable has been moved to a temporary alternative
/// location.  This also means that if you want to manipulate that file further (for
/// instance to change the permissions) you can do so.
///
/// By default the permissions of the original file are restored.
pub fn self_replace<P: AsRef<Path>>(new_executable: P) -> Result<(), io::Error> {
    imp::self_replace(new_executable.as_ref())
}

/// Sudo version of [`self_delete`].
///
/// This works exactly like [`self_delete`] but it requests sudo
/// permissions first.  The `gui` flag controls the type of sudo
/// prompt that should be shown.
///
/// To only sudo when necessary, [`has_delete_permissions`] can be
/// used beforehand.
#[cfg(feature = "sudo")]
pub fn sudo_self_delete(gui: bool) -> Result<(), io::Error> {
    imp::sudo_self_delete(gui)
}

/// Sudo version of [`self_replace`].
///
/// This works exactly like [`self_replace`] but it requests sudo
/// permissions first.  The `gui` flag controls the type of sudo
/// prompt that should be shown.
///
/// To only sudo when necessary, [`has_delete_permissions`] can be
/// used beforehand.
#[cfg(feature = "sudo")]
pub fn sudo_self_replace<P: AsRef<Path>>(new_executable: P, gui: bool) -> Result<(), io::Error> {
    imp::sudo_self_replace(new_executable.as_ref(), gui)
}

/// Checks if the current user has permissions to delete the executable.
#[cfg(feature = "sudo")]
pub fn has_delete_permissions() -> Result<bool, io::Error> {
    let exe = std::env::current_exe()?;
    permissions::is_removable(exe)
}

/// Checks if the current user has permissions to replace the executable.
#[cfg(feature = "sudo")]
pub fn has_replace_permissions() -> Result<bool, io::Error> {
    let exe = std::env::current_exe()?;
    let parent = match exe.parent() {
        Some(parent) => parent,
        None => return Ok(false),
    };
    Ok(permissions::is_removable(&exe)?
        && permissions::is_creatable(parent)?
        && permissions::is_creatable(&exe)?)
}
