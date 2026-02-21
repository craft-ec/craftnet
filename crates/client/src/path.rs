//! Client path selection for onion routing
//!
//! Uses the topology graph to select valid multi-hop paths where each
//! consecutive hop is connected in the topology.

use std::collections::HashSet;

use rand::seq::SliceRandom;
use rand::Rng;

use craftnet_core::{Id, PublicKey};
use crate::{ClientError, Result};

/// A single hop in an onion path
#[derive(Debug, Clone)]
pub struct PathHop {
    /// libp2p PeerId bytes
    pub peer_id: Vec<u8>,
    /// Ed25519 signing pubkey
    pub signing_pubkey: PublicKey,
    /// X25519 encryption pubkey (for onion layer ECDH)
    pub encryption_pubkey: [u8; 32],
}

/// A complete onion path from first relay to exit
#[derive(Debug, Clone)]
pub struct OnionPath {
    /// Relay hops (first relay → last relay)
    pub hops: Vec<PathHop>,
    /// Exit node (final destination)
    pub exit: PathHop,
}

/// Relay info stored in topology graph
#[derive(Debug, Clone)]
pub struct TopologyRelay {
    pub peer_id: Vec<u8>,
    pub signing_pubkey: PublicKey,
    pub encryption_pubkey: [u8; 32],
    pub connected_peers: HashSet<Vec<u8>>,
    pub last_seen: std::time::Instant,
}

/// Topology graph built from gossipsub topology messages
pub struct TopologyGraph {
    relays: Vec<TopologyRelay>,
}

impl TopologyGraph {
    pub fn new() -> Self {
        Self { relays: Vec::new() }
    }

    /// Update or insert a relay into the topology
    pub fn update_relay(&mut self, relay: TopologyRelay) {
        if let Some(existing) = self.relays.iter_mut().find(|r| r.peer_id == relay.peer_id) {
            existing.signing_pubkey = relay.signing_pubkey;
            existing.encryption_pubkey = relay.encryption_pubkey;
            existing.connected_peers = relay.connected_peers;
            existing.last_seen = relay.last_seen;
        } else {
            self.relays.push(relay);
        }
    }

    /// Remove stale relays not seen within max_age
    pub fn prune_stale(&mut self, max_age: std::time::Duration) {
        let now = std::time::Instant::now();
        self.relays.retain(|r| now.duration_since(r.last_seen) < max_age);
    }

    /// Check if two peers are connected according to topology
    pub fn is_connected(&self, a: &[u8], b: &[u8]) -> bool {
        if let Some(relay_a) = self.get_relay(a) {
            if relay_a.connected_peers.contains(b) {
                return true;
            }
        }
        if let Some(relay_b) = self.get_relay(b) {
            if relay_b.connected_peers.contains(a) {
                return true;
            }
        }
        false
    }

    /// Get a relay by peer_id
    pub fn get_relay(&self, peer_id: &[u8]) -> Option<&TopologyRelay> {
        self.relays.iter().find(|r| r.peer_id == peer_id)
    }

    /// Get all relays with encryption pubkeys
    pub fn relays_with_encryption(&self) -> Vec<&TopologyRelay> {
        self.relays.iter().filter(|r| r.encryption_pubkey != [0u8; 32]).collect()
    }

    /// Get number of relays
    pub fn len(&self) -> usize {
        self.relays.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.relays.is_empty()
    }
}

impl Default for TopologyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Path selection utilities
pub struct PathSelector;

impl PathSelector {
    /// Select a path of `hop_count` relays to the exit.
    ///
    /// Each consecutive hop must be connected in the topology.
    /// The last relay must be connected to the exit.
    ///
    /// `entry_peer`: if provided, the first hop must be connected to this peer
    /// (used to ensure the first relay hop is reachable from the gateway).
    pub fn select_path(
        topology: &TopologyGraph,
        hop_count: usize,
        exit: &PathHop,
        exclude: &HashSet<Vec<u8>>,
        entry_peer: Option<&[u8]>,
    ) -> Result<OnionPath> {
        if hop_count == 0 {
            return Ok(OnionPath {
                hops: vec![],
                exit: exit.clone(),
            });
        }

        let eligible: Vec<&TopologyRelay> = topology
            .relays_with_encryption()
            .into_iter()
            .filter(|r| !exclude.contains(&r.peer_id))
            // Exclude the exit itself — it cannot relay for its own circuit
            // (shard would arrive with non-empty header, get relayed to self, and dropped)
            .filter(|r| r.peer_id != exit.peer_id)
            .collect();

        if eligible.len() < hop_count {
            return Err(ClientError::RequestFailed(format!(
                "Insufficient relays: need {} but only {} available",
                hop_count,
                eligible.len()
            )));
        }

        let mut rng = rand::thread_rng();

        // Try random walk multiple times
        for _ in 0..100 {
            let mut path: Vec<PathHop> = Vec::new();
            let mut used: HashSet<Vec<u8>> = HashSet::new();
            let mut valid = true;

            // Randomly pick relays
            let mut candidates: Vec<&&TopologyRelay> = eligible.iter().collect();
            candidates.shuffle(&mut rng);

            for i in 0..hop_count {
                // Find a relay connected to the previous hop
                let found = candidates.iter().find(|&&relay| {
                    if used.contains(&relay.peer_id) {
                        return false;
                    }
                    if i == 0 {
                        // First hop: must be connected to entry_peer (gateway)
                        if let Some(entry) = entry_peer {
                            return topology.is_connected(entry, &relay.peer_id);
                        }
                        return true;
                    }
                    // Must be connected to previous hop
                    topology.is_connected(&path[i - 1].peer_id, &relay.peer_id)
                });

                if let Some(&&relay) = found {
                    used.insert(relay.peer_id.clone());
                    path.push(PathHop {
                        peer_id: relay.peer_id.clone(),
                        signing_pubkey: relay.signing_pubkey,
                        encryption_pubkey: relay.encryption_pubkey,
                    });
                } else {
                    valid = false;
                    break;
                }
            }

            if !valid {
                continue;
            }

            // Last relay must be connected to exit
            let last_relay = &path[path.len() - 1];
            if !topology.is_connected(&last_relay.peer_id, &exit.peer_id) {
                continue;
            }

            return Ok(OnionPath {
                hops: path,
                exit: exit.clone(),
            });
        }

        Err(ClientError::RequestFailed(
            "Could not find valid path through topology (no connected chain)".to_string(),
        ))
    }

    /// Select N diverse paths (minimize relay overlap).
    ///
    /// `entry_peer`: if provided, the first hop of each path must be connected
    /// to this peer in topology (used for gateway connectivity).
    pub fn select_diverse_paths(
        topology: &TopologyGraph,
        hop_count: usize,
        exit: &PathHop,
        count: usize,
        entry_peer: Option<&[u8]>,
    ) -> Result<Vec<OnionPath>> {
        let mut paths = Vec::new();
        let mut used_relays: HashSet<Vec<u8>> = HashSet::new();

        for _ in 0..count {
            // Try with excluding previously used relays first
            match Self::select_path(topology, hop_count, exit, &used_relays, entry_peer) {
                Ok(path) => {
                    for hop in &path.hops {
                        used_relays.insert(hop.peer_id.clone());
                    }
                    paths.push(path);
                }
                Err(_) => {
                    // Fallback: allow relay reuse
                    let path = Self::select_path(topology, hop_count, exit, &HashSet::new(), entry_peer)?;
                    paths.push(path);
                }
            }
        }

        Ok(paths)
    }

    /// Select gateway relays for the lease set (relays the client is directly connected to).
    pub fn select_gateways(
        topology: &TopologyGraph,
        count: usize,
        our_peer_id: &[u8],
    ) -> Result<Vec<PathHop>> {
        let mut rng = rand::thread_rng();
        let mut eligible: Vec<&TopologyRelay> = topology
            .relays_with_encryption()
            .into_iter()
            .filter(|r| r.connected_peers.contains(our_peer_id) || topology.is_connected(&r.peer_id, our_peer_id))
            .collect();

        eligible.shuffle(&mut rng);

        let selected: Vec<PathHop> = eligible
            .into_iter()
            .take(count)
            .map(|r| PathHop {
                peer_id: r.peer_id.clone(),
                signing_pubkey: r.signing_pubkey,
                encryption_pubkey: r.encryption_pubkey,
            })
            .collect();

        if selected.is_empty() {
            return Err(ClientError::RequestFailed(
                "No gateways available (no connected relays with encryption keys)".to_string(),
            ));
        }

        Ok(selected)
    }
}

/// Generate a random 32-byte ID
pub fn random_id() -> Id {
    let mut rng = rand::thread_rng();
    let mut id = [0u8; 32];
    rng.fill(&mut id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn make_relay(id: u8) -> TopologyRelay {
        TopologyRelay {
            peer_id: vec![id],
            signing_pubkey: [id; 32],
            encryption_pubkey: [id + 100; 32],
            connected_peers: HashSet::new(),
            last_seen: Instant::now(),
        }
    }

    fn make_exit(id: u8) -> PathHop {
        PathHop {
            peer_id: vec![id],
            signing_pubkey: [id; 32],
            encryption_pubkey: [id + 100; 32],
        }
    }

    #[test]
    fn test_topology_graph_basics() {
        let mut graph = TopologyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);

        graph.update_relay(make_relay(1));
        assert_eq!(graph.len(), 1);
        assert!(!graph.is_empty());

        // Update existing
        let mut relay = make_relay(1);
        relay.encryption_pubkey = [42u8; 32];
        graph.update_relay(relay);
        assert_eq!(graph.len(), 1);
        assert_eq!(graph.get_relay(&[1]).unwrap().encryption_pubkey, [42u8; 32]);
    }

    #[test]
    fn test_topology_connectivity() {
        let mut graph = TopologyGraph::new();

        let mut r1 = make_relay(1);
        r1.connected_peers.insert(vec![2]);
        graph.update_relay(r1);

        let r2 = make_relay(2);
        graph.update_relay(r2);

        assert!(graph.is_connected(&[1], &[2]));
        assert!(graph.is_connected(&[2], &[1])); // bidirectional check
        assert!(!graph.is_connected(&[1], &[3]));
    }

    #[test]
    fn test_select_path_zero_hops() {
        let graph = TopologyGraph::new();
        let exit = make_exit(10);

        let path = PathSelector::select_path(&graph, 0, &exit, &HashSet::new(), None).unwrap();
        assert!(path.hops.is_empty());
        assert_eq!(path.exit.peer_id, vec![10]);
    }

    #[test]
    fn test_select_path_one_hop() {
        let mut graph = TopologyGraph::new();

        let mut r1 = make_relay(1);
        r1.connected_peers.insert(vec![10]); // connected to exit
        graph.update_relay(r1);

        let exit = make_exit(10);
        let path = PathSelector::select_path(&graph, 1, &exit, &HashSet::new(), None).unwrap();

        assert_eq!(path.hops.len(), 1);
        assert_eq!(path.hops[0].peer_id, vec![1]);
    }

    #[test]
    fn test_select_path_two_hops() {
        let mut graph = TopologyGraph::new();

        let mut r1 = make_relay(1);
        r1.connected_peers.insert(vec![2]);
        graph.update_relay(r1);

        let mut r2 = make_relay(2);
        r2.connected_peers.insert(vec![10]); // connected to exit
        graph.update_relay(r2);

        let exit = make_exit(10);
        let path = PathSelector::select_path(&graph, 2, &exit, &HashSet::new(), None).unwrap();

        assert_eq!(path.hops.len(), 2);
    }

    #[test]
    fn test_select_path_insufficient_relays() {
        let graph = TopologyGraph::new();
        let exit = make_exit(10);

        let result = PathSelector::select_path(&graph, 2, &exit, &HashSet::new(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_select_diverse_paths() {
        let mut graph = TopologyGraph::new();

        // Create a fully connected topology of 6 relays + exit
        for i in 1..=6 {
            let mut relay = make_relay(i);
            for j in 1..=6 {
                if i != j {
                    relay.connected_peers.insert(vec![j]);
                }
            }
            relay.connected_peers.insert(vec![10]); // connected to exit
            graph.update_relay(relay);
        }

        let exit = make_exit(10);
        let paths = PathSelector::select_diverse_paths(&graph, 1, &exit, 3, None).unwrap();

        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn test_prune_stale() {
        let mut graph = TopologyGraph::new();

        let mut r1 = make_relay(1);
        r1.last_seen = Instant::now() - std::time::Duration::from_secs(600);
        graph.update_relay(r1);

        graph.update_relay(make_relay(2)); // fresh

        assert_eq!(graph.len(), 2);
        graph.prune_stale(std::time::Duration::from_secs(300));
        assert_eq!(graph.len(), 1);
        assert!(graph.get_relay(&[2]).is_some());
    }

    #[test]
    fn test_random_id() {
        let id1 = random_id();
        let id2 = random_id();
        assert_ne!(id1, id2);
    }

    /// Simulate the E2E topology: 5 relays + 3 exits (exits added as relays).
    /// Prove that exits can be selected as intermediate relay hops when
    /// the exit's peer_id is NOT excluded from eligible relays.
    #[test]
    fn test_exit_in_topology_can_be_selected_as_relay_hop() {
        let mut graph = TopologyGraph::new();

        // 5 relays: ids 1..=5, fully connected to each other + exits
        for i in 1u8..=5 {
            let mut relay = make_relay(i);
            for j in 1u8..=5 {
                if i != j { relay.connected_peers.insert(vec![j]); }
            }
            // Relays connected to exits
            for e in 10u8..=12 {
                relay.connected_peers.insert(vec![e]);
            }
            graph.update_relay(relay);
        }

        // 3 exits: ids 10, 11, 12 — added as "relays" in topology (the bug)
        for e in 10u8..=12 {
            let mut exit_relay = make_relay(e);
            // Exits connected to all relays + other exits
            for i in 1u8..=5 { exit_relay.connected_peers.insert(vec![i]); }
            for j in 10u8..=12 {
                if e != j { exit_relay.connected_peers.insert(vec![j]); }
            }
            graph.update_relay(exit_relay);
        }

        // Exit 10 is the destination
        let exit = make_exit(10);
        let gateway_bytes = vec![1u8]; // relay 1 is gateway

        // With 8 relays in topology (5 real + 3 exits), selecting 1 extra hop
        // has a significant chance of picking exit 10 as the relay hop.
        // Run 200 trials to detect the problem statistically.
        let mut exit_selected_as_relay = 0;
        for _ in 0..200 {
            if let Ok(path) = PathSelector::select_path(&graph, 1, &exit, &HashSet::new(), Some(&gateway_bytes)) {
                if path.hops.iter().any(|h| h.peer_id == exit.peer_id) {
                    exit_selected_as_relay += 1;
                }
            }
        }

        // Before the fix: exit_selected_as_relay > 0 (exit can appear as relay hop)
        // After the fix: exit_selected_as_relay == 0 (exit excluded from relay pool)
        assert_eq!(
            exit_selected_as_relay, 0,
            "Exit was selected as a relay hop {exit_selected_as_relay} out of 200 times! \
             This causes shards to arrive at exit with non-empty header, \
             get relayed to self, and silently dropped.",
        );
    }

    /// Verify that excluding exit from relays still allows valid paths.
    #[test]
    fn test_path_selection_works_after_exit_exclusion() {
        let mut graph = TopologyGraph::new();

        // 5 relays fully connected
        for i in 1u8..=5 {
            let mut relay = make_relay(i);
            for j in 1u8..=5 {
                if i != j { relay.connected_peers.insert(vec![j]); }
            }
            relay.connected_peers.insert(vec![10]); // connected to exit
            graph.update_relay(relay);
        }

        // Exit NOT in topology (correct behavior)
        let exit = make_exit(10);
        let gateway = vec![1u8];

        // Single extra hop
        let path = PathSelector::select_path(&graph, 1, &exit, &HashSet::new(), Some(&gateway)).unwrap();
        assert_eq!(path.hops.len(), 1);
        assert_ne!(path.hops[0].peer_id, exit.peer_id);

        // Double extra hop
        let path = PathSelector::select_path(&graph, 2, &exit, &HashSet::new(), Some(&gateway)).unwrap();
        assert_eq!(path.hops.len(), 2);
        for hop in &path.hops {
            assert_ne!(hop.peer_id, exit.peer_id);
        }

        // Diverse paths
        let paths = PathSelector::select_diverse_paths(&graph, 2, &exit, 5, Some(&gateway)).unwrap();
        assert_eq!(paths.len(), 5);
        for p in &paths {
            for hop in &p.hops {
                assert_ne!(hop.peer_id, exit.peer_id, "Exit must never appear as relay hop");
            }
        }
    }
}
