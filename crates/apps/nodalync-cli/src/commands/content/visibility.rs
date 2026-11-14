//! Change visibility command.

use nodalync_types::Visibility;

use crate::config::CliConfig;
use crate::context::{parse_hash, require_owned_manifest, NodeContext};
use crate::error::CliResult;
use crate::output::{OutputFormat, Render, VisibilityOutput};

/// Execute the visibility command.
pub async fn visibility(
    config: CliConfig,
    format: OutputFormat,
    hash_str: &str,
    new_visibility: Visibility,
) -> CliResult<String> {
    // Parse hash
    let hash = parse_hash(hash_str)?;

    // Initialize context with network (needed for DHT operations)
    let mut ctx = NodeContext::with_network(config).await?;

    // Verify content exists and we own it
    require_owned_manifest(&ctx, &hash, hash_str)?;

    // Set visibility
    ctx.ops.set_content_visibility(&hash, new_visibility)?;

    let output = VisibilityOutput {
        hash: hash.to_string(),
        new_visibility: format!("{:?}", new_visibility),
    };

    Ok(output.render(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_output() {
        let output = VisibilityOutput {
            hash: "abc123".to_string(),
            new_visibility: "Shared".to_string(),
        };

        let human = output.render(OutputFormat::Human);
        assert!(human.contains("Visibility updated"));
        assert!(human.contains("Shared"));

        let json = output.render(OutputFormat::Json);
        assert!(json.contains("\"new_visibility\""));
    }
}
