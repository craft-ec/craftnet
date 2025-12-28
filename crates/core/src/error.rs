use thiserror::Error;

#[derive(Error, Debug)]
pub enum TunnelCraftError {
    #[error("Destination mismatch: response destination does not match request origin")]
    DestinationMismatch,

    #[error("Invalid chain signature at index {0}")]
    InvalidChainSignature(usize),

    #[error("Chain verification failed: {0}")]
    ChainVerificationFailed(String),

    #[error("Insufficient shards: need {required}, got {available}")]
    InsufficientShards { required: usize, available: usize },

    #[error("Shard reconstruction failed: {0}")]
    ShardReconstructionFailed(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid credit secret")]
    InvalidCreditSecret,

    #[error("Credit expired")]
    CreditExpired,

    #[error("Request not found: {0}")]
    RequestNotFound(String),

    #[error("Request already settled")]
    RequestAlreadySettled,

    #[error("Request not pending")]
    RequestNotPending,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Settlement error: {0}")]
    SettlementError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid public key")]
    InvalidPublicKey,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Timeout")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, TunnelCraftError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_destination_mismatch() {
        let err = TunnelCraftError::DestinationMismatch;
        assert_eq!(
            err.to_string(),
            "Destination mismatch: response destination does not match request origin"
        );
    }

    #[test]
    fn test_error_display_invalid_chain_signature() {
        let err = TunnelCraftError::InvalidChainSignature(5);
        assert_eq!(err.to_string(), "Invalid chain signature at index 5");
    }

    #[test]
    fn test_error_display_chain_verification_failed() {
        let err = TunnelCraftError::ChainVerificationFailed("bad signature".to_string());
        assert_eq!(err.to_string(), "Chain verification failed: bad signature");
    }

    #[test]
    fn test_error_display_insufficient_shards() {
        let err = TunnelCraftError::InsufficientShards {
            required: 3,
            available: 2,
        };
        assert_eq!(err.to_string(), "Insufficient shards: need 3, got 2");
    }

    #[test]
    fn test_error_display_shard_reconstruction_failed() {
        let err = TunnelCraftError::ShardReconstructionFailed("corrupted data".to_string());
        assert_eq!(err.to_string(), "Shard reconstruction failed: corrupted data");
    }

    #[test]
    fn test_error_display_encryption_failed() {
        let err = TunnelCraftError::EncryptionFailed("invalid key".to_string());
        assert_eq!(err.to_string(), "Encryption failed: invalid key");
    }

    #[test]
    fn test_error_display_decryption_failed() {
        let err = TunnelCraftError::DecryptionFailed("corrupted ciphertext".to_string());
        assert_eq!(err.to_string(), "Decryption failed: corrupted ciphertext");
    }

    #[test]
    fn test_error_display_invalid_credit_secret() {
        let err = TunnelCraftError::InvalidCreditSecret;
        assert_eq!(err.to_string(), "Invalid credit secret");
    }

    #[test]
    fn test_error_display_credit_expired() {
        let err = TunnelCraftError::CreditExpired;
        assert_eq!(err.to_string(), "Credit expired");
    }

    #[test]
    fn test_error_display_request_not_found() {
        let err = TunnelCraftError::RequestNotFound("abc123".to_string());
        assert_eq!(err.to_string(), "Request not found: abc123");
    }

    #[test]
    fn test_error_display_request_already_settled() {
        let err = TunnelCraftError::RequestAlreadySettled;
        assert_eq!(err.to_string(), "Request already settled");
    }

    #[test]
    fn test_error_display_request_not_pending() {
        let err = TunnelCraftError::RequestNotPending;
        assert_eq!(err.to_string(), "Request not pending");
    }

    #[test]
    fn test_error_display_network_error() {
        let err = TunnelCraftError::NetworkError("connection refused".to_string());
        assert_eq!(err.to_string(), "Network error: connection refused");
    }

    #[test]
    fn test_error_display_peer_not_found() {
        let err = TunnelCraftError::PeerNotFound("peer123".to_string());
        assert_eq!(err.to_string(), "Peer not found: peer123");
    }

    #[test]
    fn test_error_display_settlement_error() {
        let err = TunnelCraftError::SettlementError("transaction failed".to_string());
        assert_eq!(err.to_string(), "Settlement error: transaction failed");
    }

    #[test]
    fn test_error_display_serialization_error() {
        let err = TunnelCraftError::SerializationError("invalid format".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid format");
    }

    #[test]
    fn test_error_display_invalid_public_key() {
        let err = TunnelCraftError::InvalidPublicKey;
        assert_eq!(err.to_string(), "Invalid public key");
    }

    #[test]
    fn test_error_display_invalid_signature() {
        let err = TunnelCraftError::InvalidSignature;
        assert_eq!(err.to_string(), "Invalid signature");
    }

    #[test]
    fn test_error_display_timeout() {
        let err = TunnelCraftError::Timeout;
        assert_eq!(err.to_string(), "Timeout");
    }

    #[test]
    fn test_error_is_debug() {
        let err = TunnelCraftError::Timeout;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Timeout"));
    }

    #[test]
    fn test_result_type_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_type_err() {
        let result: Result<i32> = Err(TunnelCraftError::Timeout);
        assert!(result.is_err());
    }
}
