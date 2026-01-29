//! Nodalync CLI binary entry point.

use clap::Parser;
use colored::Colorize;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use nodalync_cli::{
    cli::{Cli, Commands},
    commands,
    config::{default_config_path, CliConfig},
    error::{CliError, CliResult},
    output::OutputFormat,
};

fn main() {
    // Parse CLI arguments BEFORE creating tokio runtime
    let cli = Cli::parse();

    // Check if this is a daemon start - must be handled before tokio runtime
    if let Commands::Start {
        daemon: true,
        health,
        health_port,
    } = &cli.command
    {
        // Handle daemon mode synchronously before any async runtime exists
        if let Err(e) = handle_daemon_start(&cli, *health, *health_port) {
            print_error(&e);
            std::process::exit(e.exit_code());
        }
        return;
    }

    // For all other commands, use the normal async runtime
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async_main(cli));
}

async fn async_main(cli: Cli) {
    // Initialize logging based on --verbose flag or RUST_LOG env var
    let has_rust_log = std::env::var("RUST_LOG").is_ok();
    if cli.verbose || has_rust_log {
        let filter = if cli.verbose {
            EnvFilter::from_default_env().add_directive("nodalync=debug".parse().unwrap())
        } else {
            EnvFilter::from_default_env()
        };
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(filter)
            .init();
    }

    // Run the command
    if let Err(e) = run(cli).await {
        print_error(&e);
        std::process::exit(e.exit_code());
    }
}

/// Print a user-friendly error message with error code and recovery hint.
fn print_error(e: &CliError) {
    let code = e.error_code();

    // Error line with code
    eprintln!(
        "{} [{}]: {}",
        "Error".red().bold(),
        code.to_string().yellow(),
        e
    );

    // Suggestion if available
    if let Some(suggestion) = code.suggestion() {
        eprintln!("{}: {}", "Hint".cyan(), suggestion);
    }
}

/// Handle daemon start before tokio runtime is created.
/// This avoids the "cannot start runtime from within runtime" panic.
fn handle_daemon_start(cli: &Cli, health: bool, health_port: u16) -> CliResult<()> {
    use nodalync_cli::commands::start_daemon_sync;

    // Initialize logging based on --verbose flag or RUST_LOG env var
    let has_rust_log = std::env::var("RUST_LOG").is_ok();
    if cli.verbose || has_rust_log {
        let filter = if cli.verbose {
            EnvFilter::from_default_env().add_directive("nodalync=debug".parse().unwrap())
        } else {
            EnvFilter::from_default_env()
        };
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(filter)
            .init();
    }

    // Load configuration
    let config_path = cli.config.clone().unwrap_or_else(default_config_path);
    let config = CliConfig::load(&config_path)?;
    let format: OutputFormat = cli.format.into();

    // Call the synchronous daemon start function
    // Note: On success, the parent process exits inside this call after forking.
    // The child process runs the daemon and never returns here.
    // Only on error does this function return.
    start_daemon_sync(config, format, health, health_port)?;

    // If we get here, something unexpected happened
    Ok(())
}

async fn run(cli: Cli) -> CliResult<()> {
    // Load configuration
    let config_path = cli.config.unwrap_or_else(default_config_path);
    let config = CliConfig::load(&config_path)?;

    // Get output format
    let format: OutputFormat = cli.format.into();

    // Dispatch command
    let output = match cli.command {
        // Identity commands
        Commands::Init { wizard } => commands::init(config, format, wizard)?,

        Commands::Whoami => commands::whoami(config, format)?,

        // Content management commands
        Commands::Publish {
            file,
            price,
            visibility,
            title,
            description,
        } => {
            commands::publish(
                config,
                format,
                &file,
                price,
                visibility.into(),
                title,
                description,
            )
            .await?
        }

        Commands::List {
            visibility,
            content_type,
            limit,
            network,
        } => {
            commands::list(
                config,
                format,
                visibility.map(Into::into),
                content_type.map(Into::into),
                limit,
                network,
            )
            .await?
        }

        Commands::Update { hash, file, title } => {
            commands::update(config, format, &hash, &file, title)?
        }

        Commands::Visibility { hash, level } => {
            commands::visibility(config, format, &hash, level.into()).await?
        }

        Commands::Versions { hash } => commands::versions(config, format, &hash)?,

        Commands::Delete { hash, force } => commands::delete(config, format, &hash, force)?,

        // Discovery & query commands
        Commands::Preview { hash } => commands::preview(config, format, &hash).await?,

        Commands::Query { hash, output } => commands::query(config, format, &hash, output).await?,

        // Synthesis commands
        Commands::Synthesize {
            sources,
            output,
            title,
            price,
            publish,
        } => commands::synthesize(config, format, &sources, &output, title, price, publish).await?,

        Commands::BuildL2 { sources, title } => {
            commands::build_l2(config, format, &sources, title)?
        }

        Commands::MergeL2 { graphs, title } => commands::merge_l2(config, format, &graphs, title)?,

        Commands::Reference { hash } => commands::reference(config, format, &hash)?,

        // Economics commands
        Commands::Balance => commands::balance(config, format).await?,

        Commands::Earnings { content, limit } => {
            commands::earnings(config, format, content, limit)?
        }

        Commands::Deposit { amount } => commands::deposit(config, format, amount).await?,

        Commands::Withdraw { amount } => commands::withdraw(config, format, amount).await?,

        Commands::Settle => commands::settle(config, format).await?,

        // Channel commands
        Commands::OpenChannel { peer_id, deposit } => {
            commands::open_channel(config, format, &peer_id, deposit).await?
        }

        Commands::CloseChannel { peer_id } => {
            commands::close_channel(config, format, &peer_id).await?
        }

        Commands::ListChannels => commands::list_channels(config, format)?,

        // Node management commands
        Commands::Start {
            daemon,
            health,
            health_port,
        } => commands::start(config, format, daemon, health, health_port).await?,

        Commands::Status => commands::status(config, format).await?,

        Commands::Stop => commands::stop(config, format).await?,

        // MCP server command
        Commands::McpServer {
            budget,
            auto_approve,
            enable_network,
            hedera_account_id,
            hedera_private_key,
            hedera_contract_id,
            hedera_network,
        } => {
            let hedera_args = commands::mcp_server::HederaArgs {
                account_id: hedera_account_id,
                private_key: hedera_private_key,
                contract_id: hedera_contract_id,
                network: hedera_network,
            };
            commands::mcp_server(config, budget, auto_approve, enable_network, hedera_args).await?
        }

        // Search command
        Commands::Search {
            query,
            content_type,
            limit,
            all,
        } => {
            commands::search(
                config,
                format,
                &query,
                content_type.map(Into::into),
                limit,
                all,
            )
            .await?
        }

        // Completions command
        Commands::Completions { shell } => commands::completions(shell)?,
    };

    // Print output
    println!("{}", output);

    Ok(())
}
