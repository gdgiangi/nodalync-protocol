//! Shell completions command.

use std::io;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::{Cli, CompletionShell};
use crate::error::CliResult;

/// Generate shell completions for the specified shell.
pub fn completions(shell: CompletionShell) -> CliResult<String> {
    let mut cmd = Cli::command();
    let shell = match shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
        CompletionShell::PowerShell => Shell::PowerShell,
    };

    generate(shell, &mut cmd, "nodalync", &mut io::stdout());

    // Return empty string since output is written to stdout
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completions_bash() {
        // Just verify it doesn't panic
        let _ = completions(CompletionShell::Bash);
    }

    #[test]
    fn test_completions_zsh() {
        let _ = completions(CompletionShell::Zsh);
    }

    #[test]
    fn test_completions_fish() {
        let _ = completions(CompletionShell::Fish);
    }

    #[test]
    fn test_completions_powershell() {
        let _ = completions(CompletionShell::PowerShell);
    }
}
