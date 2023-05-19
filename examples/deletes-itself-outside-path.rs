use std::fs;

fn main() {
    let me = std::env::current_exe().unwrap();
    let parent = me.parent().unwrap();
    println!("When I finish, all of my parent folder is gone.");
    self_replace::self_delete_outside_path(parent).unwrap();
    fs::remove_dir_all(parent).unwrap();

    if std::env::var("FORCE_EXIT").ok().as_deref() == Some("1") {
        std::process::exit(0);
    }
}
