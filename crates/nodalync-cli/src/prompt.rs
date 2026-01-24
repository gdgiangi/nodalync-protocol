//! Interactive prompt utilities for CLI commands.

use dialoguer::{theme::ColorfulTheme, Confirm, Password};
use std::io::{self, IsTerminal};

/// Check if we're running in an interactive terminal.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

/// Prompt for confirmation with a yes/no question.
///
/// Returns `Ok(true)` if user confirmed, `Ok(false)` if declined.
pub fn confirm(prompt: &str) -> io::Result<bool> {
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Prompt for a password (hidden input).
pub fn password(prompt: &str) -> io::Result<String> {
    Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact()
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Prompt for a password with confirmation.
///
/// The user must enter the same password twice for it to be accepted.
pub fn password_with_confirm(prompt: &str) -> io::Result<String> {
    Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .with_confirmation("Confirm password", "Passwords do not match")
        .interact()
        .map_err(|e| io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_interactive() {
        // In test environment, stdin is typically not a terminal
        // This test just verifies the function doesn't panic
        let _ = is_interactive();
    }
}
