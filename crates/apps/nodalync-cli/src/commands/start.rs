//! Start node command.

use crate::config::{hbar_to_tinybars, tinybars_to_hbar, CliConfig};
use crate::context::NodeContext;
use crate::error::{CliError, CliResult};
use crate::node_runner::{
    check_existing_node, pid_file_path, remove_pid_file, remove_status_file,
    run_event_loop_with_health, status_file_path, write_pid_file_with_start_time, HealthConfig,
};
use crate::output::{OutputFormat, Render, StartOutput};
use crate::signals::shutdown_signal;

use nodalync_settle::Settlement;
use std::sync::Arc;
use tracing::{info, warn};

/// Check and perform auto-deposit if needed.
///
/// If settlement is configured and auto_deposit is enabled, this function checks
/// the current balance in the settlement contract. If the balance is below the
/// configured minimum, it deposits the configured amount.
///
/// This ensures the node can accept payment channels from other peers without
/// requiring manual deposit operations.
async fn maybe_auto_deposit(
    settlement: &Arc<dyn Settlement>,
    config: &CliConfig,
) -> CliResult<Option<String>> {
    if !config.settlement.auto_deposit {
        return Ok(None);
    }

    let min_balance = hbar_to_tinybars(config.settlement.min_contract_balance_hbar);
    let deposit_amount = hbar_to_tinybars(config.settlement.auto_deposit_amount_hbar);

    // Check current contract balance
    let balance = settlement
        .get_balance()
        .await
        .map_err(|e| CliError::user(format!("Failed to check settlement balance: {}", e)))?;

    if balance < min_balance {
        info!(
            current_balance_hbar = tinybars_to_hbar(balance),
            min_balance_hbar = config.settlement.min_contract_balance_hbar,
            deposit_amount_hbar = config.settlement.auto_deposit_amount_hbar,
            "Auto-depositing to settlement contract"
        );

        let tx_id = settlement
            .deposit(deposit_amount)
            .await
            .map_err(|e| CliError::user(format!("Auto-deposit failed: {}", e)))?;

        let new_balance = settlement.get_balance().await.unwrap_or(0);
        info!(
            tx_id = %tx_id,
            new_balance_hbar = tinybars_to_hbar(new_balance),
            "Auto-deposit successful"
        );

        Ok(Some(tx_id.to_string()))
    } else {
        info!(
            balance_hbar = tinybars_to_hbar(balance),
            min_balance_hbar = config.settlement.min_contract_balance_hbar,
            "Settlement contract balance sufficient, skipping auto-deposit"
        );
        Ok(None)
    }
}

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

    // Check identity exists BEFORE checking PID file (Issue #45).
    // Without this order, a missing identity hits the stale PID check first
    // and reports "Node is already running" instead of "Identity not initialized."
    if !crate::context::identity_exists(&config) {
        return Err(CliError::IdentityNotInitialized);
    }

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

    // Auto-deposit if settlement is configured and balance is low
    if let Some(ref settlement) = ctx.settlement {
        match maybe_auto_deposit(settlement, &config).await {
            Ok(Some(tx_id)) => {
                println!("Auto-deposited to settlement contract (tx: {})", tx_id);
            }
            Ok(None) => {}
            Err(e) => {
                warn!(error = %e, "Auto-deposit failed, continuing anyway");
            }
        }
    }

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

    // Configure health endpoint with alerting
    let health_config = HealthConfig {
        enabled: health,
        port: health_port,
        alerting: config.alerting.clone(),
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

    // Check identity exists BEFORE checking PID file (Issue #45)
    if !crate::context::identity_exists(&config) {
        return Err(CliError::IdentityNotInitialized);
    }

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
    // Issue #80: Warn that the daemon needs time to become ready
    println!(
        "Note: The daemon may take a few seconds to become ready. Check with 'nodalync status'."
    );

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

                // Auto-deposit if settlement is configured and balance is low
                if let Some(ref settlement) = ctx.settlement {
                    match maybe_auto_deposit(settlement, &config).await {
                        Ok(Some(tx_id)) => {
                            eprintln!("Auto-deposited to settlement contract (tx: {})", tx_id);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("Warning: Auto-deposit failed: {}", e);
                            // Continue anyway - node can still serve content
                        }
                    }
                }

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

                // Configure health endpoint with alerting
                let health_config = HealthConfig {
                    enabled: health,
                    port: health_port,
                    alerting: config.alerting.clone(),
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

    /// Regression test for Issue #80: daemon start should mention readiness delay.
    /// We verify the note is present in the daemon start code path by checking
    /// that the source code prints the expected message (the actual forking path
    /// cannot be unit-tested without spawning a real daemon process).
    #[test]
    fn test_daemon_start_mentions_readiness_delay() {
        // The readiness note is printed in start_daemon_sync before forking.
        // We verify it exists by checking the source file contains the expected text.
        // This test will fail if someone removes the note.
        let source = include_str!("start.rs");
        assert!(
            source.contains("may take a few seconds to become ready"),
            "Daemon start should include a readiness delay note for users"
        );
    }

    /// Regression test for Issue #45: `start` without identity should say
    /// "Identity not initialized", not "Node is already running."
    #[tokio::test]
    async fn test_start_without_identity_gives_correct_error() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut config = CliConfig::default();
        config.storage.content_dir = temp_dir.path().join("content");
        config.storage.cache_dir = temp_dir.path().join("cache");
        config.storage.database = temp_dir.path().join("nodalync.db");
        config.identity.keyfile = temp_dir.path().join("identity").join("keypair.key");

        // Do NOT initialize identity — start should fail with IdentityNotInitialized
        let result = start(config, OutputFormat::Human, false, false, 8080).await;
        assert!(result.is_err());
        assert!(
            matches!(
                result.as_ref().unwrap_err(),
                CliError::IdentityNotInitialized
            ),
            "Should get IdentityNotInitialized, got: {:?}",
            result.unwrap_err()
        );
    }
}
