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
            .store_rate("EUR", "USD", None, &rate("2023-07-14", 1.1193))
            .unwrap();
        cache
            .store_rate(
                "EUR",
                "USD",
                Some("2022-09-28"),
                &rate("2022-09-28", 0.96024),
            )
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
        base: "EUR".to_string(),
        quote: "USD".to_string(),
        rate: value,
    }
}

fn currencies() -> Vec<Currency> {
    [
        ("USD", "United States Dollar", Some("$"), "currency"),
        ("EUR", "Euro", Some("€"), "currency"),
        ("CHF", "Swiss Franc", Some("Fr"), "currency"),
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
            "CHF   Swiss Franc\nEUR   Euro\nGBP   British Pound\nJPY   Japanese Yen\nUSD   United States Dollar\n",
        )
        .stdout(predicate::str::contains("BTC").not());
}

#[test]
fn list_filters_by_code_or_name() {
    Fixture::new()
        .offline()
        .args(["list", "franc"])
        .assert()
        .success()
        .stdout("CHF   Swiss Franc\n");

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
    assert_eq!(first["code"], "CHF");
    assert_eq!(first["name"], "Swiss Franc");
    assert_eq!(first["type"], "currency");
}

#[test]
fn converts_with_plain_number_output() {
    Fixture::new()
        .offline()
        .args(["eur", "usd", "137.42"])
        .assert()
        .success()
        .stdout("153.81\n");
}

#[test]
fn amount_can_stand_before_the_pair() {
    Fixture::new()
        .offline()
        .args(["42.75", "eur", "usd"])
        .assert()
        .success()
        .stdout("47.85\n");
}

#[test]
fn accepts_comma_as_decimal_separator() {
    Fixture::new()
        .offline()
        .args(["18,75", "eur", "usd"])
        .assert()
        .success()
        .stdout("20.99\n");
}

#[test]
fn precision_flag_controls_fraction_digits() {
    Fixture::new()
        .offline()
        .args(["--precision", "6", "eur", "usd", "137.42"])
        .assert()
        .success()
        .stdout("153.814206\n");

    Fixture::new()
        .offline()
        .args(["--precision", "0", "eur", "usd", "137.42"])
        .assert()
        .success()
        .stdout("154\n");
}

#[test]
fn json_output_carries_details() {
    let output = Fixture::new()
        .offline()
        .args(["--json", "eur", "usd", "137.42"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["from"], "EUR");
    assert_eq!(value["to"], "USD");
    assert_eq!(value["amount"], 137.42);
    assert_eq!(value["result"], 153.81);
    assert_eq!(value["rate"], 1.1193);
    assert_eq!(value["date"], "2023-07-14");
    assert_eq!(value["source"], "frankfurter.dev");
}

#[test]
fn historical_date_uses_its_own_cache_entry() {
    Fixture::new()
        .offline()
        .args(["--precision", "5", "--date", "2022-09-28", "eur", "usd"])
        .assert()
        .success()
        .stdout("0.96024\n");
}

#[test]
fn offline_without_cache_fails_clearly() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.cache_dir.join("rate_eur_usd.json")).unwrap();

    fixture
        .offline()
        .args(["eur", "usd", "137.42"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no cached rate for EUR/USD"));
}

#[test]
fn no_cache_ignores_existing_entries() {
    Fixture::new()
        .offline()
        .args(["--no-cache", "eur", "usd", "137.42"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("no cached rate"));
}

#[test]
fn corrupt_cache_entry_is_ignored() {
    let fixture = Fixture::new();
    std::fs::write(fixture.cache_dir.join("rate_eur_usd.json"), b"{broken").unwrap();

    fixture
        .offline()
        .args(["eur", "usd", "137.42"])
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
        fixture.cache_dir.join("rate_eur_usd.json"),
        format!(
            r#"{{"fetched_at":{stale},"data":{{"date":"2023-07-14","base":"EUR","quote":"USD","rate":1.1193}}}}"#
        ),
    )
    .unwrap();

    fixture
        .offline()
        .args(["eur", "usd", "137.42"])
        .assert()
        .success()
        .stdout("153.81\n")
        .stderr(predicate::str::contains(
            "using cached rate from 2023-07-14",
        ));
}

#[test]
fn missing_currency_is_a_usage_error() {
    Fixture::new()
        .offline()
        .args(["eur"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("expected two currencies"));
}

#[test]
fn invalid_precision_is_rejected_by_the_parser() {
    Fixture::new()
        .command()
        .args(["--precision", "9", "eur", "usd"])
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
        .stdout(predicate::str::contains("currcalc eur usd 137.42"));
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
