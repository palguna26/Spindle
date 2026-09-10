fn main() {
    if let Err(error) = spindle::cli::run() {
        eprintln!("spindle: {error}");
        std::process::exit(1);
    }
}
