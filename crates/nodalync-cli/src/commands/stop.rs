//! Stop node command.

use crate::config::CliConfig;
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, StopOutput};

/// Execute the stop command.
pub async fn stop(_config: CliConfig, format: OutputFormat) -> CliResult<String> {
    // In a full implementation, this would:
    // 1. Find the running node process (via PID file or socket)
    // 2. Send a shutdown signal
    // 3. Wait for graceful shutdown

    // For now, we just report success
    // A real implementation would use:
    // - PID file at ~/.nodalync/node.pid
    // - Or a control socket at ~/.nodalync/control.sock

    let output = StopOutput { success: true };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_output() {
        let output = StopOutput { success: true };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Node stopped"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"success\": true"));
    }

    #[test]
    fn test_stop_output_failed() {
        let output = StopOutput { success: false };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Failed"));
    }
}
