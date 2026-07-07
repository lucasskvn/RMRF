use nix::sys::signal::{self, SigHandler, Signal};
use std::sync::atomic::{AtomicBool, Ordering};

pub static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

pub fn setup_signal_handler() {
    let handler = SigHandler::Handler(handle_signal);
    for sig in &[Signal::SIGTERM, Signal::SIGINT] {
        if let Err(e) = unsafe { signal::signal(*sig, handler) } {
            eprintln!("Warning: could not set signal handler for {sig:?}: {e}");
        }
    }
}

extern "C" fn handle_signal(_: i32) {
    SHOULD_EXIT.store(true, Ordering::SeqCst);
}
