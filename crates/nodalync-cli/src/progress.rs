//! Progress bar utilities for CLI commands.

use indicatif::{ProgressBar, ProgressStyle};

/// Create a spinner progress bar with a message.
///
/// The spinner animates while waiting for an operation to complete.
pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.green} {msg}")
            .expect("Invalid progress bar template"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

/// Create a hidden progress bar (no-op).
///
/// Use this when running in non-interactive mode or JSON output.
pub fn hidden() -> ProgressBar {
    ProgressBar::hidden()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_creation() {
        let pb = spinner("Testing...");
        pb.finish_with_message("Done");
    }

    #[test]
    fn test_hidden_creation() {
        let pb = hidden();
        pb.finish();
    }
}
