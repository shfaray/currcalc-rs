//! End to end tests for the CLI. Every test seeds the cache in a temporary
//! XDG directory and runs with --offline, so no request leaves the machine.

use assert_cmd::Command;
use currcalc::api::{Currency, Rate};
use currcalc::cache::Cache;
use predicates::prelude::*;
use std::path::PathBuf;

struct Fixture {
    home: tempfile::TempDir,
    cache_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let cache_dir = home.path().join("currcalc");
        let cache = Cache::at(&cache_dir);

        cache.store_currencies(&currencies()).unwrap();
        cache
            .store_rate("USD", "RUB", None, &rate("2026-11-22", 82.0))
            .unwrap();
        cache
            .store_rate("USD", "RUB", Some("2024-05-15"), &rate("2024-05-15", 91.38))
            .unwrap();

        Self { home, cache_dir }
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("currcalc").unwrap();
        command.env("XDG_CACHE_HOME", self.home.path());
        command
    }

    fn offline(&self) -> Command {
        let mut command = self.command();
        command.arg("--offline");
        command
    }
}

fn rate(date: &str, value: f64) -> Rate {
    Rate {
        date: date.to_string(),
        base: "USD".to_string(),
        quote: "RUB".to_string(),
        rate: value,
    }
}

fn currencies() -> Vec<Currency> {
    [
        ("USD", "United States Dollar", Some("$"), "currency"),
        ("EUR", "Euro", Some("€"), "currency"),
        ("RUB", "Russian Ruble", Some("₽"), "currency"),
        ("JPY", "Japanese Yen", Some("¥"), "currency"),
        ("GBP", "British Pound", Some("£"), "currency"),
        ("BTC", "Bitcoin", Some("₿"), "crypto"),
        ("XAU", "Gold (Troy Ounce)", None, "metal"),
    ]
    .into_iter()
    .map(|(iso_code, name, symbol, kind)| Currency {
        iso_code: iso_code.to_string(),
        name: name.to_string(),
        symbol: symbol.map(str::to_string),
        kind: kind.to_string(),
    })
    .collect()
}

#[test]
fn lists_only_fiat_by_default() {
    Fixture::new()
        .offline()
        .arg("list")
        .assert()
        .success()
        .stdout(
            "EUR   Euro\nGBP   British Pound\nJPY   Japanese Yen\nRUB   Russian Ruble\nUSD   United States Dollar\n",
        )
        .stdout(predicate::str::contains("BTC").not());
}

#[test]
fn list_filters_by_code_or_name() {
    Fixture::new()
        .offline()
        .args(["list", "ruble"])
        .assert()
        .success()
        .stdout("RUB   Russian Ruble\n");

    Fixture::new()
        .offline()
        .args(["list", "eur"])
        .assert()
        .success()
        .stdout("EUR   Euro\n");
}

#[test]
fn list_all_includes_crypto_and_metals() {
    Fixture::new()
        .offline()
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BTC   crypto    Bitcoin"))
        .stdout(predicate::str::contains(
            "XAU   metal     Gold (Troy Ounce)",
        ));
}

#[test]
fn list_reports_empty_filter() {
    Fixture::new()
        .offline()
        .args(["list", "zzz"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no currencies match 'zzz'"));
}

#[test]
fn list_json_output() {
    let output = Fixture::new()
        .offline()
        .args(["list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let items: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(items.len(), 5);
    let first = &items[0];
    assert_eq!(first["code"], "EUR");
    assert_eq!(first["name"], "Euro");
    assert_eq!(first["type"], "currency");
}

#[test]
fn converts_with_plain_number_output() {
    Fixture::new()
        .offline()
        .args(["usd", "rub", "10"])
        .assert()
        .success()
        .stdout("820.00\n");
}

#[test]
fn amount_can_stand_before_the_pair() {
    Fixture::new()
        .offline()
        .args(["10", "usd", "rub"])
        .assert()
        .success()
        .stdout("820.00\n");
}

#[test]
fn accepts_comma_as_decimal_separator() {
    Fixture::new()
        .offline()
        .args(["51,51", "usd", "rub"])
        .assert()
        .success()
        .stdout("4223.82\n");
}

#[test]
fn precision_flag_controls_fraction_digits() {
    Fixture::new()
        .offline()
        .args(["--precision", "6", "usd", "rub", "10"])
        .assert()
        .success()
        .stdout("820.000000\n");

    Fixture::new()
        .offline()
        .args(["--precision", "0", "usd", "rub", "10"])
        .assert()
        .success()
        .stdout("820\n");
}

#[test]
fn json_output_carries_details() {
    let output = Fixture::new()
        .offline()
        .args(["--json", "usd", "rub", "10"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["from"], "USD");
    assert_eq!(value["to"], "RUB");
    assert_eq!(value["amount"], 10.0);
    assert_eq!(value["result"], 820.0);
    assert_eq!(value["rate"], 82.0);
    assert_eq!(value["date"], "2026-11-22");
    assert_eq!(value["source"], "frankfurter.dev");
}

#[test]
fn historical_date_uses_its_own_cache_entry() {
    Fixture::new()
        .offline()
        .args(["--date", "2024-05-15", "usd", "rub"])
        .assert()
        .success()
        .stdout("91.38\n");
}

#[test]
fn offline_without_cache_fails_clearly() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.cache_dir.join("rate_usd_rub.json")).unwrap();

    fixture
        .offline()
        .args(["usd", "rub", "10"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no cached rate for USD/RUB"));
}

#[test]
fn no_cache_ignores_existing_entries() {
    Fixture::new()
        .offline()
        .args(["--no-cache", "usd", "rub", "10"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no cached rate"));
}

#[test]
fn corrupt_cache_entry_is_ignored() {
    let fixture = Fixture::new();
    std::fs::write(fixture.cache_dir.join("rate_usd_rub.json"), b"{broken").unwrap();

    fixture
        .offline()
        .args(["usd", "rub", "10"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no cached rate"));
}

#[test]
fn stale_entry_is_used_with_a_warning_when_offline() {
    let fixture = Fixture::new();
    let stale = now_secs() - 3 * 60 * 60;
    std::fs::write(
        fixture.cache_dir.join("rate_usd_rub.json"),
        format!(
            r#"{{"fetched_at":{stale},"data":{{"date":"2026-11-22","base":"USD","quote":"RUB","rate":82.0}}}}"#
        ),
    )
    .unwrap();

    fixture
        .offline()
        .args(["usd", "rub", "10"])
        .assert()
        .success()
        .stdout("820.00\n")
        .stderr(predicate::str::contains(
            "using cached rate from 2026-11-22",
        ));
}

#[test]
fn missing_currency_is_a_usage_error() {
    Fixture::new()
        .offline()
        .args(["usd"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("expected two currencies"));
}

#[test]
fn invalid_precision_is_rejected_by_the_parser() {
    Fixture::new()
        .command()
        .args(["--precision", "9", "usd", "rub"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn no_arguments_prints_help() {
    Fixture::new()
        .command()
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("Usage: currcalc"));
}

#[test]
fn help_lists_commands_and_examples() {
    Fixture::new()
        .command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: currcalc"))
        .stdout(predicate::str::contains("currcalc usd rub 10"));
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
