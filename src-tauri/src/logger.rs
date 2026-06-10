use std::panic;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;

pub fn setup_panic_hook() {
    panic::set_hook(Box::new(|panic_info| {
        let message = format!(
            "[{}] Panic: {}\nLocation: {:?}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            panic_info,
            panic_info.location()
        );
        
        let _ = log_error(&message);
        eprint!("{}", message);
    }));
}

pub fn log_error(message: &str) -> std::io::Result<()> {
    let log_path = std::env::temp_dir().join("quickwheel_error.log");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(message.as_bytes())?;
    Ok(())
}
