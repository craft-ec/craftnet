//! Geo-location detection for nodes
//!
//! Provides auto-detection of node location for announcement to the network.

use serde::{Deserialize, Serialize};
use crate::types::ExitRegion;

/// Detected location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Detected region
    pub region: ExitRegion,
    /// Country code (ISO 3166-1 alpha-2)
    pub country_code: String,
    /// Country name
    pub country_name: String,
    /// City name (if available)
    pub city: Option<String>,
    /// Internet Service Provider
    pub isp: Option<String>,
    /// Organization/AS name
    pub org: Option<String>,
    /// Autonomous System number
    pub as_number: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
}

impl GeoLocation {
    /// Create a new GeoLocation
    pub fn new(
        region: ExitRegion,
        country_code: String,
        country_name: String,
        city: Option<String>,
    ) -> Self {
        Self {
            region,
            country_code,
            country_name,
            city,
            isp: None,
            org: None,
            as_number: None,
            latitude: None,
            longitude: None,
        }
    }

    /// Create unknown location (fallback)
    pub fn unknown() -> Self {
        Self {
            region: ExitRegion::Auto,
            country_code: "XX".to_string(),
            country_name: "Unknown".to_string(),
            city: None,
            isp: None,
            org: None,
            as_number: None,
            latitude: None,
            longitude: None,
        }
    }
}

/// Map country code to region
pub fn country_to_region(country_code: &str) -> ExitRegion {
    match country_code.to_uppercase().as_str() {
        // North America
        "US" | "CA" | "MX" => ExitRegion::NorthAmerica,

        // Europe
        "GB" | "DE" | "FR" | "IT" | "ES" | "NL" | "BE" | "AT" | "CH" | "SE" |
        "NO" | "DK" | "FI" | "PL" | "CZ" | "PT" | "IE" | "GR" | "HU" | "RO" |
        "BG" | "HR" | "SK" | "SI" | "LT" | "LV" | "EE" | "LU" | "MT" | "CY" => ExitRegion::Europe,

        // Asia Pacific
        "JP" | "KR" | "CN" | "HK" | "TW" | "SG" | "MY" | "TH" | "VN" | "PH" |
        "ID" | "IN" | "BD" | "PK" | "LK" => ExitRegion::AsiaPacific,

        // Oceania
        "AU" | "NZ" | "FJ" | "PG" => ExitRegion::Oceania,

        // South America
        "BR" | "AR" | "CL" | "CO" | "PE" | "VE" | "EC" | "UY" | "PY" | "BO" => ExitRegion::SouthAmerica,

        // Middle East
        "AE" | "SA" | "IL" | "TR" | "QA" | "KW" | "BH" | "OM" | "JO" | "LB" => ExitRegion::MiddleEast,

        // Africa
        "ZA" | "EG" | "NG" | "KE" | "MA" | "GH" | "TN" | "TZ" | "ET" => ExitRegion::Africa,

        _ => ExitRegion::Auto,
    }
}

/// Response from ip-api.com (free tier)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpApiResponse {
    pub status: String,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub region_name: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    #[serde(rename = "as")]
    pub as_info: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

impl IpApiResponse {
    /// Convert API response to GeoLocation
    pub fn to_geo_location(&self) -> GeoLocation {
        let country_code = self.country_code.clone().unwrap_or_else(|| "XX".to_string());
        let region = country_to_region(&country_code);

        GeoLocation {
            region,
            country_code,
            country_name: self.country.clone().unwrap_or_else(|| "Unknown".to_string()),
            city: self.city.clone(),
            isp: self.isp.clone(),
            org: self.org.clone(),
            as_number: self.as_info.clone(),
            latitude: self.lat,
            longitude: self.lon,
        }
    }
}

/// Geo-location detector
pub struct GeoDetector {
    /// Cached location
    cached_location: Option<GeoLocation>,
}

impl GeoDetector {
    /// Create a new geo detector
    pub fn new() -> Self {
        Self {
            cached_location: None,
        }
    }

    /// Get cached location (if available)
    pub fn cached(&self) -> Option<&GeoLocation> {
        self.cached_location.as_ref()
    }

    /// Set cached location (from external detection)
    pub fn set_cached(&mut self, location: GeoLocation) {
        self.cached_location = Some(location);
    }

    /// Parse location from IP-API response JSON
    pub fn parse_ip_api_response(&mut self, json: &str) -> Option<GeoLocation> {
        match serde_json::from_str::<IpApiResponse>(json) {
            Ok(response) if response.status == "success" => {
                let location = response.to_geo_location();
                self.cached_location = Some(location.clone());
                Some(location)
            }
            _ => None,
        }
    }
}

impl Default for GeoDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_country_to_region_north_america() {
        assert_eq!(country_to_region("US"), ExitRegion::NorthAmerica);
        assert_eq!(country_to_region("CA"), ExitRegion::NorthAmerica);
        assert_eq!(country_to_region("MX"), ExitRegion::NorthAmerica);
    }

    #[test]
    fn test_country_to_region_europe() {
        assert_eq!(country_to_region("GB"), ExitRegion::Europe);
        assert_eq!(country_to_region("DE"), ExitRegion::Europe);
        assert_eq!(country_to_region("FR"), ExitRegion::Europe);
    }

    #[test]
    fn test_country_to_region_asia() {
        assert_eq!(country_to_region("JP"), ExitRegion::AsiaPacific);
        assert_eq!(country_to_region("SG"), ExitRegion::AsiaPacific);
        assert_eq!(country_to_region("KR"), ExitRegion::AsiaPacific);
    }

    #[test]
    fn test_country_to_region_oceania() {
        assert_eq!(country_to_region("AU"), ExitRegion::Oceania);
        assert_eq!(country_to_region("NZ"), ExitRegion::Oceania);
    }

    #[test]
    fn test_country_to_region_unknown() {
        assert_eq!(country_to_region("XX"), ExitRegion::Auto);
        assert_eq!(country_to_region("ZZ"), ExitRegion::Auto);
    }

    #[test]
    fn test_country_to_region_case_insensitive() {
        assert_eq!(country_to_region("us"), ExitRegion::NorthAmerica);
        assert_eq!(country_to_region("Us"), ExitRegion::NorthAmerica);
    }

    #[test]
    fn test_geo_location_new() {
        let loc = GeoLocation::new(
            ExitRegion::Europe,
            "DE".to_string(),
            "Germany".to_string(),
            Some("Berlin".to_string()),
        );
        assert_eq!(loc.region, ExitRegion::Europe);
        assert_eq!(loc.country_code, "DE");
        assert_eq!(loc.city, Some("Berlin".to_string()));
    }

    #[test]
    fn test_geo_location_unknown() {
        let loc = GeoLocation::unknown();
        assert_eq!(loc.region, ExitRegion::Auto);
        assert_eq!(loc.country_code, "XX");
    }

    #[test]
    fn test_geo_detector_cache() {
        let mut detector = GeoDetector::new();
        assert!(detector.cached().is_none());

        let loc = GeoLocation::new(
            ExitRegion::NorthAmerica,
            "US".to_string(),
            "United States".to_string(),
            Some("New York".to_string()),
        );
        detector.set_cached(loc);

        assert!(detector.cached().is_some());
        assert_eq!(detector.cached().unwrap().country_code, "US");
    }

    #[test]
    fn test_parse_ip_api_response() {
        let mut detector = GeoDetector::new();
        let json = r#"{
            "status": "success",
            "country": "Germany",
            "countryCode": "DE",
            "regionName": "Hesse",
            "city": "Frankfurt am Main",
            "lat": 50.1109,
            "lon": 8.6821
        }"#;

        let loc = detector.parse_ip_api_response(json).unwrap();
        assert_eq!(loc.region, ExitRegion::Europe);
        assert_eq!(loc.country_code, "DE");
        assert_eq!(loc.city, Some("Frankfurt am Main".to_string()));
    }

    #[test]
    fn test_parse_ip_api_response_failure() {
        let mut detector = GeoDetector::new();
        let json = r#"{"status": "fail", "message": "reserved range"}"#;

        let loc = detector.parse_ip_api_response(json);
        assert!(loc.is_none());
    }
}
