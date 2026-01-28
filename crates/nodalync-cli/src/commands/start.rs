//! Start node command.

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::node_runner::{
    check_existing_node, pid_file_path, remove_pid_file, remove_status_file,
    run_event_loop_with_health, status_file_path, write_pid_file_with_start_time, HealthConfig,
};
use crate::output::{OutputFormat, Render, StartOutput};
use crate::signals::shutdown_signal;

use tracing::info;

/// Execute the start command (foreground mode only).
///
/// For daemon mode, use `start_daemon_sync` which must be called
/// before any tokio runtime is created.
pub async fn start(
    config: CliConfig,
    format: OutputFormat,
    daemon: bool,
    health: bool,
    health_port: u16,
) -> CliResult<String> {
    let base_dir = config.base_dir();

    // Check for existing running node
    if check_existing_node(&base_dir).is_some() {
        return Err(CliError::NodeAlreadyRunning);
    }

    if daemon {
        // Daemon mode should be handled by start_daemon_sync before the runtime starts.
        // If we get here, something is wrong.
        return Err(CliError::user(
            "Daemon mode must be handled before async runtime. Use start_daemon_sync instead.",
        ));
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
    if health {
        println!("Health endpoint: http://0.0.0.0:{}/health", health_port);
    }
    println!("\nPress Ctrl+C to stop the node...\n");

    // Set up shutdown signal handler
    let shutdown_rx = shutdown_signal();

    // Configure health endpoint
    let health_config = HealthConfig {
        enabled: health,
        port: health_port,
    };

    // Run the event loop with status file updates and health endpoint
    let result =
        run_event_loop_with_health(&mut ctx, shutdown_rx, Some(&base_dir), health_config).await;

    // Cleanup on exit
    info!("Cleaning up...");
    let _ = remove_pid_file(&pid_path);
    let _ = remove_status_file(&status_file_path(&base_dir));

    result?;

    Ok("Node stopped gracefully.".to_string())
}

/// Start the node in daemon mode (synchronous, Unix only).
///
/// IMPORTANT: This function MUST be called BEFORE any tokio runtime is created.
/// It forks the process first, then creates a fresh tokio runtime in the child.
/// This avoids the "cannot start runtime from within runtime" panic.
#[cfg(unix)]
pub fn start_daemon_sync(
    config: CliConfig,
    _format: OutputFormat,
    health: bool,
    health_port: u16,
) -> CliResult<String> {
    use daemonize::Daemonize;
    use std::fs::File;

    let base_dir = config.base_dir();

    // Check for existing running node
    if check_existing_node(&base_dir).is_some() {
        return Err(CliError::NodeAlreadyRunning);
    }

    let pid_path = pid_file_path(&base_dir);

    // Create log files for stdout/stderr
    let stdout_path = base_dir.join("node.stdout.log");
    let stderr_path = base_dir.join("node.stderr.log");

    // Ensure base directory exists
    std::fs::create_dir_all(&base_dir)?;

    let stdout = File::create(&stdout_path)?;
    let stderr = File::create(&stderr_path)?;

    // Configure daemonize - note: we don't use pid_file here because we want
    // to write it ourselves with the start time
    let daemonize = Daemonize::new()
        .working_directory(&base_dir)
        .stdout(stdout)
        .stderr(stderr);

    // Print status BEFORE forking (parent will exit after fork)
    println!("Starting Nodalync node in background...");
    println!("Logs: {}", stderr_path.display());

    // Fork to background BEFORE any tokio runtime exists
    // Note: On success, the parent exits and only the child continues
    match daemonize.start() {
        Ok(()) => {
            // We are now in the child process (daemon)
            // Create a fresh Tokio runtime - this is safe because no runtime existed before fork
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| CliError::config(format!("Failed to create runtime: {}", e)))?;

            rt.block_on(async {
                // Now create the context in the fresh runtime
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

                // Log startup info
                let peer_id = ctx.peer_id().to_string();
                eprintln!("Nodalync daemon started (PID: {})", std::process::id());
                eprintln!("PeerId: {}", peer_id);
                if health {
                    eprintln!("Health endpoint: http://0.0.0.0:{}/health", health_port);
                }

                // Set up shutdown signal handler
                let shutdown_rx = shutdown_signal();

                // Configure health endpoint
                let health_config = HealthConfig {
                    enabled: health,
                    port: health_port,
                };

                // Run the event loop with status file updates and health endpoint
                let result = run_event_loop_with_health(
                    &mut ctx,
                    shutdown_rx,
                    Some(&base_dir),
                    health_config,
                )
                .await;

                // Cleanup
                let _ = remove_pid_file(&pid_path);
                let _ = remove_status_file(&status_file_path(&base_dir));

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
pub fn start_daemon_sync(
    _config: CliConfig,
    _format: OutputFormat,
    _health: bool,
    _health_port: u16,
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
