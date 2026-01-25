//! Signal handling for graceful shutdown.
//!
//! This module provides signal handling for SIGINT (Ctrl+C) and SIGTERM
//! to enable graceful shutdown of the node.

use tokio::sync::watch;

/// Creates a shutdown signal receiver that triggers on SIGINT or SIGTERM.
///
/// Returns a `watch::Receiver<bool>` that changes to `true` when a shutdown
/// signal is received. The receiver can be cloned and shared across tasks.
///
/// # Example
///
/// ```no_run
/// use nodalync_cli::signals::shutdown_signal;
///
/// #[tokio::main]
/// async fn main() {
///     let mut shutdown = shutdown_signal();
///
///     loop {
///         tokio::select! {
///             _ = shutdown.changed() => {
///                 println!("Shutdown signal received");
///                 break;
///             }
///             // ... other work
///         }
///     }
/// }
/// ```
pub fn shutdown_signal() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);

    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        let _ = tx.send(true);
    });

    rx
}

/// Wait for either SIGINT or SIGTERM.
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_signal_initial_state() {
        let rx = shutdown_signal();
        // Initial state should be false (not shutting down)
        assert!(!*rx.borrow());
    }
}
