use std::sync::atomic::{AtomicBool, Ordering};

pub static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

pub fn setup_signal_handler() {
    let result = ctrlc::set_handler(move || {
        SHOULD_EXIT.store(true, Ordering::SeqCst);
        eprintln!("\nInterrupt received — finishing current chunk, then stopping...");
    });
    if let Err(e) = result {
        eprintln!("Warning: could not set signal handler: {e}");
    }
}
