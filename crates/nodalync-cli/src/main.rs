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

#[tokio::main]
async fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

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

        // Economics commands
        Commands::Balance => commands::balance(config, format).await?,

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
