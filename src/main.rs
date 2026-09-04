fn main() {
    if let Err(err) = difforge::check_startup() {
        eprintln!("{}", difforge::startup_message(err));
        std::process::exit(1);
    }
    if let Err(err) = difforge::run() {
        let _ = ratatui::try_restore();
        eprintln!("difforge: {err}");
        std::process::exit(1);
    }
}
