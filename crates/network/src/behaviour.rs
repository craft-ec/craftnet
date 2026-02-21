//! Network behaviour for CraftNet
//!
//! Re-exports CraftBehaviour from craftec-network as CraftNetBehaviour.
//! CraftNet-specific constants (gossipsub topics, DHT key prefixes) are defined here.
//! Gossipsub/DHT helper methods are provided via the CraftNetExt trait.

use libp2p::{
    gossipsub, kad,
    PeerId, StreamProtocol,
};
use std::time::Duration;

// Re-export the generic behaviour as CraftNet's behaviour
pub use craftec_network::CraftBehaviour as CraftNetBehaviour;
pub use craftec_network::behaviour::CraftBehaviourEvent as CraftNetBehaviourEvent;

/// Kademlia protocol name (used when CraftNet runs standalone — not in shared swarm)
pub const KADEMLIA_PROTOCOL: StreamProtocol = StreamProtocol::new("/craftnet/kad/1.0.0");

/// Rendezvous namespace for CraftNet nodes (bootstrap only)
pub const RENDEZVOUS_NAMESPACE: &str = "craftnet";

/// DHT key prefix for exit node records
pub const EXIT_DHT_KEY_PREFIX: &str = "/craftnet/exits/";

/// DHT key prefix for peer pubkey → PeerId records
/// Used by clients to announce themselves so relays can route response shards
pub const PEER_DHT_KEY_PREFIX: &str = "/craftnet/peers/";

/// TTL for peer records (5 minutes, same as exit records)
pub const PEER_RECORD_TTL: Duration = Duration::from_secs(300);

/// Generate DHT key for an exit node's info record
pub fn exit_dht_key(peer_id: &PeerId) -> Vec<u8> {
    format!("{}{}", EXIT_DHT_KEY_PREFIX, peer_id).into_bytes()
}

/// Generate DHT key for a peer's pubkey → PeerId record
pub fn peer_dht_key(pubkey: &[u8; 32]) -> Vec<u8> {
    format!("{}{}", PEER_DHT_KEY_PREFIX, hex::encode(pubkey)).into_bytes()
}

/// Well-known DHT key for the exit node registry
/// Nodes query this to get the list of known exit peer IDs
pub const EXIT_REGISTRY_KEY: &[u8] = b"/craftnet/exit-registry";

/// TTL for exit records (5 minutes)
/// Exits re-announce every 2 minutes, so 5 min gives 2.5x safety margin
/// Shorter TTL optimized for mobile churn - faster dead exit detection
pub const EXIT_RECORD_TTL: Duration = Duration::from_secs(300);

/// Gossipsub topic for exit node status (heartbeat, load, online/offline)
pub const EXIT_STATUS_TOPIC: &str = "craftnet/exit-status/1.0.0";

/// Gossipsub topic for ZK-proven receipt summaries
pub const PROOF_TOPIC: &str = "craftnet/proofs/1.0.0";

/// Gossipsub topic for subscription announcements
pub const SUBSCRIPTION_TOPIC: &str = "craftnet/subscriptions/1.0.0";

/// Gossipsub topic for aggregator history sync (new aggregators catching up)
pub const AGGREGATOR_SYNC_TOPIC: &str = "craftnet/aggregator-sync/1.0.0";

/// Heartbeat interval for exit nodes (30 seconds)
pub const EXIT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Consider exit offline if no heartbeat for this duration (90 seconds = 3 missed heartbeats)
pub const EXIT_OFFLINE_THRESHOLD: Duration = Duration::from_secs(90);

// ============================================================================
// Relay DHT discovery constants (mirrors exit pattern)
// ============================================================================

/// DHT key prefix for relay node records
pub const RELAY_DHT_KEY_PREFIX: &str = "/craftnet/relays/";

/// Well-known DHT key for the relay node registry
pub const RELAY_REGISTRY_KEY: &[u8] = b"/craftnet/relay-registry";

/// TTL for relay records (5 minutes)
pub const RELAY_RECORD_TTL: Duration = Duration::from_secs(300);

/// Gossipsub topic for relay node status (heartbeat, load, online/offline)
pub const RELAY_STATUS_TOPIC: &str = "craftnet/relay-status/1.0.0";

/// Heartbeat interval for relay nodes (30 seconds)
pub const RELAY_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Consider relay offline if no heartbeat for this duration (90 seconds)
pub const RELAY_OFFLINE_THRESHOLD: Duration = Duration::from_secs(90);

/// Generate DHT key for a relay node's info record
pub fn relay_dht_key(peer_id: &PeerId) -> Vec<u8> {
    format!("{}{}", RELAY_DHT_KEY_PREFIX, peer_id).into_bytes()
}

// ============================================================================
// Extension trait for CraftNet-specific gossipsub + DHT operations
// ============================================================================

/// Extension trait adding CraftNet-specific gossipsub and DHT operations
/// to the generic CraftBehaviour (re-exported as CraftNetBehaviour).
pub trait CraftNetExt {
    // Gossipsub subscriptions
    fn subscribe_exit_status(&mut self) -> Result<bool, gossipsub::SubscriptionError>;
    fn unsubscribe_exit_status(&mut self) -> bool;
    fn publish_exit_status(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError>;
    fn subscribe_proofs(&mut self) -> Result<bool, gossipsub::SubscriptionError>;
    fn unsubscribe_proofs(&mut self) -> bool;
    fn publish_proof(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError>;
    fn subscribe_subscriptions(&mut self) -> Result<bool, gossipsub::SubscriptionError>;
    fn unsubscribe_subscriptions(&mut self) -> bool;
    fn publish_subscription(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError>;
    fn subscribe_aggregator_sync(&mut self) -> Result<bool, gossipsub::SubscriptionError>;
    fn unsubscribe_aggregator_sync(&mut self) -> bool;
    fn publish_aggregator_sync(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError>;
    fn subscribe_relay_status(&mut self) -> Result<bool, gossipsub::SubscriptionError>;
    fn unsubscribe_relay_status(&mut self) -> bool;
    fn publish_relay_status(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError>;

    // DHT: exit records
    fn put_exit_record(&mut self, peer_id: &PeerId, record_value: Vec<u8>) -> Result<kad::QueryId, kad::store::Error>;
    fn start_providing_exit(&mut self) -> Result<kad::QueryId, kad::store::Error>;
    fn stop_providing_exit(&mut self);
    fn get_exit_record(&mut self, peer_id: &PeerId) -> kad::QueryId;
    fn get_exit_providers(&mut self) -> kad::QueryId;

    // DHT: relay records
    fn put_relay_record(&mut self, peer_id: &PeerId, record_value: Vec<u8>) -> Result<kad::QueryId, kad::store::Error>;
    fn start_providing_relay(&mut self) -> Result<kad::QueryId, kad::store::Error>;
    fn stop_providing_relay(&mut self);
    fn get_relay_record(&mut self, peer_id: &PeerId) -> kad::QueryId;
    fn get_relay_providers(&mut self) -> kad::QueryId;

    // DHT: peer records
    fn put_peer_record(&mut self, pubkey: &[u8; 32], peer_id: &PeerId) -> Result<kad::QueryId, kad::store::Error>;
    fn get_peer_record(&mut self, pubkey: &[u8; 32]) -> kad::QueryId;
}

impl CraftNetExt for CraftNetBehaviour {
    // === Gossipsub ===
    fn subscribe_exit_status(&mut self) -> Result<bool, gossipsub::SubscriptionError> {
        self.subscribe_topic(EXIT_STATUS_TOPIC)
    }
    fn unsubscribe_exit_status(&mut self) -> bool {
        self.unsubscribe_topic(EXIT_STATUS_TOPIC)
    }
    fn publish_exit_status(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        self.publish_to_topic(EXIT_STATUS_TOPIC, data)
    }
    fn subscribe_proofs(&mut self) -> Result<bool, gossipsub::SubscriptionError> {
        self.subscribe_topic(PROOF_TOPIC)
    }
    fn unsubscribe_proofs(&mut self) -> bool {
        self.unsubscribe_topic(PROOF_TOPIC)
    }
    fn publish_proof(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        self.publish_to_topic(PROOF_TOPIC, data)
    }
    fn subscribe_subscriptions(&mut self) -> Result<bool, gossipsub::SubscriptionError> {
        self.subscribe_topic(SUBSCRIPTION_TOPIC)
    }
    fn unsubscribe_subscriptions(&mut self) -> bool {
        self.unsubscribe_topic(SUBSCRIPTION_TOPIC)
    }
    fn publish_subscription(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        self.publish_to_topic(SUBSCRIPTION_TOPIC, data)
    }
    fn subscribe_aggregator_sync(&mut self) -> Result<bool, gossipsub::SubscriptionError> {
        self.subscribe_topic(AGGREGATOR_SYNC_TOPIC)
    }
    fn unsubscribe_aggregator_sync(&mut self) -> bool {
        self.unsubscribe_topic(AGGREGATOR_SYNC_TOPIC)
    }
    fn publish_aggregator_sync(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        self.publish_to_topic(AGGREGATOR_SYNC_TOPIC, data)
    }
    fn subscribe_relay_status(&mut self) -> Result<bool, gossipsub::SubscriptionError> {
        self.subscribe_topic(RELAY_STATUS_TOPIC)
    }
    fn unsubscribe_relay_status(&mut self) -> bool {
        self.unsubscribe_topic(RELAY_STATUS_TOPIC)
    }
    fn publish_relay_status(&mut self, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        self.publish_to_topic(RELAY_STATUS_TOPIC, data)
    }

    // === DHT: exit ===
    fn put_exit_record(&mut self, peer_id: &PeerId, record_value: Vec<u8>) -> Result<kad::QueryId, kad::store::Error> {
        let key = kad::RecordKey::new(&exit_dht_key(peer_id));
        let expires = std::time::Instant::now() + EXIT_RECORD_TTL;
        let record = kad::Record {
            key,
            value: record_value,
            publisher: Some(*peer_id),
            expires: Some(expires),
        };
        self.kademlia.put_record(record, kad::Quorum::One)
    }
    fn start_providing_exit(&mut self) -> Result<kad::QueryId, kad::store::Error> {
        let key = kad::RecordKey::new(&EXIT_REGISTRY_KEY);
        self.kademlia.start_providing(key)
    }
    fn stop_providing_exit(&mut self) {
        let key = kad::RecordKey::new(&EXIT_REGISTRY_KEY);
        self.kademlia.stop_providing(&key);
    }
    fn get_exit_record(&mut self, peer_id: &PeerId) -> kad::QueryId {
        let key = kad::RecordKey::new(&exit_dht_key(peer_id));
        self.kademlia.get_record(key)
    }
    fn get_exit_providers(&mut self) -> kad::QueryId {
        let key = kad::RecordKey::new(&EXIT_REGISTRY_KEY);
        self.kademlia.get_providers(key)
    }

    // === DHT: relay ===
    fn put_relay_record(&mut self, peer_id: &PeerId, record_value: Vec<u8>) -> Result<kad::QueryId, kad::store::Error> {
        let key = kad::RecordKey::new(&relay_dht_key(peer_id));
        let expires = std::time::Instant::now() + RELAY_RECORD_TTL;
        let record = kad::Record {
            key,
            value: record_value,
            publisher: Some(*peer_id),
            expires: Some(expires),
        };
        self.kademlia.put_record(record, kad::Quorum::One)
    }
    fn start_providing_relay(&mut self) -> Result<kad::QueryId, kad::store::Error> {
        let key = kad::RecordKey::new(&RELAY_REGISTRY_KEY);
        self.kademlia.start_providing(key)
    }
    fn stop_providing_relay(&mut self) {
        let key = kad::RecordKey::new(&RELAY_REGISTRY_KEY);
        self.kademlia.stop_providing(&key);
    }
    fn get_relay_record(&mut self, peer_id: &PeerId) -> kad::QueryId {
        let key = kad::RecordKey::new(&relay_dht_key(peer_id));
        self.kademlia.get_record(key)
    }
    fn get_relay_providers(&mut self) -> kad::QueryId {
        let key = kad::RecordKey::new(&RELAY_REGISTRY_KEY);
        self.kademlia.get_providers(key)
    }

    // === DHT: peer records ===
    fn put_peer_record(&mut self, pubkey: &[u8; 32], peer_id: &PeerId) -> Result<kad::QueryId, kad::store::Error> {
        let key = kad::RecordKey::new(&peer_dht_key(pubkey));
        let expires = std::time::Instant::now() + PEER_RECORD_TTL;
        let record = kad::Record {
            key,
            value: peer_id.to_bytes(),
            publisher: Some(*peer_id),
            expires: Some(expires),
        };
        self.kademlia.put_record(record, kad::Quorum::One)
    }
    fn get_peer_record(&mut self, pubkey: &[u8; 32]) -> kad::QueryId {
        let key = kad::RecordKey::new(&peer_dht_key(pubkey));
        self.kademlia.get_record(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kademlia_protocol() {
        assert_eq!(KADEMLIA_PROTOCOL.as_ref(), "/craftnet/kad/1.0.0");
    }

    #[test]
    fn test_rendezvous_namespace() {
        assert_eq!(RENDEZVOUS_NAMESPACE, "craftnet");
    }
}
