//! HTTP response representation for exit node

use std::collections::HashMap;

/// HTTP response to be fragmented into shards
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Create a new HTTP response
    pub fn new(status: u16, headers: HashMap<String, String>, body: Vec<u8>) -> Self {
        Self { status, headers, body }
    }

    /// Parse an HTTP response from raw bytes
    ///
    /// Format: status\n header_count\n headers...\n body_len\n body
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        let mut lines = data.split(|&b| b == b'\n');

        let status = lines.next()
            .ok_or("missing status")?;
        let status: u16 = String::from_utf8_lossy(status)
            .parse()
            .map_err(|_| "invalid status code")?;

        let header_count = lines.next()
            .ok_or("missing header count")?;
        let header_count: usize = String::from_utf8_lossy(header_count)
            .parse()
            .map_err(|_| "invalid header count")?;

        let mut headers = HashMap::new();
        for _ in 0..header_count {
            let header_line = lines.next()
                .ok_or("missing header")?;
            let header_str = String::from_utf8_lossy(header_line);
            if let Some((key, value)) = header_str.split_once(':') {
                headers.insert(
                    key.trim().to_string(),
                    value.trim().to_string(),
                );
            }
        }

        let body_len = lines.next()
            .ok_or("missing body length")?;
        let body_len: usize = String::from_utf8_lossy(body_len)
            .parse()
            .map_err(|_| "invalid body length")?;

        // Collect remaining bytes as body
        let remaining: Vec<u8> = lines
            .flat_map(|line| line.iter().copied().chain(std::iter::once(b'\n')))
            .collect();
        let body: Vec<u8> = remaining.into_iter().take(body_len).collect();

        Ok(Self { status, headers, body })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(self.status.to_string().as_bytes());
        data.push(b'\n');

        data.extend_from_slice(self.headers.len().to_string().as_bytes());
        data.push(b'\n');

        for (key, value) in &self.headers {
            data.extend_from_slice(format!("{}: {}", key, value).as_bytes());
            data.push(b'\n');
        }

        data.extend_from_slice(self.body.len().to_string().as_bytes());
        data.push(b'\n');
        data.extend_from_slice(&self.body);

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_roundtrip() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let response = HttpResponse {
            status: 200,
            headers,
            body: b"{\"success\": true}".to_vec(),
        };

        let bytes = response.to_bytes();
        let parsed = HttpResponse::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.headers.get("Content-Type").unwrap(), "application/json");
        assert_eq!(parsed.body, b"{\"success\": true}");
    }

    #[test]
    fn test_response_empty_body() {
        let response = HttpResponse {
            status: 204,
            headers: HashMap::new(),
            body: Vec::new(),
        };

        let bytes = response.to_bytes();
        let parsed = HttpResponse::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.status, 204);
        assert!(parsed.body.is_empty());
    }
}
