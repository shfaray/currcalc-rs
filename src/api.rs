//! Thin client for the public Frankfurter API (<https://frankfurter.dev>).

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const API_BASE: &str = "https://api.frankfurter.dev/v2";
pub const SOURCE: &str = "frankfurter.dev";

/// A single exchange rate quoted by the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub date: String,
    pub base: String,
    pub quote: String,
    pub rate: f64,
}

/// A currency, metal or crypto asset known to the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Currency {
    pub iso_code: String,
    pub name: String,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

impl Currency {
    pub fn is_fiat(&self) -> bool {
        self.kind == "currency"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    UnknownCurrency(String),
    NotFound(String),
    Network(String),
    BadResponse(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::UnknownCurrency(code) => write!(f, "unknown currency: {code}"),
            ApiError::NotFound(message) => write!(f, "rate not found: {message}"),
            ApiError::Network(message) => write!(f, "network error: {message}"),
            ApiError::BadResponse(message) => write!(f, "unexpected response: {message}"),
        }
    }
}

impl std::error::Error for ApiError {}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
}

/// Blocking API client with short timeouts suitable for a CLI.
pub struct Client {
    agent: ureq::Agent,
    base: String,
}

impl Client {
    pub fn new() -> Self {
        Self::with_base(API_BASE)
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .timeout_connect(Some(Duration::from_secs(2)))
            .http_status_as_error(false)
            .build();

        Self {
            agent: config.into(),
            base: base.into(),
        }
    }

    pub fn rate(&self, from: &str, to: &str, date: Option<&str>) -> Result<Rate, ApiError> {
        let mut url = format!("{}/rate/{from}/{to}", self.base);
        if let Some(date) = date {
            url.push_str("?date=");
            url.push_str(date);
        }
        parse_rate(&self.get(&url)?)
    }

    pub fn currencies(&self) -> Result<Vec<Currency>, ApiError> {
        parse_currencies(&self.get(&format!("{}/currencies", self.base))?)
    }

    fn get(&self, url: &str) -> Result<String, ApiError> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|err| ApiError::Network(err.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|err| ApiError::Network(err.to_string()))?;

        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(error_from_response(status, &body))
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_rate(body: &str) -> Result<Rate, ApiError> {
    serde_json::from_str(body)
        .map_err(|err| ApiError::BadResponse(format!("cannot parse rate: {err}")))
}

pub fn parse_currencies(body: &str) -> Result<Vec<Currency>, ApiError> {
    serde_json::from_str(body)
        .map_err(|err| ApiError::BadResponse(format!("cannot parse currency list: {err}")))
}

/// Cheap local check so a typo fails before a network round trip.
pub fn validate_date(date: &str) -> Result<(), String> {
    let bytes = date.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 4 || idx == 7 || byte.is_ascii_digit());

    if !shape_ok {
        return Err(format!("invalid date: {date} (expected YYYY-MM-DD)"));
    }

    let month: u8 = date[5..7].parse().unwrap_or(0);
    let day: u8 = date[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(format!("invalid date: {date}"));
    }
    Ok(())
}

fn error_from_response(status: u16, body: &str) -> ApiError {
    let message = serde_json::from_str::<ErrorBody>(body)
        .map(|parsed| parsed.message)
        .unwrap_or_else(|_| body.trim().to_string());

    if status == 422
        && let Some(code) = message.strip_prefix("invalid currency: ")
    {
        return ApiError::UnknownCurrency(code.trim().to_ascii_uppercase());
    }

    match status {
        404 => ApiError::NotFound(message),
        _ => ApiError::BadResponse(format!("HTTP {status}: {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE_FIXTURE: &str = include_str!("../tests/fixtures/rate_usd_rub.json");
    const UNKNOWN_FIXTURE: &str = include_str!("../tests/fixtures/rate_unknown.json");
    const CURRENCIES_FIXTURE: &str = include_str!("../tests/fixtures/currencies.json");

    #[test]
    fn parses_rate_response() {
        let rate = parse_rate(RATE_FIXTURE).unwrap();
        assert_eq!(rate.base, "USD");
        assert_eq!(rate.quote, "RUB");
        assert_eq!(rate.date, "2026-11-22");
        assert!((rate.rate - 82.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_currency_list() {
        let currencies = parse_currencies(CURRENCIES_FIXTURE).unwrap();
        let usd = currencies.iter().find(|c| c.iso_code == "USD").unwrap();
        assert!(usd.is_fiat());
        assert_eq!(usd.name, "United States Dollar");
        assert_eq!(usd.symbol.as_deref(), Some("$"));

        let btc = currencies.iter().find(|c| c.iso_code == "BTC").unwrap();
        assert!(!btc.is_fiat());
    }

    #[test]
    fn maps_invalid_currency_to_domain_error() {
        let err = error_from_response(422, UNKNOWN_FIXTURE);
        assert_eq!(err, ApiError::UnknownCurrency("XYZ".to_string()));
        assert_eq!(err.to_string(), "unknown currency: XYZ");
    }

    #[test]
    fn maps_missing_rate_to_not_found() {
        let err = error_from_response(404, r#"{"status":404,"message":"not found"}"#);
        assert_eq!(err, ApiError::NotFound("not found".to_string()));
    }

    #[test]
    fn rejects_malformed_bodies() {
        assert!(matches!(
            parse_rate("not json"),
            Err(ApiError::BadResponse(_))
        ));
    }

    #[test]
    fn validates_date_shape() {
        assert!(validate_date("2024-05-15").is_ok());
        for bad in ["2024-5-15", "15.05.2024", "2024-13-01", "2024-05-32", ""] {
            assert!(validate_date(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    #[ignore = "hits the live frankfurter.dev API"]
    fn talks_to_the_real_api() {
        let client = Client::new();
        let rate = client.rate("USD", "RUB", None).unwrap();
        assert_eq!(rate.base, "USD");
        assert_eq!(rate.quote, "RUB");
        assert!(rate.rate > 0.0);

        let currencies = client.currencies().unwrap();
        assert!(currencies.len() > 100);

        let historical = client.rate("USD", "RUB", Some("2024-05-15")).unwrap();
        assert_eq!(historical.date, "2024-05-15");
    }
}
