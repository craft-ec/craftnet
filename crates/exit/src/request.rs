//! HTTP request representation for exit node

use std::collections::HashMap;

/// HTTP request reconstructed from shards
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Target URL
    pub url: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Request body (if any)
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    /// Parse an HTTP request from raw bytes
    ///
    /// Format: method\n url\n header_count\n headers...\n body_len\n body
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        let mut lines = data.split(|&b| b == b'\n');

        let method = lines.next()
            .ok_or("missing method")?;
        let method = String::from_utf8_lossy(method).to_string();

        let url = lines.next()
            .ok_or("missing url")?;
        let url = String::from_utf8_lossy(url).to_string();

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

        let body = if body_len > 0 {
            // Collect remaining bytes as body
            let remaining: Vec<u8> = lines
                .flat_map(|line| line.iter().copied().chain(std::iter::once(b'\n')))
                .collect();
            // Remove trailing newline and ensure correct length
            let body_data: Vec<u8> = remaining.into_iter().take(body_len).collect();
            if body_data.len() != body_len {
                return Err("body length mismatch");
            }
            Some(body_data)
        } else {
            None
        };

        Ok(Self {
            method,
            url,
            headers,
            body,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(self.method.as_bytes());
        data.push(b'\n');

        data.extend_from_slice(self.url.as_bytes());
        data.push(b'\n');

        data.extend_from_slice(self.headers.len().to_string().as_bytes());
        data.push(b'\n');

        for (key, value) in &self.headers {
            data.extend_from_slice(format!("{}: {}", key, value).as_bytes());
            data.push(b'\n');
        }

        let body_len = self.body.as_ref().map(|b| b.len()).unwrap_or(0);
        data.extend_from_slice(body_len.to_string().as_bytes());
        data.push(b'\n');

        if let Some(body) = &self.body {
            data.extend_from_slice(body);
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_roundtrip() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("User-Agent".to_string(), "TunnelCraft/0.1".to_string());

        let request = HttpRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/data".to_string(),
            headers,
            body: Some(b"{\"key\": \"value\"}".to_vec()),
        };

        let bytes = request.to_bytes();
        let parsed = HttpRequest::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.url, "https://api.example.com/data");
        assert_eq!(parsed.headers.len(), 2);
        assert_eq!(parsed.body.unwrap(), b"{\"key\": \"value\"}");
    }

    #[test]
    fn test_request_no_body() {
        let request = HttpRequest {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        let bytes = request.to_bytes();
        let parsed = HttpRequest::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.url, "https://example.com");
        assert!(parsed.body.is_none());
    }
}
