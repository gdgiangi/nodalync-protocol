//! Start node command.

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::node_runner::{
    check_existing_node, pid_file_path, remove_pid_file, run_event_loop,
    write_pid_file_with_start_time,
};
use crate::output::{OutputFormat, Render, StartOutput};
use crate::signals::shutdown_signal;

use tracing::info;

/// Execute the start command.
pub async fn start(config: CliConfig, format: OutputFormat, daemon: bool) -> CliResult<String> {
    let base_dir = config.base_dir();

    // Check for existing running node
    if check_existing_node(&base_dir).is_some() {
        return Err(CliError::NodeAlreadyRunning);
    }

    if daemon {
        // Daemon mode: fork first, then create context
        #[cfg(unix)]
        {
            return start_daemon(config, format, &base_dir).await;
        }

        #[cfg(not(unix))]
        {
            return Err(CliError::user(
                "Daemon mode is only supported on Unix systems",
            ));
        }
    }

    // Foreground mode: create context and run
    let mut ctx = NodeContext::with_network(config.clone()).await?;

    // Bootstrap
    ctx.bootstrap().await?;

    // Subscribe to announcements
    if let Some(ref network) = ctx.network {
        network.subscribe_announcements().await?;
    }

    // Get info for output
    let listen_addresses = config.network.listen_addresses.clone();
    let connected_peers = ctx.connected_peers() as u32;
    let peer_id = ctx.peer_id().to_string();

    let output = StartOutput {
        peer_id,
        listen_addresses,
        connected_peers,
        daemon,
    };

    // Write PID file with start time
    let pid_path = pid_file_path(&base_dir);
    write_pid_file_with_start_time(&pid_path)?;

    // Print status and run event loop
    let output_str = output.render(format);
    println!("{}", output_str);
    println!("\nPress Ctrl+C to stop the node...\n");

    // Set up shutdown signal handler
    let shutdown_rx = shutdown_signal();

    // Run the event loop
    let result = run_event_loop(&mut ctx, shutdown_rx).await;

    // Cleanup on exit
    info!("Cleaning up...");
    let _ = remove_pid_file(&pid_path);

    result?;

    Ok("Node stopped gracefully.".to_string())
}

/// Start the node in daemon mode (Unix only).
///
/// IMPORTANT: We fork FIRST, then create the context in the child process.
/// This avoids the issue of inheriting a partial Tokio runtime state from the parent.
#[cfg(unix)]
async fn start_daemon(
    config: CliConfig,
    _format: OutputFormat,
    base_dir: &std::path::Path,
) -> CliResult<String> {
    use daemonize::Daemonize;
    use std::fs::File;

    let pid_path = pid_file_path(base_dir);

    // Create log files for stdout/stderr
    let stdout_path = base_dir.join("node.stdout.log");
    let stderr_path = base_dir.join("node.stderr.log");

    // Ensure base directory exists
    std::fs::create_dir_all(base_dir)?;

    let stdout = File::create(&stdout_path)?;
    let stderr = File::create(&stderr_path)?;

    // Configure daemonize - note: we don't use pid_file here because we want
    // to write it ourselves with the start time
    let daemonize = Daemonize::new()
        .working_directory(base_dir)
        .stdout(stdout)
        .stderr(stderr);

    // Fork to background
    match daemonize.start() {
        Ok(()) => {
            // We are now in the child process (daemon)
            // Create a new Tokio runtime for the child process
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| CliError::config(format!("Failed to create runtime: {}", e)))?;

            rt.block_on(async {
                // Now create the context AFTER the fork, in a fresh runtime
                let mut ctx = match NodeContext::with_network(config.clone()).await {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        eprintln!("Failed to initialize node: {}", e);
                        std::process::exit(1);
                    }
                };

                // Bootstrap
                if let Err(e) = ctx.bootstrap().await {
                    eprintln!("Failed to bootstrap: {}", e);
                    std::process::exit(1);
                }

                // Subscribe to announcements
                if let Some(ref network) = ctx.network {
                    if let Err(e) = network.subscribe_announcements().await {
                        eprintln!("Failed to subscribe to announcements: {}", e);
                        std::process::exit(1);
                    }
                }

                // Write PID file with start time (after successful init)
                if let Err(e) = write_pid_file_with_start_time(&pid_path) {
                    eprintln!("Failed to write PID file: {}", e);
                    std::process::exit(1);
                }

                // Set up shutdown signal handler
                let shutdown_rx = shutdown_signal();

                // Run the event loop
                let result = run_event_loop(&mut ctx, shutdown_rx).await;

                // Cleanup
                let _ = remove_pid_file(&pid_path);

                if let Err(e) = result {
                    eprintln!("Node error: {}", e);
                    std::process::exit(1);
                }
            });

            std::process::exit(0);
        }
        Err(e) => {
            // We are still in the parent process, daemonize failed
            Err(CliError::user(format!("Failed to daemonize: {}", e)))
        }
    }
}

/// Start the node in daemon mode (non-Unix stub).
#[cfg(not(unix))]
async fn start_daemon(
    _config: CliConfig,
    _format: OutputFormat,
    _base_dir: &std::path::Path,
) -> CliResult<String> {
    Err(CliError::user(
        "Daemon mode is only supported on Unix systems",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_output() {
        let output = StartOutput {
            peer_id: "ndl1abc123".to_string(),
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/9000".to_string()],
            connected_peers: 5,
            daemon: false,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Nodalync node started"));
        assert!(human.contains("Listening on"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"peer_id\""));
    }

    #[test]
    fn test_start_output_daemon() {
        let output = StartOutput {
            peer_id: "ndl1abc123".to_string(),
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/9000".to_string()],
            connected_peers: 0,
            daemon: true,
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("background"));
    }
}
