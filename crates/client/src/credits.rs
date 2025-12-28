//! Credit Manager for local credit tracking
//!
//! Tracks local credit consumption to avoid overuse before epoch reconciliation.
//! The CreditProof represents the user's credit balance at epoch end (signed by chain).
//! As requests are made, credits are consumed locally.
//!
//! ## Design
//!
//! - CreditProof is obtained from the chain (epoch-based balance)
//! - Local consumption is tracked per-request
//! - User is warned when approaching balance limit
//! - Post-epoch reconciliation handles actual settlement
//!
//! ## Usage
//!
//! ```ignore
//! let mut manager = CreditManager::new();
//!
//! // Set the chain-signed credit proof
//! manager.set_credit_proof(proof);
//!
//! // Before each request, estimate and reserve credits
//! let estimated = manager.estimate_request_cost(payload_size, hops);
//! if manager.can_afford(estimated) {
//!     manager.reserve(estimated);
//!     // ... send request ...
//!     manager.confirm_consumed(request_id, actual_cost);
//! }
//!
//! // Check remaining balance
//! let remaining = manager.available_credits();
//! ```

use std::collections::HashMap;
use tunnelcraft_core::{CreditProof, Id};
use tunnelcraft_erasure::TOTAL_SHARDS;

/// Cost per shard per hop (in credit units)
const COST_PER_SHARD_HOP: u64 = 1;

/// Base cost per request (overhead)
const BASE_REQUEST_COST: u64 = 5;

/// Credit Manager for tracking local credit consumption
#[derive(Debug)]
pub struct CreditManager {
    /// Current credit proof from chain
    credit_proof: Option<CreditProof>,
    /// Total consumed credits in this epoch
    consumed: u64,
    /// Reserved credits (pending confirmation)
    reserved: u64,
    /// Per-request reserved amounts
    reservations: HashMap<Id, u64>,
}

impl Default for CreditManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CreditManager {
    /// Create a new credit manager
    pub fn new() -> Self {
        Self {
            credit_proof: None,
            consumed: 0,
            reserved: 0,
            reservations: HashMap::new(),
        }
    }

    /// Set the chain-signed credit proof
    pub fn set_credit_proof(&mut self, proof: CreditProof) {
        // If epoch changed, reset consumption tracking
        if let Some(current) = &self.credit_proof {
            if current.epoch != proof.epoch {
                self.consumed = 0;
                self.reserved = 0;
                self.reservations.clear();
            }
        }
        self.credit_proof = Some(proof);
    }

    /// Get the current credit proof
    pub fn credit_proof(&self) -> Option<&CreditProof> {
        self.credit_proof.as_ref()
    }

    /// Get total balance from credit proof
    pub fn total_balance(&self) -> u64 {
        self.credit_proof.as_ref().map(|p| p.balance).unwrap_or(0)
    }

    /// Get available credits (balance - consumed - reserved)
    pub fn available_credits(&self) -> u64 {
        let total = self.total_balance();
        total.saturating_sub(self.consumed).saturating_sub(self.reserved)
    }

    /// Get consumed credits
    pub fn consumed_credits(&self) -> u64 {
        self.consumed
    }

    /// Get reserved credits
    pub fn reserved_credits(&self) -> u64 {
        self.reserved
    }

    /// Estimate cost for a request
    ///
    /// Cost = base + (shards * hops * cost_per_shard_hop)
    pub fn estimate_request_cost(&self, _payload_size: usize, hops: u8) -> u64 {
        let shard_cost = (TOTAL_SHARDS as u64) * (hops as u64) * COST_PER_SHARD_HOP;
        BASE_REQUEST_COST + shard_cost
    }

    /// Check if we can afford a given cost
    pub fn can_afford(&self, cost: u64) -> bool {
        self.available_credits() >= cost
    }

    /// Reserve credits for a pending request
    ///
    /// Returns false if insufficient credits
    pub fn reserve(&mut self, request_id: Id, amount: u64) -> bool {
        if !self.can_afford(amount) {
            return false;
        }
        self.reserved += amount;
        self.reservations.insert(request_id, amount);
        true
    }

    /// Confirm consumption of reserved credits
    ///
    /// If actual_cost differs from reservation, adjusts accordingly
    pub fn confirm_consumed(&mut self, request_id: &Id, actual_cost: u64) {
        if let Some(reserved) = self.reservations.remove(request_id) {
            self.reserved = self.reserved.saturating_sub(reserved);
            self.consumed += actual_cost;
        } else {
            // No reservation found, just consume directly
            self.consumed += actual_cost;
        }
    }

    /// Cancel a reservation (request failed/cancelled)
    pub fn cancel_reservation(&mut self, request_id: &Id) {
        if let Some(reserved) = self.reservations.remove(request_id) {
            self.reserved = self.reserved.saturating_sub(reserved);
        }
    }

    /// Get percentage of credits used
    pub fn usage_percentage(&self) -> f64 {
        let total = self.total_balance();
        if total == 0 {
            return 0.0;
        }
        let used = self.consumed + self.reserved;
        (used as f64 / total as f64) * 100.0
    }

    /// Check if credits are running low (>80% used)
    pub fn is_low(&self) -> bool {
        self.usage_percentage() > 80.0
    }

    /// Check if credits are critically low (>95% used)
    pub fn is_critical(&self) -> bool {
        self.usage_percentage() > 95.0
    }

    /// Reset consumption tracking (e.g., for new epoch)
    pub fn reset(&mut self) {
        self.consumed = 0;
        self.reserved = 0;
        self.reservations.clear();
    }

    /// Get current epoch (from credit proof)
    pub fn current_epoch(&self) -> Option<u64> {
        self.credit_proof.as_ref().map(|p| p.epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_credit_proof(balance: u64) -> CreditProof {
        CreditProof {
            user_pubkey: [1u8; 32],
            balance,
            epoch: 1,
            chain_signature: [0u8; 64],
        }
    }

    #[test]
    fn test_new_manager() {
        let manager = CreditManager::new();
        assert_eq!(manager.total_balance(), 0);
        assert_eq!(manager.available_credits(), 0);
    }

    #[test]
    fn test_set_credit_proof() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(1000));

        assert_eq!(manager.total_balance(), 1000);
        assert_eq!(manager.available_credits(), 1000);
        assert_eq!(manager.current_epoch(), Some(1));
    }

    #[test]
    fn test_estimate_request_cost() {
        let manager = CreditManager::new();

        // 5 shards, 2 hops: base(5) + 5*2*1 = 15
        let cost = manager.estimate_request_cost(1024, 2);
        assert_eq!(cost, 15);

        // 5 shards, 4 hops: base(5) + 5*4*1 = 25
        let cost = manager.estimate_request_cost(1024, 4);
        assert_eq!(cost, 25);
    }

    #[test]
    fn test_can_afford() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        assert!(manager.can_afford(50));
        assert!(manager.can_afford(100));
        assert!(!manager.can_afford(101));
    }

    #[test]
    fn test_reserve_and_confirm() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        let request_id = [1u8; 32];

        // Reserve 30 credits
        assert!(manager.reserve(request_id, 30));
        assert_eq!(manager.available_credits(), 70);
        assert_eq!(manager.reserved_credits(), 30);

        // Confirm consumption (actual was 25)
        manager.confirm_consumed(&request_id, 25);
        assert_eq!(manager.available_credits(), 75);
        assert_eq!(manager.reserved_credits(), 0);
        assert_eq!(manager.consumed_credits(), 25);
    }

    #[test]
    fn test_cancel_reservation() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        let request_id = [1u8; 32];

        manager.reserve(request_id, 30);
        assert_eq!(manager.available_credits(), 70);

        manager.cancel_reservation(&request_id);
        assert_eq!(manager.available_credits(), 100);
        assert_eq!(manager.reserved_credits(), 0);
    }

    #[test]
    fn test_insufficient_credits() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(50));

        let request_id = [1u8; 32];

        // Try to reserve more than available
        assert!(!manager.reserve(request_id, 60));
        assert_eq!(manager.reserved_credits(), 0);
    }

    #[test]
    fn test_usage_percentage() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        assert_eq!(manager.usage_percentage(), 0.0);

        let request_id = [1u8; 32];
        manager.reserve(request_id, 50);
        assert_eq!(manager.usage_percentage(), 50.0);

        manager.confirm_consumed(&request_id, 50);
        assert_eq!(manager.usage_percentage(), 50.0);
    }

    #[test]
    fn test_low_credits_warning() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        // Consume 75 credits (not low yet)
        manager.consumed = 75;
        assert!(!manager.is_low());

        // Consume 85 credits (low)
        manager.consumed = 85;
        assert!(manager.is_low());
        assert!(!manager.is_critical());

        // Consume 96 credits (critical)
        manager.consumed = 96;
        assert!(manager.is_critical());
    }

    #[test]
    fn test_epoch_change_resets_consumption() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        // Consume some credits
        manager.consumed = 50;
        assert_eq!(manager.available_credits(), 50);

        // New epoch with fresh balance
        let mut new_proof = test_credit_proof(200);
        new_proof.epoch = 2;
        manager.set_credit_proof(new_proof);

        // Consumption should be reset
        assert_eq!(manager.consumed_credits(), 0);
        assert_eq!(manager.available_credits(), 200);
        assert_eq!(manager.current_epoch(), Some(2));
    }

    #[test]
    fn test_reset() {
        let mut manager = CreditManager::new();
        manager.set_credit_proof(test_credit_proof(100));

        manager.consumed = 30;
        manager.reserved = 20;
        manager.reservations.insert([1u8; 32], 20);

        manager.reset();

        assert_eq!(manager.consumed_credits(), 0);
        assert_eq!(manager.reserved_credits(), 0);
        assert_eq!(manager.available_credits(), 100);
    }
}
