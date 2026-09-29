fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    if let Err(error) = kah_check::check_tree(std::path::Path::new(&root)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
