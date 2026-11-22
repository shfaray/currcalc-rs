//! Disk cache for exchange rates and the currency list.
//!
//! Entries live next to each other in one directory, keyed by request so
//! the latest and a historical rate never overwrite each other. Writes go
//! through a temporary file and a rename, which keeps a half written entry
//! from being read after an interrupted process.

use crate::api::{Currency, Rate};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Rates are reference values published a few times a day.
pub const RATE_TTL: Duration = Duration::from_secs(60 * 60);
/// The list of currencies changes rarely; a week is plenty.
pub const CURRENCIES_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Tolerance for a system clock that steps a little between write and read.
const CLOCK_SKEW: u64 = 60;

#[derive(Debug, Serialize, Deserialize)]
struct Entry<T> {
    fetched_at: u64,
    data: T,
}

/// A cached value plus how long ago it was stored.
#[derive(Debug, Clone)]
pub struct Cached<T> {
    pub data: T,
    pub age: Duration,
}

#[derive(Debug)]
pub enum CacheError {
    NoCacheDir,
    Io(std::io::Error),
    Serde(serde_json::Error),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::NoCacheDir => write!(f, "cannot determine a cache directory"),
            CacheError::Io(err) => write!(f, "cache I/O error: {err}"),
            CacheError::Serde(err) => write!(f, "cache serialization error: {err}"),
        }
    }
}

impl std::error::Error for CacheError {}

impl From<std::io::Error> for CacheError {
    fn from(err: std::io::Error) -> Self {
        CacheError::Io(err)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(err: serde_json::Error) -> Self {
        CacheError::Serde(err)
    }
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// Open the cache under `$XDG_CACHE_HOME/currcalc` (usually `~/.cache`).
    pub fn open() -> Result<Self, CacheError> {
        let dir = dirs::cache_dir()
            .ok_or(CacheError::NoCacheDir)?
            .join("currcalc");
        Ok(Self { dir })
    }

    /// Open a cache in an explicit directory. Used by tests.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn rate(&self, from: &str, to: &str, date: Option<&str>) -> Option<Cached<Rate>> {
        self.read(&rate_file(from, to, date))
    }

    pub fn store_rate(
        &self,
        from: &str,
        to: &str,
        date: Option<&str>,
        rate: &Rate,
    ) -> Result<(), CacheError> {
        self.write(&rate_file(from, to, date), rate)
    }

    pub fn currencies(&self) -> Option<Cached<Vec<Currency>>> {
        self.read("currencies.json")
    }

    pub fn store_currencies(&self, currencies: &[Currency]) -> Result<(), CacheError> {
        self.write("currencies.json", currencies)
    }

    fn read<T: DeserializeOwned>(&self, name: &str) -> Option<Cached<T>> {
        let bytes = fs::read(self.dir.join(name)).ok()?;
        let entry: Entry<T> = serde_json::from_slice(&bytes).ok()?;
        let now = now_secs();
        if entry.fetched_at > now.saturating_add(CLOCK_SKEW) {
            // The stamp is in the future, so either the clock moved backwards
            // or the file was touched. Treat it as a miss instead of letting
            // it stay fresh until the clock catches up.
            return None;
        }
        Some(Cached {
            data: entry.data,
            age: Duration::from_secs(now.saturating_sub(entry.fetched_at)),
        })
    }

    fn write(&self, name: &str, data: impl Serialize) -> Result<(), CacheError> {
        fs::create_dir_all(&self.dir)?;

        let entry = Entry {
            fetched_at: now_secs(),
            data,
        };
        let bytes = serde_json::to_vec(&entry)?;

        let target = self.dir.join(name);
        let tmp = self.dir.join(format!("{name}.{}.tmp", std::process::id()));

        // A run killed mid-write can leave this name behind, and the pid may
        // have been reused since. Drop the leftover and create the file
        // exclusively so the write cannot land on a symlink.
        let _ = fs::remove_file(&tmp);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        if let Err(err) = file.write_all(&bytes) {
            drop(file);
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        drop(file);

        if let Err(err) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        Ok(())
    }
}

fn rate_file(from: &str, to: &str, date: Option<&str>) -> String {
    let from = sanitize(from);
    let to = sanitize(to);
    match date {
        Some(date) => format!("rate_{}_{from}_{to}.json", sanitize(date)),
        None => format!("rate_{from}_{to}.json"),
    }
}

/// Keep user input from turning into a path outside the cache directory.
///
/// Callers only pass validated currency codes and dates, so two different
/// requests cannot collapse into one file name.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rate() -> Rate {
        Rate {
            date: "2023-07-14".to_string(),
            base: "EUR".to_string(),
            quote: "USD".to_string(),
            rate: 1.1193,
        }
    }

    fn sample_currencies() -> Vec<Currency> {
        vec![
            Currency {
                iso_code: "USD".to_string(),
                name: "United States Dollar".to_string(),
                symbol: Some("$".to_string()),
                kind: "currency".to_string(),
            },
            Currency {
                iso_code: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                symbol: None,
                kind: "crypto".to_string(),
            },
        ]
    }

    #[test]
    fn stores_and_reads_latest_rate() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());

        cache
            .store_rate("EUR", "USD", None, &sample_rate())
            .unwrap();
        let cached = cache.rate("EUR", "USD", None).unwrap();

        assert_eq!(cached.data, sample_rate());
        assert!(cached.age < Duration::from_secs(5));
    }

    #[test]
    fn keeps_historical_and_latest_rates_apart() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());

        cache
            .store_rate("EUR", "USD", None, &sample_rate())
            .unwrap();
        let mut historical = sample_rate();
        historical.date = "2022-09-28".to_string();
        historical.rate = 0.96024;
        cache
            .store_rate("EUR", "USD", Some("2022-09-28"), &historical)
            .unwrap();

        assert_eq!(cache.rate("EUR", "USD", None).unwrap().data.rate, 1.1193);
        assert_eq!(
            cache
                .rate("EUR", "USD", Some("2022-09-28"))
                .unwrap()
                .data
                .rate,
            0.96024
        );
        assert!(cache.rate("EUR", "USD", Some("2022-09-29")).is_none());
    }

    #[test]
    fn reports_entry_age() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let stale = now_secs().saturating_sub(3 * 60 * 60);
        fs::write(
            dir.path().join("rate_eur_usd.json"),
            format!(
                r#"{{"fetched_at":{stale},"data":{{"date":"2023-07-14","base":"EUR","quote":"USD","rate":1.1193}}}}"#
            ),
        )
        .unwrap();

        let cached = cache.rate("EUR", "USD", None).unwrap();
        assert!(cached.age >= Duration::from_secs(3 * 60 * 60));
    }

    #[test]
    fn ignores_entries_stamped_in_the_future() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let future = now_secs() + 24 * 60 * 60;
        fs::write(
            dir.path().join("rate_eur_usd.json"),
            format!(
                r#"{{"fetched_at":{future},"data":{{"date":"2023-07-14","base":"EUR","quote":"USD","rate":1.1193}}}}"#
            ),
        )
        .unwrap();

        assert!(cache.rate("EUR", "USD", None).is_none());
    }

    #[test]
    fn tolerates_a_small_clock_skew() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let slightly_ahead = now_secs() + CLOCK_SKEW / 2;
        fs::write(
            dir.path().join("rate_eur_usd.json"),
            format!(
                r#"{{"fetched_at":{slightly_ahead},"data":{{"date":"2023-07-14","base":"EUR","quote":"USD","rate":1.1193}}}}"#
            ),
        )
        .unwrap();

        let cached = cache.rate("EUR", "USD", None).unwrap();
        assert_eq!(cached.age, Duration::ZERO);
    }

    #[test]
    fn ignores_corrupt_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        fs::write(dir.path().join("rate_eur_usd.json"), b"{not json").unwrap();

        assert!(cache.rate("EUR", "USD", None).is_none());
    }

    #[test]
    fn missing_directory_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("does-not-exist"));

        assert!(cache.rate("EUR", "USD", None).is_none());
        assert!(cache.currencies().is_none());
    }

    #[test]
    fn stores_and_reads_currency_list() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());

        cache.store_currencies(&sample_currencies()).unwrap();
        let cached = cache.currencies().unwrap();

        assert_eq!(cached.data, sample_currencies());
        assert_eq!(cached.data[1].symbol, None);
    }

    #[test]
    fn replaces_a_leftover_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let leftover = dir
            .path()
            .join(format!("rate_eur_usd.json.{}.tmp", std::process::id()));
        fs::write(&leftover, b"stale half written entry").unwrap();

        cache
            .store_rate("EUR", "USD", None, &sample_rate())
            .unwrap();

        assert!(!leftover.exists());
        assert_eq!(cache.rate("EUR", "USD", None).unwrap().data, sample_rate());
    }

    #[test]
    fn sanitize_strips_path_characters() {
        assert_eq!(sanitize("eur/chf"), "eurchf");
        assert_eq!(sanitize("eur\\chf"), "eurchf");
        assert_eq!(sanitize("eur chf"), "eurchf");
        assert_eq!(sanitize("usd.v2"), "usdv2");
        assert_eq!(sanitize("EUR-CHF"), "eur-chf");
    }

    #[test]
    fn rate_file_names_are_flat() {
        assert_eq!(rate_file("EUR", "USD", None), "rate_eur_usd.json");
        assert_eq!(
            rate_file("EUR", "USD", Some("2023-07-14")),
            "rate_2023-07-14_eur_usd.json"
        );
    }
}
