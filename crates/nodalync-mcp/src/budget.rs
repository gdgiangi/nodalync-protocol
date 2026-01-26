//! Budget tracking for MCP sessions.
//!
//! Tracks spending against a session budget to ensure AI assistants
//! don't overspend on queries.

use nodalync_types::Amount;
use std::sync::atomic::{AtomicU64, Ordering};

/// Tinybars per HBAR (10^8).
pub const TINYBARS_PER_HBAR: u64 = 100_000_000;

/// Default auto-approve threshold in HBAR.
pub const DEFAULT_AUTO_APPROVE_HBAR: f64 = 0.01;

/// Budget tracker for MCP sessions.
///
/// Thread-safe budget tracking with atomic operations.
#[derive(Debug)]
pub struct BudgetTracker {
    /// Total budget in tinybars.
    total_budget: Amount,
    /// Amount spent so far in tinybars.
    spent: AtomicU64,
    /// Auto-approve threshold in tinybars.
    auto_approve_threshold: Amount,
}

impl BudgetTracker {
    /// Create a new budget tracker with the given budget in HBAR.
    pub fn new(budget_hbar: f64) -> Self {
        let total_budget = hbar_to_tinybars(budget_hbar);
        let auto_approve_threshold = hbar_to_tinybars(DEFAULT_AUTO_APPROVE_HBAR);

        Self {
            total_budget,
            spent: AtomicU64::new(0),
            auto_approve_threshold,
        }
    }

    /// Create a budget tracker with a custom auto-approve threshold.
    pub fn with_auto_approve(budget_hbar: f64, auto_approve_hbar: f64) -> Self {
        let total_budget = hbar_to_tinybars(budget_hbar);
        let auto_approve_threshold = hbar_to_tinybars(auto_approve_hbar);

        Self {
            total_budget,
            spent: AtomicU64::new(0),
            auto_approve_threshold,
        }
    }

    /// Get the total budget in tinybars.
    pub fn total_budget(&self) -> Amount {
        self.total_budget
    }

    /// Get the total budget in HBAR.
    pub fn total_budget_hbar(&self) -> f64 {
        tinybars_to_hbar(self.total_budget)
    }

    /// Get the amount spent so far in tinybars.
    pub fn spent(&self) -> Amount {
        self.spent.load(Ordering::Relaxed)
    }

    /// Get the amount spent so far in HBAR.
    pub fn spent_hbar(&self) -> f64 {
        tinybars_to_hbar(self.spent())
    }

    /// Get the remaining budget in tinybars.
    pub fn remaining(&self) -> Amount {
        self.total_budget.saturating_sub(self.spent())
    }

    /// Get the remaining budget in HBAR.
    pub fn remaining_hbar(&self) -> f64 {
        tinybars_to_hbar(self.remaining())
    }

    /// Check if a query can be auto-approved (under threshold).
    pub fn can_auto_approve(&self, cost: Amount) -> bool {
        cost <= self.auto_approve_threshold && self.can_afford(cost)
    }

    /// Check if the budget can afford a given cost.
    pub fn can_afford(&self, cost: Amount) -> bool {
        cost <= self.remaining()
    }

    /// Record a spend, returning the new total spent.
    ///
    /// Returns `None` if the spend would exceed the budget.
    pub fn spend(&self, amount: Amount) -> Option<Amount> {
        // Use compare-and-swap to atomically check and update
        loop {
            let current = self.spent.load(Ordering::Relaxed);
            let new_spent = current.checked_add(amount)?;

            if new_spent > self.total_budget {
                return None;
            }

            if self
                .spent
                .compare_exchange(current, new_spent, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Some(new_spent);
            }
            // Retry if another thread modified spent
        }
    }

    /// Refund a previously spent amount (e.g., on query failure).
    ///
    /// This atomically decreases the spent amount. Used when a query
    /// fails after budget was reserved.
    pub fn refund(&self, amount: Amount) {
        loop {
            let current = self.spent.load(Ordering::Relaxed);
            let new_spent = current.saturating_sub(amount);

            if self
                .spent
                .compare_exchange(current, new_spent, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            // Retry if another thread modified spent
        }
    }

    /// Get the auto-approve threshold in HBAR.
    pub fn auto_approve_threshold_hbar(&self) -> f64 {
        tinybars_to_hbar(self.auto_approve_threshold)
    }

    /// Get budget status as a human-readable string.
    pub fn status(&self) -> String {
        format!(
            "Budget: {:.6} HBAR remaining ({:.6} / {:.6} HBAR spent)",
            self.remaining_hbar(),
            self.spent_hbar(),
            self.total_budget_hbar()
        )
    }

    /// Get budget status as a structured object for JSON serialization.
    pub fn status_json(&self) -> BudgetStatus {
        BudgetStatus {
            total_hbar: self.total_budget_hbar(),
            spent_hbar: self.spent_hbar(),
            remaining_hbar: self.remaining_hbar(),
            auto_approve_hbar: self.auto_approve_threshold_hbar(),
        }
    }
}

/// Structured budget status for JSON serialization.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BudgetStatus {
    /// Total session budget in HBAR.
    pub total_hbar: f64,
    /// Amount spent so far in HBAR.
    pub spent_hbar: f64,
    /// Remaining budget in HBAR.
    pub remaining_hbar: f64,
    /// Auto-approve threshold in HBAR.
    pub auto_approve_hbar: f64,
}

/// Convert HBAR to tinybars.
pub fn hbar_to_tinybars(hbar: f64) -> Amount {
    (hbar * TINYBARS_PER_HBAR as f64) as Amount
}

/// Convert tinybars to HBAR.
pub fn tinybars_to_hbar(tinybars: Amount) -> f64 {
    tinybars as f64 / TINYBARS_PER_HBAR as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hbar_conversion() {
        assert_eq!(hbar_to_tinybars(1.0), 100_000_000);
        assert_eq!(hbar_to_tinybars(0.5), 50_000_000);
        assert_eq!(hbar_to_tinybars(0.01), 1_000_000);

        assert_eq!(tinybars_to_hbar(100_000_000), 1.0);
        assert_eq!(tinybars_to_hbar(50_000_000), 0.5);
        assert_eq!(tinybars_to_hbar(1_000_000), 0.01);
    }

    #[test]
    fn test_budget_tracker_new() {
        let tracker = BudgetTracker::new(1.0);

        assert_eq!(tracker.total_budget(), 100_000_000);
        assert_eq!(tracker.spent(), 0);
        assert_eq!(tracker.remaining(), 100_000_000);
    }

    #[test]
    fn test_budget_tracker_spend() {
        let tracker = BudgetTracker::new(1.0);

        // Spend some
        let result = tracker.spend(50_000_000);
        assert_eq!(result, Some(50_000_000));
        assert_eq!(tracker.spent(), 50_000_000);
        assert_eq!(tracker.remaining(), 50_000_000);

        // Spend more
        let result = tracker.spend(25_000_000);
        assert_eq!(result, Some(75_000_000));
        assert_eq!(tracker.remaining(), 25_000_000);

        // Try to overspend
        let result = tracker.spend(50_000_000);
        assert_eq!(result, None);
        assert_eq!(tracker.spent(), 75_000_000); // Unchanged
    }

    #[test]
    fn test_budget_tracker_auto_approve() {
        let tracker = BudgetTracker::new(1.0);

        // Under threshold - auto approve
        assert!(tracker.can_auto_approve(500_000)); // 0.005 HBAR

        // At threshold - auto approve
        assert!(tracker.can_auto_approve(1_000_000)); // 0.01 HBAR

        // Over threshold - no auto approve
        assert!(!tracker.can_auto_approve(2_000_000)); // 0.02 HBAR
    }

    #[test]
    fn test_budget_tracker_status() {
        let tracker = BudgetTracker::new(1.0);
        tracker.spend(50_000_000).unwrap();

        let status = tracker.status();
        assert!(status.contains("0.5"));
        assert!(status.contains("1.0"));
    }

    #[test]
    fn test_budget_tracker_refund() {
        let tracker = BudgetTracker::new(1.0);

        // Spend some
        tracker.spend(50_000_000).unwrap();
        assert_eq!(tracker.spent(), 50_000_000);
        assert_eq!(tracker.remaining(), 50_000_000);

        // Refund partial amount
        tracker.refund(20_000_000);
        assert_eq!(tracker.spent(), 30_000_000);
        assert_eq!(tracker.remaining(), 70_000_000);

        // Refund more than spent (saturates to 0)
        tracker.refund(100_000_000);
        assert_eq!(tracker.spent(), 0);
        assert_eq!(tracker.remaining(), 100_000_000);
    }

    #[test]
    fn test_budget_status_json() {
        let tracker = BudgetTracker::with_auto_approve(1.0, 0.05);
        tracker.spend(25_000_000).unwrap();

        let status = tracker.status_json();
        assert_eq!(status.total_hbar, 1.0);
        assert_eq!(status.spent_hbar, 0.25);
        assert_eq!(status.remaining_hbar, 0.75);
        assert_eq!(status.auto_approve_hbar, 0.05);
    }
}
