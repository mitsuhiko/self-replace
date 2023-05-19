fn main() {
    println!("When I finish, I am deleted");
    self_replace::self_delete().unwrap();

    if std::env::var("FORCE_EXIT").ok().as_deref() == Some("1") {
        std::process::exit(0);
    }
}
