//! Nodalync CLI binary entry point.

use clap::Parser;
use colored::Colorize;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use nodalync_cli::{
    cli::{Cli, Commands},
    commands,
    config::{default_config_path, CliConfig},
    error::CliResult,
    output::OutputFormat,
};

fn main() {
    // Parse CLI arguments BEFORE creating tokio runtime
    let cli = Cli::parse();

    // Check if this is a daemon start - must be handled before tokio runtime
    if let Commands::Start { daemon: true } = &cli.command {
        // Handle daemon mode synchronously before any async runtime exists
        if let Err(e) = handle_daemon_start(&cli) {
            eprintln!("{}: {}", "Error".red().bold(), e);
            std::process::exit(e.exit_code());
        }
        return;
    }

    // For all other commands, use the normal async runtime
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async_main(cli));
}

async fn async_main(cli: Cli) {
    // Initialize logging
    if cli.verbose {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env().add_directive("nodalync=debug".parse().unwrap()))
            .init();
    }

    // Run the command
    if let Err(e) = run(cli).await {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(e.exit_code());
    }
}

/// Handle daemon start before tokio runtime is created.
/// This avoids the "cannot start runtime from within runtime" panic.
fn handle_daemon_start(cli: &Cli) -> CliResult<()> {
    use nodalync_cli::commands::start_daemon_sync;

    // Initialize logging if verbose
    if cli.verbose {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env().add_directive("nodalync=debug".parse().unwrap()))
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
    start_daemon_sync(config, format)?;

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
        Commands::Init => commands::init(config, format)?,

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
        } => commands::list(
            config,
            format,
            visibility.map(Into::into),
            content_type.map(Into::into),
            limit,
        )?,

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

        // Node management commands
        Commands::Start { daemon } => commands::start(config, format, daemon).await?,

        Commands::Status => commands::status(config, format).await?,

        Commands::Stop => commands::stop(config, format).await?,
    };

    // Print output
    println!("{}", output);

    Ok(())
}
