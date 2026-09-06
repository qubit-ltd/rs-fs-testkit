//! Panic assertions shared by cleanup regression tests.

#![allow(dead_code)]

/// Runs `operation` and returns its panic payload as text.
pub fn catch_message(operation: impl FnOnce() + std::panic::UnwindSafe) -> String {
    match std::panic::catch_unwind(operation) {
        Ok(()) => panic!("operation unexpectedly succeeded"),
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|value| (*value).to_owned()))
            .unwrap_or_else(|| "non-string panic".to_owned()),
    }
}
