use std::env;
use std::fs;
use std::io;
use std::path::Path;

pub fn self_delete() -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    fs::remove_file(exe)?;
    Ok(())
}

#[cfg(feature = "sudo")]
pub fn sudo_self_delete(gui: bool) -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    let status = runas::Command::new("rm")
        .arg("-f")
        .arg("--")
        .arg(exe)
        .gui(gui)
        .status()?;
    if !status.success() {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unable to delete",
        ))
    } else {
        Ok(())
    }
}

pub fn self_replace(new_executable: &Path) -> Result<(), io::Error> {
    let exe = env::current_exe()?;
    let old_permissions = exe.metadata()?.permissions();

    let tmp = tempfile::Builder::new()
        .prefix("._tempexeswap")
        .tempfile_in(exe.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "executable has no known parent folder",
            )
        })?)?;
    fs::copy(new_executable, tmp.path())?;
    fs::set_permissions(tmp.path(), old_permissions)?;

    // if we made it this far, try to persist the temporary file and move it over.
    let (_, path) = tmp.keep()?;
    match fs::rename(&path, &exe) {
        Ok(()) => {}
        Err(err) => {
            fs::remove_file(&path).ok();
            return Err(err);
        }
    }

    Ok(())
}

#[cfg(feature = "sudo")]
pub fn sudo_self_replace(new_executable: &Path, gui: bool) -> Result<(), io::Error> {
    let exe = env::current_exe()?;
    let old_permissions = exe.metadata()?.permissions();

    let tmp = tempfile::NamedTempFile::new()?;
    fs::copy(new_executable, tmp.path())?;
    fs::set_permissions(tmp.path(), old_permissions)?;

    // if we made it this far, try to persist the temporary file and move it over.
    let (_, path) = tmp.keep()?;

    match runas::Command::new("mv")
        .arg("--")
        .arg(&path)
        .arg(&exe)
        .gui(gui)
        .status()
    {
        Ok(status) => {
            fs::remove_file(&path).ok();
            if !status.success() {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "could not replace file",
                ))
            } else {
                Ok(())
            }
        }
        Err(err) => {
            fs::remove_file(&path).ok();
            Err(err)
        }
    }
}
