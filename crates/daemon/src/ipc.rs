//! IPC server for JSON-RPC communication

use std::sync::Arc;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::{DaemonError, Result};

/// IPC server configuration
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Socket path (Unix) or pipe name (Windows)
    pub socket_path: PathBuf,
}

impl Default for IpcConfig {
    fn default() -> Self {
        // Default socket path
        let path = if cfg!(target_os = "macos") {
            PathBuf::from("/tmp/craftnet.sock")
        } else if cfg!(target_os = "linux") {
            let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
                .unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(format!("{}/craftnet.sock", xdg_runtime))
        } else {
            // Windows would use named pipes, but for now use a path
            PathBuf::from("\\\\.\\pipe\\craftnet")
        };

        Self { socket_path: path }
    }
}

/// JSON-RPC request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: serde_json::Value,
}

/// JSON-RPC response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

/// JSON-RPC error
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
            id,
        }
    }
}

/// Handler for IPC requests
pub trait IpcHandler: Send + Sync {
    /// Handle a JSON-RPC request
    fn handle(&self, method: &str, params: Option<serde_json::Value>)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>> + Send + '_>>;
}

/// IPC server
pub struct IpcServer {
    config: IpcConfig,
    shutdown_tx: Option<mpsc::Sender<()>>,
    event_tx: Option<broadcast::Sender<String>>,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(config: IpcConfig) -> Self {
        Self {
            config,
            shutdown_tx: None,
            event_tx: None,
        }
    }

    /// Set the event broadcast sender for streaming events to clients
    pub fn set_event_sender(&mut self, tx: broadcast::Sender<String>) {
        self.event_tx = Some(tx);
    }

    /// Start the IPC server
    pub async fn start<H: IpcHandler + 'static>(&mut self, handler: H) -> Result<()> {
        // Remove existing socket file
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| DaemonError::IpcError(format!("Failed to bind: {}", e)))?;

        info!("IPC server listening on {:?}", self.config.socket_path);

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let handler = std::sync::Arc::new(handler);
        let event_tx = self.event_tx.clone();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let handler = handler.clone();
                            let event_rx = event_tx.as_ref().map(|tx| tx.subscribe());
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, handler, event_rx).await {
                                    warn!("Connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("IPC server shutting down");
                    break;
                }
            }
        }

        // Cleanup socket file
        let _ = std::fs::remove_file(&self.config.socket_path);

        Ok(())
    }

    /// Handle a single connection with concurrent request handling and event streaming
    async fn handle_connection<H: IpcHandler + 'static>(
        stream: UnixStream,
        handler: std::sync::Arc<H>,
        event_rx: Option<broadcast::Receiver<String>>,
    ) -> Result<()> {
        let (reader, writer) = stream.into_split();
        let reader = BufReader::new(reader);
        let writer = Arc::new(tokio::sync::Mutex::new(writer));

        let request_writer = writer.clone();
        let request_handler = handler.clone();

        // Task 1: Read JSON-RPC requests and write responses
        let request_task = tokio::spawn(async move {
            let mut reader = reader;
            let mut line = String::new();

            loop {
                line.clear();
                let bytes_read = match reader.read_line(&mut line).await {
                    Ok(n) => n,
                    Err(e) => {
                        debug!("Read error: {}", e);
                        break;
                    }
                };

                if bytes_read == 0 {
                    break;
                }

                debug!("Received: {}", line.trim());

                let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => {
                        if request.jsonrpc != "2.0" {
                            JsonRpcResponse::error(
                                request.id,
                                -32600,
                                "Invalid Request: jsonrpc must be '2.0'".to_string(),
                            )
                        } else {
                            match request_handler.handle(&request.method, request.params).await {
                                Ok(result) => JsonRpcResponse::success(request.id, result),
                                Err(msg) => JsonRpcResponse::error(request.id, -32000, msg),
                            }
                        }
                    }
                    Err(e) => {
                        JsonRpcResponse::error(
                            serde_json::Value::Null,
                            -32700,
                            format!("Parse error: {}", e),
                        )
                    }
                };

                let response_str = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Serialize error: {}", e);
                        break;
                    }
                };

                debug!("Sending: {}", response_str);
                let mut w = request_writer.lock().await;
                if w.write_all(response_str.as_bytes()).await.is_err()
                    || w.write_all(b"\n").await.is_err()
                    || w.flush().await.is_err()
                {
                    break;
                }
            }
        });

        // Task 2: Forward broadcast events to the client
        let event_task = if let Some(mut rx) = event_rx {
            let event_writer = writer.clone();
            Some(tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let mut w = event_writer.lock().await;
                            if w.write_all(event.as_bytes()).await.is_err()
                                || w.write_all(b"\n").await.is_err()
                                || w.flush().await.is_err()
                            {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Event stream lagged, missed {} events", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }))
        } else {
            None
        };

        // Wait for the request task to finish (client disconnected)
        let _ = request_task.await;

        // Cancel the event task
        if let Some(task) = event_task {
            task.abort();
        }

        Ok(())
    }

    /// Stop the IPC server
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.config.socket_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = IpcConfig::default();
        assert!(config.socket_path.to_str().unwrap().contains("craftnet"));
    }

    #[test]
    fn test_json_rpc_response_success() {
        let response = JsonRpcResponse::success(
            serde_json::json!(1),
            serde_json::json!({"status": "connected"}),
        );

        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let response = JsonRpcResponse::error(
            serde_json::json!(1),
            -32600,
            "Invalid Request".to_string(),
        );

        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32600);
    }

    #[test]
    fn test_parse_request() {
        let json = r#"{"jsonrpc":"2.0","method":"status","id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "status");
        assert!(request.params.is_none());
    }

    #[test]
    fn test_parse_request_with_params() {
        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hops":2},"id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.method, "connect");
        assert!(request.params.is_some());

        let params = request.params.unwrap();
        assert_eq!(params["hops"], 2);
    }

    // ==================== NEGATIVE TESTS ====================

    #[test]
    fn test_parse_invalid_json() {
        let json = r#"{not valid json}"#;
        let result: std::result::Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_jsonrpc_field() {
        let json = r#"{"method":"status","id":1}"#;
        let result: std::result::Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_method_field() {
        let json = r#"{"jsonrpc":"2.0","id":1}"#;
        let result: std::result::Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_id_field() {
        let json = r#"{"jsonrpc":"2.0","method":"status"}"#;
        let result: std::result::Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_jsonrpc_version() {
        let json = r#"{"jsonrpc":"1.0","method":"status","id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        // Should parse but version is wrong
        assert_eq!(request.jsonrpc, "1.0");
        assert_ne!(request.jsonrpc, "2.0");
    }

    #[test]
    fn test_empty_method() {
        let json = r#"{"jsonrpc":"2.0","method":"","id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.method, "");
    }

    #[test]
    fn test_null_id() {
        let json = r#"{"jsonrpc":"2.0","method":"status","id":null}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(request.id.is_null());
    }

    #[test]
    fn test_string_id() {
        let json = r#"{"jsonrpc":"2.0","method":"status","id":"abc-123"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.id, "abc-123");
    }

    #[test]
    fn test_response_serialization() {
        let response = JsonRpcResponse::success(
            serde_json::json!(1),
            serde_json::json!({"key": "value"}),
        );

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
        // Note: error field will be present as null, which is fine for JSON-RPC
        assert!(response.error.is_none());
    }

    #[test]
    fn test_error_response_has_no_result() {
        let response = JsonRpcResponse::error(
            serde_json::json!(1),
            -32600,
            "Test error".to_string(),
        );

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32600"));
        assert!(json.contains("Test error"));
    }

    #[test]
    fn test_ipc_server_creation() {
        let config = IpcConfig {
            socket_path: PathBuf::from("/tmp/test.sock"),
        };
        let server = IpcServer::new(config.clone());
        assert_eq!(server.socket_path(), &config.socket_path);
    }

    #[test]
    fn test_ipc_server_with_event_sender() {
        let config = IpcConfig {
            socket_path: PathBuf::from("/tmp/test_events.sock"),
        };
        let mut server = IpcServer::new(config);
        let (tx, _rx) = broadcast::channel::<String>(16);
        server.set_event_sender(tx);
        assert!(server.event_tx.is_some());
    }

    #[test]
    fn test_custom_socket_path() {
        let config = IpcConfig {
            socket_path: PathBuf::from("/custom/path/to/socket.sock"),
        };
        assert_eq!(
            config.socket_path.to_str().unwrap(),
            "/custom/path/to/socket.sock"
        );
    }

    #[test]
    fn test_params_with_nested_object() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":{"outer":{"inner":"value"}},"id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        let params = request.params.unwrap();
        assert_eq!(params["outer"]["inner"], "value");
    }

    #[test]
    fn test_params_with_array() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":[1,2,3],"id":1}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        let params = request.params.unwrap();
        assert!(params.is_array());
        assert_eq!(params.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_large_id_number() {
        let json = r#"{"jsonrpc":"2.0","method":"status","id":9999999999999}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(request.id.is_number());
    }

    #[test]
    fn test_error_code_constants() {
        // Standard JSON-RPC 2.0 error codes
        let parse_error = JsonRpcResponse::error(serde_json::Value::Null, -32700, "Parse error".to_string());
        assert_eq!(parse_error.error.as_ref().unwrap().code, -32700);

        let invalid_request = JsonRpcResponse::error(serde_json::Value::Null, -32600, "Invalid Request".to_string());
        assert_eq!(invalid_request.error.as_ref().unwrap().code, -32600);

        let method_not_found = JsonRpcResponse::error(serde_json::Value::Null, -32601, "Method not found".to_string());
        assert_eq!(method_not_found.error.as_ref().unwrap().code, -32601);
    }
}
