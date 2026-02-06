//! JSON-RPC 2.0 protocol types

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize)]
pub struct RpcRequest {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    pub id: u64,
}

impl RpcRequest {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>, id: u64) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
            id,
        }
    }
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
    pub id: serde_json::Value,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Parameters for the `connect` method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    #[serde(default = "default_hops")]
    pub hops: u8,
}

fn default_hops() -> u8 {
    2
}

impl Default for ConnectParams {
    fn default() -> Self {
        Self { hops: default_hops() }
    }
}

/// Result of the `connect` method
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectResult {
    pub connected: bool,
    pub exit_node: Option<String>,
    pub hops: Option<u8>,
}

/// Result of the `status` method
#[derive(Debug, Clone, Deserialize)]
pub struct StatusResult {
    pub state: String,
    pub connected: bool,
    #[serde(default)]
    pub credits: Option<u64>,
    #[serde(default)]
    pub pending_requests: Option<usize>,
    #[serde(default)]
    pub peer_count: Option<usize>,
    #[serde(default)]
    pub shards_relayed: Option<u64>,
    #[serde(default)]
    pub requests_exited: Option<u64>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub privacy_level: Option<String>,
    #[serde(default)]
    pub exit_node: Option<String>,
    #[serde(default)]
    pub hops: Option<u8>,
}

/// Result of the `get_credits` method
#[derive(Debug, Clone, Deserialize)]
pub struct CreditsResult {
    pub credits: u64,
}

/// Result of the `get_node_stats` method
#[derive(Debug, Clone, Deserialize)]
pub struct NodeStatsResult {
    #[serde(default)]
    pub shards_relayed: u64,
    #[serde(default)]
    pub requests_exited: u64,
    #[serde(default)]
    pub peers_connected: usize,
    #[serde(default)]
    pub credits_earned: u64,
    #[serde(default)]
    pub credits_spent: u64,
    #[serde(default)]
    pub bytes_sent: u64,
    #[serde(default)]
    pub bytes_received: u64,
    #[serde(default)]
    pub bytes_relayed: u64,
}

/// Result of the `request` method
#[derive(Debug, Clone, Deserialize)]
pub struct RequestResult {
    pub status: u16,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
}

/// Info about an available exit node
#[derive(Debug, Clone, Deserialize)]
pub struct ExitNodeInfo {
    pub pubkey: String,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub region: String,
    pub score: u8,
    pub load: u8,
    pub latency_ms: Option<u64>,
}

/// Result of the `get_available_exits` method
#[derive(Debug, Clone, Deserialize)]
pub struct AvailableExitsResult {
    pub exits: Vec<ExitNodeInfo>,
}

/// Connection history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHistoryEntry {
    pub id: u64,
    pub connected_at: u64,
    pub disconnected_at: Option<u64>,
    pub duration_secs: Option<u64>,
    pub exit_region: Option<String>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// Result of the `get_connection_history` method
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionHistoryResult {
    pub entries: Vec<ConnectionHistoryEntry>,
}

/// Earnings history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarningsEntry {
    pub id: u64,
    pub timestamp: u64,
    pub entry_type: String,
    pub credits_earned: u64,
    pub shards_count: u64,
}

/// Result of the `get_earnings_history` method
#[derive(Debug, Clone, Deserialize)]
pub struct EarningsHistoryResult {
    pub entries: Vec<EarningsEntry>,
}

/// Speed test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedTestResult {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub latency_ms: u64,
    pub jitter_ms: u64,
    pub timestamp: u64,
}

/// Result of the `run_speed_test` method
#[derive(Debug, Clone, Deserialize)]
pub struct SpeedTestResponse {
    pub result: SpeedTestResult,
}

/// Key export result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyExportResult {
    pub path: String,
    pub public_key: String,
}

/// Key import result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyImportResult {
    pub public_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_request_serialization() {
        let request = RpcRequest::new("connect", Some(serde_json::json!({"hops": 2})), 1);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"connect\""));
    }

    #[test]
    fn test_rpc_request_no_params() {
        let request = RpcRequest::new("status", None, 1);
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("params"));
    }

    #[test]
    fn test_connect_params_default() {
        let params = ConnectParams::default();
        assert_eq!(params.hops, 2);
    }

    #[test]
    fn test_rpc_response_with_result() {
        let json = r#"{"jsonrpc":"2.0","result":{"connected":true},"id":1}"#;
        let response: RpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_rpc_response_with_error() {
        let json = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid request"},"id":1}"#;
        let response: RpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, -32600);
    }
}
