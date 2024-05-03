use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

fn prepare_current_exe() -> Result<(NamedTempFile, fs::Permissions, PathBuf), io::Error> {
    let mut exe = env::current_exe()?;
    if fs::symlink_metadata(&exe).map_or(false, |x| x.file_type().is_symlink()) {
        exe = fs::read_link(exe)?;
    }
    let old_permissions = exe.metadata()?.permissions();

    let prefix = if let Some(hint) = exe.file_stem().and_then(|x| x.to_str()) {
        format!(".{}.__temp__", hint)
    } else {
        ".__temp__".into()
    };

    Ok((
        tempfile::Builder::new()
            .prefix(&prefix)
            .tempfile_in(exe.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "executable has no known parent folder",
                )
            })?)?,
        old_permissions,
        exe,
    ))
}

fn finalize_updated_exe(tmp: NamedTempFile, exe: PathBuf) -> Result<(), io::Error> {
    let (_, path) = tmp.keep()?;
    match fs::rename(&path, exe) {
        Ok(()) => Ok(()),
        Err(err) => {
            fs::remove_file(&path).ok();
            Err(err)
        }
    }
}

/// On Unix a running executable can be safely deleted.
pub fn self_delete() -> Result<(), io::Error> {
    let exe = env::current_exe()?.canonicalize()?;
    fs::remove_file(exe)?;
    Ok(())
}

pub fn self_replace(new_executable: &Path) -> Result<(), io::Error> {
    let (tmp, old_permissions, exe) = prepare_current_exe()?;
    fs::copy(new_executable, tmp.path())?;
    fs::set_permissions(tmp.path(), old_permissions)?;

    // if we made it this far, try to persist the temporary file and move it over.
    finalize_updated_exe(tmp, exe)?;

    Ok(())
}

pub fn self_replace_with(new_executable_content: &[u8]) -> Result<(), io::Error> {
    let (tmp, old_permissions, exe) = prepare_current_exe()?;
    let mut new_executable = fs::File::create(tmp.path())?;
    new_executable.write_all(new_executable_content)?;
    new_executable.flush()?;
    fs::set_permissions(tmp.path(), old_permissions)?;

    // if we made it this far, try to persist the temporary file and move it over.
    finalize_updated_exe(tmp, exe)?;

    Ok(())
}
