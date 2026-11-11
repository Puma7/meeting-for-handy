#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--protected-profile")
    {
        handy_meeting_desktop::run();
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = handy_meeting_desktop::run_cli(&args) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    handy_meeting_desktop::run();
}
