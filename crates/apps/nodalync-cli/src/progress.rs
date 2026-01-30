//! Progress bar utilities for CLI commands.

use std::future::Future;

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

/// Create a progress bar for known-length operations.
///
/// Shows progress as a percentage bar with ETA.
pub fn progress_bar(total: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("Invalid progress bar template")
            .progress_chars("=> "),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

/// Run an async operation with a spinner, returning the result.
///
/// The spinner displays while the operation runs and is cleared on completion.
pub async fn with_spinner<F, T>(msg: &str, fut: F) -> T
where
    F: Future<Output = T>,
{
    let pb = spinner(msg);
    let result = fut.await;
    pb.finish_and_clear();
    result
}

/// Run an async operation with a spinner, showing a success message on completion.
///
/// The spinner displays while the operation runs and shows the done message on completion.
pub async fn with_spinner_done<F, T>(msg: &str, done_msg: &str, fut: F) -> T
where
    F: Future<Output = T>,
{
    let pb = spinner(msg);
    let result = fut.await;
    pb.finish_with_message(done_msg.to_string());
    result
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

    #[test]
    fn test_progress_bar_creation() {
        let pb = progress_bar(100, "Downloading...");
        pb.inc(50);
        assert_eq!(pb.position(), 50);
        pb.finish_with_message("Done");
    }

    #[tokio::test]
    async fn test_with_spinner() {
        let result = with_spinner("Processing...", async { 42 }).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_with_spinner_done() {
        let result = with_spinner_done("Processing...", "Done!", async { "success" }).await;
        assert_eq!(result, "success");
    }
}
