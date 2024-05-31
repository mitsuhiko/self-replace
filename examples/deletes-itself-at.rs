fn main() {
    println!("When I finish, I am deleted");
    let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
    let exe_renamed = exe.with_file_name(format!(
        "deletes-itself-renamed{}",
        std::env::consts::EXE_SUFFIX
    ));

    std::fs::rename(exe, &exe_renamed).unwrap();
    self_replace::self_delete_at(exe_renamed).unwrap();

    if std::env::var("FORCE_EXIT").ok().as_deref() == Some("1") {
        std::process::exit(0);
    }
}
