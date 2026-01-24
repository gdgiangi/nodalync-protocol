//! Start node command.

use crate::config::CliConfig;
use crate::context::NodeContext;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, StartOutput};

/// Execute the start command.
pub async fn start(config: CliConfig, format: OutputFormat, daemon: bool) -> CliResult<String> {
    // Initialize context with network
    let ctx = NodeContext::with_network(config.clone()).await?;

    // Bootstrap
    ctx.bootstrap().await?;

    // Get listen addresses
    let listen_addresses = config.network.listen_addresses.clone();

    // Get connected peers
    let connected_peers = ctx.connected_peers() as u32;

    let output = StartOutput {
        peer_id: ctx.peer_id().to_string(),
        listen_addresses,
        connected_peers,
        daemon,
    };

    // If not daemon mode, we'd normally run the event loop here
    // For CLI simplicity, we just report the start
    if !daemon {
        // In a full implementation, this would:
        // 1. Subscribe to announcements
        // 2. Run the event loop
        // 3. Handle incoming requests
        // For now, we just print the startup info

        // The event loop would look like:
        // loop {
        //     let event = ctx.network.as_ref().unwrap().next_event().await?;
        //     // Handle event
        // }
    }

    Ok(output.render(format))
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
