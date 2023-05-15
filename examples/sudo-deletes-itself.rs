fn main() {
    println!("When I finish, I am deleted");
    self_replace::sudo_self_delete(true).unwrap();
}
