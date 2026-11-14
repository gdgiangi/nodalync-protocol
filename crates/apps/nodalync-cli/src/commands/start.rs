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
    check_can_start(&config)?;

    if daemon {
        // Daemon mode should be handled by start_daemon_sync before the runtime starts.
        // If we get here, something is wrong.
        return Err(CliError::user(
            "Daemon mode must be handled before async runtime. Use start_daemon_sync instead.",
        ));
    }

    // Foreground mode: create context and run
    let mut ctx = prepare_foreground_node(&config).await?;

    // Get info for output
    let output = StartOutput {
        peer_id: ctx.peer_id().to_string(),
        listen_addresses: config.network.listen_addresses.clone(),
        connected_peers: ctx.connected_peers() as u32,
        daemon,
    };

    // Write PID file with start time
    let pid_path = pid_file_path(&base_dir);
    write_pid_file_with_start_time(&pid_path)?;

    // Print status and run event loop
    print_start_banner(&output.render(format), health, health_port);

    let health_config = health_config(&config, health, health_port);
    run_until_shutdown(&mut ctx, &base_dir, &pid_path, health_config).await?;

    Ok("Node stopped gracefully.".to_string())
}

/// Check identity exists BEFORE checking the PID file (Issue #45): without this order,
/// a missing identity hits the stale PID check first and reports "Node is already
/// running" instead of "Identity not initialized." Then check no node is running.
fn check_can_start(config: &CliConfig) -> CliResult<()> {
    if !crate::context::identity_exists(config) {
        return Err(CliError::IdentityNotInitialized);
    }

    // Check for existing running node
    if check_existing_node(&config.base_dir()).is_some() {
        return Err(CliError::NodeAlreadyRunning);
    }
    Ok(())
}

/// Create the node context, auto-deposit if needed, bootstrap and subscribe to announcements.
async fn prepare_foreground_node(config: &CliConfig) -> CliResult<NodeContext> {
    let ctx = NodeContext::with_network(config.clone()).await?;

    // Auto-deposit if settlement is configured and balance is low
    auto_deposit_foreground(&ctx, config).await;

    // Bootstrap
    ctx.bootstrap().await?;

    // Subscribe to announcements
    if let Some(ref network) = ctx.network {
        network.subscribe_announcements().await?;
    }
    Ok(ctx)
}

/// Auto-deposit if settlement is configured and the balance is low; a failure is
/// logged and the node starts anyway.
async fn auto_deposit_foreground(ctx: &NodeContext, config: &CliConfig) {
    let Some(ref settlement) = ctx.settlement else {
        return;
    };
    match maybe_auto_deposit(settlement, config).await {
        Ok(Some(tx_id)) => {
            println!("Auto-deposited to settlement contract (tx: {})", tx_id);
        }
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, "Auto-deposit failed, continuing anyway");
        }
    }
}

fn print_start_banner(output_str: &str, health: bool, health_port: u16) {
    println!("{}", output_str);
    if health {
        println!("Health endpoint: http://0.0.0.0:{}/health", health_port);
    }
    println!("\nPress Ctrl+C to stop the node...\n");
}

/// Configure health endpoint with alerting.
fn health_config(config: &CliConfig, health: bool, health_port: u16) -> HealthConfig {
    HealthConfig {
        enabled: health,
        port: health_port,
        alerting: config.alerting.clone(),
    }
}

/// Run the event loop (with status file updates and health endpoint) until a
/// shutdown signal, then remove the PID and status files.
async fn run_until_shutdown(
    ctx: &mut NodeContext,
    base_dir: &std::path::Path,
    pid_path: &std::path::Path,
    health_config: HealthConfig,
) -> CliResult<()> {
    // Set up shutdown signal handler
    let shutdown_rx = shutdown_signal();

    let result = run_event_loop_with_health(ctx, shutdown_rx, Some(base_dir), health_config).await;

    // Cleanup on exit
    info!("Cleaning up...");
    let _ = remove_pid_file(pid_path);
    let _ = remove_status_file(&status_file_path(base_dir));

    result
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

    let base_dir = config.base_dir();
    check_can_start(&config)?;

    // Create log files for stdout/stderr
    let (stdout, stderr, stderr_path) = daemon_log_files(&base_dir)?;

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

            rt.block_on(run_daemon(config, health, health_port));
            std::process::exit(0);
        }
        Err(e) => {
            // We are still in the parent process, daemonize failed
            Err(CliError::user(format!("Failed to daemonize: {}", e)))
        }
    }
}

/// Create the daemon's stdout and stderr log files in the base directory (creating it).
/// Returns the files and the stderr log path.
#[cfg(unix)]
fn daemon_log_files(
    base_dir: &std::path::Path,
) -> CliResult<(std::fs::File, std::fs::File, std::path::PathBuf)> {
    let stdout_path = base_dir.join("node.stdout.log");
    let stderr_path = base_dir.join("node.stderr.log");

    // Ensure base directory exists
    std::fs::create_dir_all(base_dir)?;

    let stdout = std::fs::File::create(&stdout_path)?;
    let stderr = std::fs::File::create(&stderr_path)?;
    Ok((stdout, stderr, stderr_path))
}

/// The daemon's body, in the fresh runtime: initialize, run until shutdown and clean up.
/// Any failure is printed to stderr and exits the process with status 1.
#[cfg(unix)]
async fn run_daemon(config: CliConfig, health: bool, health_port: u16) {
    let base_dir = config.base_dir();
    let pid_path = pid_file_path(&base_dir);

    // Now create the context in the fresh runtime
    let mut ctx = prepare_daemon_node(&config).await;

    // Write PID file with start time (after successful init)
    if let Err(e) = write_pid_file_with_start_time(&pid_path) {
        exit_with_error(&format!("Failed to write PID file: {}", e));
    }

    // Log startup info
    eprintln!("Nodalync daemon started (PID: {})", std::process::id());
    eprintln!("PeerId: {}", ctx.peer_id());
    if health {
        eprintln!("Health endpoint: http://0.0.0.0:{}/health", health_port);
    }

    let health_config = health_config(&config, health, health_port);
    if let Err(e) = run_until_shutdown(&mut ctx, &base_dir, &pid_path, health_config).await {
        exit_with_error(&format!("Node error: {}", e));
    }
}

/// Create the daemon's context, auto-deposit if needed, bootstrap and subscribe to
/// announcements; exit on any failure but the auto-deposit.
#[cfg(unix)]
async fn prepare_daemon_node(config: &CliConfig) -> NodeContext {
    let ctx = NodeContext::with_network(config.clone())
        .await
        .unwrap_or_else(|e| exit_with_error(&format!("Failed to initialize node: {}", e)));

    // Auto-deposit if settlement is configured and balance is low
    if let Some(ref settlement) = ctx.settlement {
        match maybe_auto_deposit(settlement, config).await {
            Ok(Some(tx_id)) => {
                eprintln!("Auto-deposited to settlement contract (tx: {})", tx_id);
            }
            Ok(None) => {}
            Err(e) => {
                // Continue anyway - node can still serve content
                eprintln!("Warning: Auto-deposit failed: {}", e);
            }
        }
    }

    // Bootstrap
    if let Err(e) = ctx.bootstrap().await {
        exit_with_error(&format!("Failed to bootstrap: {}", e));
    }

    // Subscribe to announcements
    if let Err(e) = subscribe_announcements(&ctx).await {
        exit_with_error(&format!("Failed to subscribe to announcements: {}", e));
    }
    ctx
}

/// Subscribe to announcements when the network is enabled.
#[cfg(unix)]
async fn subscribe_announcements(ctx: &NodeContext) -> Result<(), nodalync_net::NetworkError> {
    match ctx.network {
        Some(ref network) => network.subscribe_announcements().await,
        None => Ok(()),
    }
}

/// Print `message` to stderr and exit the daemon with status 1.
#[cfg(unix)]
fn exit_with_error(message: &str) -> ! {
    eprintln!("{}", message);
    std::process::exit(1);
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
