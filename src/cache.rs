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
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Rates are reference values published a few times a day.
pub const RATE_TTL: Duration = Duration::from_secs(60 * 60);
/// The list of currencies changes rarely; a week is plenty.
pub const CURRENCIES_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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
        Some(Cached {
            data: entry.data,
            age: Duration::from_secs(now_secs().saturating_sub(entry.fetched_at)),
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
        fs::write(&tmp, bytes)?;
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
            date: "2026-11-22".to_string(),
            base: "USD".to_string(),
            quote: "RUB".to_string(),
            rate: 82.0,
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
            .store_rate("USD", "RUB", None, &sample_rate())
            .unwrap();
        let cached = cache.rate("USD", "RUB", None).unwrap();

        assert_eq!(cached.data, sample_rate());
        assert!(cached.age < Duration::from_secs(5));
    }

    #[test]
    fn keeps_historical_and_latest_rates_apart() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());

        cache
            .store_rate("USD", "RUB", None, &sample_rate())
            .unwrap();
        let mut historical = sample_rate();
        historical.date = "2024-05-15".to_string();
        historical.rate = 91.38;
        cache
            .store_rate("USD", "RUB", Some("2024-05-15"), &historical)
            .unwrap();

        assert_eq!(cache.rate("USD", "RUB", None).unwrap().data.rate, 82.0);
        assert_eq!(
            cache
                .rate("USD", "RUB", Some("2024-05-15"))
                .unwrap()
                .data
                .rate,
            91.38
        );
        assert!(cache.rate("USD", "RUB", Some("2024-05-16")).is_none());
    }

    #[test]
    fn reports_entry_age() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let stale = now_secs().saturating_sub(3 * 60 * 60);
        fs::write(
            dir.path().join("rate_usd_rub.json"),
            format!(
                r#"{{"fetched_at":{stale},"data":{{"date":"2026-11-22","base":"USD","quote":"RUB","rate":82.0}}}}"#
            ),
        )
        .unwrap();

        let cached = cache.rate("USD", "RUB", None).unwrap();
        assert!(cached.age >= Duration::from_secs(3 * 60 * 60));
    }

    #[test]
    fn ignores_corrupt_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        fs::write(dir.path().join("rate_usd_rub.json"), b"{not json").unwrap();

        assert!(cache.rate("USD", "RUB", None).is_none());
    }

    #[test]
    fn missing_directory_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("does-not-exist"));

        assert!(cache.rate("USD", "RUB", None).is_none());
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
    fn writes_stay_inside_cache_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());

        cache
            .store_rate("../../etc", "passwd", None, &sample_rate())
            .unwrap();

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("rate_etc_passwd.json")]
        );
    }
}
