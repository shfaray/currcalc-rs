//! Command line interface: argument parsing, cache policy and output.

use crate::api::{self, ApiError, Client, Currency, Rate, SOURCE};
use crate::cache::{CURRENCIES_TTL, Cache, Cached, RATE_TTL};
use crate::money;
use clap::{CommandFactory, Parser, Subcommand};
use std::process::ExitCode;
use std::time::Duration;

const EXAMPLES: &str = "\
Examples:
  currcalc list
  currcalc list franc
  currcalc eur usd 137.42
  currcalc 42,75 gbp usd
  currcalc --precision 4 --json eur usd 137.42";

#[derive(Parser)]
#[command(
    name = "currcalc",
    version,
    about = "Convert currencies at the reference rate from frankfurter.dev",
    after_help = EXAMPLES
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Currencies and an optional amount: <from> <to> [amount]
    #[arg(value_name = "ARGS", num_args = 0..)]
    args: Vec<String>,

    /// Print machine readable JSON instead of a bare number
    #[arg(long, global = true)]
    json: bool,

    /// Number of fraction digits in the result
    #[arg(
        long,
        global = true,
        value_name = "N",
        default_value_t = 2,
        value_parser = clap::value_parser!(u8).range(0..=8)
    )]
    precision: u8,

    /// Use the rate published on this date (YYYY-MM-DD)
    #[arg(long, global = true, value_name = "YYYY-MM-DD")]
    date: Option<String>,

    /// Ignore cached data and fetch fresh rates
    #[arg(long, global = true)]
    no_cache: bool,

    /// Never touch the network, use cached data only
    #[arg(long, global = true)]
    offline: bool,
}

#[derive(Subcommand)]
enum Command {
    /// List available currencies
    List {
        /// Filter by currency code or name
        query: Option<String>,

        /// Include crypto and metals, not only fiat currencies
        #[arg(long)]
        all: bool,
    },
}

#[derive(Debug, PartialEq)]
struct Conversion {
    from: String,
    to: String,
    amount: f64,
}

#[derive(Debug)]
enum AppError {
    /// The invocation itself is wrong; nothing was looked up or fetched.
    Usage(String),
    /// The invocation is fine but it cannot be completed.
    Runtime(String),
    Api(ApiError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Usage(message) | AppError::Runtime(message) => write!(f, "{message}"),
            AppError::Api(err) => write!(f, "{err}"),
        }
    }
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match &cli.command {
        Some(Command::List { query, all }) => list(&cli, query.as_deref(), *all),
        None if cli.args.is_empty() => {
            let mut command = Cli::command();
            let _ = command.print_help();
            println!();
            return ExitCode::from(2);
        }
        None => convert(&cli),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            if let AppError::Api(ApiError::UnknownCurrency(code)) = &err {
                eprintln!("hint: run `currcalc list {code}` to find the right code");
            }
            let code = match err {
                AppError::Usage(_) => 2,
                AppError::Runtime(_) | AppError::Api(_) => 1,
            };
            ExitCode::from(code)
        }
    }
}

fn convert(cli: &Cli) -> Result<(), AppError> {
    let conversion = parse_conversion(&cli.args).map_err(AppError::Usage)?;
    if let Some(date) = cli.date.as_deref() {
        api::validate_date(date).map_err(AppError::Usage)?;
    }

    let cache = open_cache();
    let from = conversion.from.as_str();
    let to = conversion.to.as_str();
    let date = cli.date.as_deref();

    let cached = if cli.no_cache {
        None
    } else {
        cache.as_ref().and_then(|cache| cache.rate(from, to, date))
    };
    let ttl = if date.is_some() {
        Duration::MAX
    } else {
        RATE_TTL
    };

    if let Some(entry) = &cached
        && entry.age <= ttl
    {
        return print_conversion(cli, conversion.amount, &entry.data);
    }

    if cli.offline {
        return match cached {
            Some(entry) => {
                warn_stale("offline mode", &entry);
                print_conversion(cli, conversion.amount, &entry.data)
            }
            None => Err(AppError::Runtime(format!(
                "no cached rate for {from}/{to}; {}",
                offline_cache_hint(cli.no_cache)
            ))),
        };
    }

    match Client::new().rate(from, to, date) {
        Ok(rate) => {
            if let Some(cache) = &cache
                && let Err(err) = cache.store_rate(from, to, date, &rate)
            {
                eprintln!("warning: {err}");
            }
            print_conversion(cli, conversion.amount, &rate)
        }
        Err(err) => match cached {
            Some(entry) => {
                warn_stale(&format!("{err}"), &entry);
                print_conversion(cli, conversion.amount, &entry.data)
            }
            None => Err(AppError::Api(err)),
        },
    }
}

fn print_conversion(cli: &Cli, amount: f64, rate: &Rate) -> Result<(), AppError> {
    let result = amount * rate.rate;
    if !result.is_finite() {
        return Err(AppError::Usage("amount is too large".to_string()));
    }
    let precision = cli.precision as usize;

    if cli.json {
        let payload = serde_json::json!({
            "from": rate.base,
            "to": rate.quote,
            "amount": amount,
            "result": money::round_amount(result, precision),
            "rate": rate.rate,
            "date": rate.date,
            "source": SOURCE,
        });
        println!("{payload}");
    } else {
        println!("{}", money::format_amount(result, precision));
    }
    Ok(())
}

fn list(cli: &Cli, query: Option<&str>, all: bool) -> Result<(), AppError> {
    if cli.date.is_some() {
        return Err(AppError::Usage(
            "`--date` applies to conversions, not to `list`".to_string(),
        ));
    }

    let cache = open_cache();
    let cached = if cli.no_cache {
        None
    } else {
        cache.as_ref().and_then(Cache::currencies)
    };

    let currencies = if let Some(entry) = &cached
        && entry.age <= CURRENCIES_TTL
    {
        entry.data.clone()
    } else if cli.offline {
        match cached {
            Some(entry) => {
                eprintln!(
                    "warning: offline mode; using currency list cached {}",
                    human_age(entry.age)
                );
                entry.data
            }
            None => {
                return Err(AppError::Runtime(format!(
                    "no cached currency list; {}",
                    offline_cache_hint(cli.no_cache)
                )));
            }
        }
    } else {
        match Client::new().currencies() {
            Ok(currencies) => {
                if let Some(cache) = &cache
                    && let Err(err) = cache.store_currencies(&currencies)
                {
                    eprintln!("warning: {err}");
                }
                currencies
            }
            Err(err) => match cached {
                Some(entry) => {
                    eprintln!(
                        "warning: {err}; using currency list cached {}",
                        human_age(entry.age)
                    );
                    entry.data
                }
                None => return Err(AppError::Api(err)),
            },
        }
    };

    let query = query.map(str::to_lowercase);
    let mut filtered: Vec<&Currency> = currencies
        .iter()
        .filter(|currency| all || currency.is_fiat())
        .filter(|currency| match &query {
            None => true,
            Some(query) => {
                currency.iso_code.to_lowercase().contains(query)
                    || currency.name.to_lowercase().contains(query)
            }
        })
        .collect();
    filtered.sort_by(|a, b| a.iso_code.cmp(&b.iso_code));

    if filtered.is_empty() {
        let message = match query {
            Some(query) => format!("no currencies match '{query}'"),
            None => "no currencies available".to_string(),
        };
        return Err(AppError::Runtime(message));
    }

    if cli.json {
        let items: Vec<_> = filtered
            .iter()
            .map(|currency| {
                serde_json::json!({
                    "code": currency.iso_code,
                    "name": currency.name,
                    "symbol": currency.symbol,
                    "type": currency.kind,
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(items));
    } else {
        for currency in filtered {
            if all {
                println!(
                    "{:<4}  {:<8}  {}",
                    currency.iso_code, currency.kind, currency.name
                );
            } else {
                println!("{:<4}  {}", currency.iso_code, currency.name);
            }
        }
    }
    Ok(())
}

fn parse_conversion(args: &[String]) -> Result<Conversion, String> {
    let mut codes = Vec::new();
    let mut amount = None;

    for arg in args {
        if is_numeric(arg) {
            if amount.is_some() {
                return Err("more than one amount given".to_string());
            }
            amount = Some(money::parse_amount(arg)?);
        } else {
            codes.push(normalize_code(arg)?);
        }
    }

    match codes.len() {
        2 => Ok(Conversion {
            from: codes.remove(0),
            to: codes.remove(0),
            amount: amount.unwrap_or(1.0),
        }),
        0 | 1 => Err("expected two currencies, e.g. `currcalc eur usd 137.42`".to_string()),
        _ => Err("too many currencies given, expected exactly two".to_string()),
    }
}

fn is_numeric(arg: &str) -> bool {
    arg.chars().any(|ch| ch.is_ascii_digit()) && !arg.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn normalize_code(arg: &str) -> Result<String, String> {
    let code = arg.trim();
    if (3..=5).contains(&code.len()) && code.chars().all(|ch| ch.is_ascii_alphabetic()) {
        Ok(code.to_ascii_uppercase())
    } else {
        Err(format!("invalid currency code: {arg}"))
    }
}

fn open_cache() -> Option<Cache> {
    match Cache::open() {
        Ok(cache) => Some(cache),
        Err(err) => {
            eprintln!("warning: {err}; continuing without cache");
            None
        }
    }
}

/// Telling the user to drop `--offline` is unhelpful when `--no-cache` is
/// the flag that emptied the cache in the first place.
fn offline_cache_hint(no_cache: bool) -> &'static str {
    if no_cache {
        "--no-cache prevents reading cached data; drop one of the two flags"
    } else {
        "run once without --offline"
    }
}

fn warn_stale(reason: &str, entry: &Cached<Rate>) {
    eprintln!(
        "warning: {reason}; using cached rate from {} (cached {})",
        entry.data.date,
        human_age(entry.age)
    );
}

fn human_age(age: Duration) -> String {
    let minutes = age.as_secs() / 60;
    match minutes {
        0 => "less than a minute ago".to_string(),
        1..=59 => format!("{minutes} min ago"),
        _ => {
            let hours = minutes / 60;
            let minutes = minutes % 60;
            if hours < 24 {
                return if minutes == 0 {
                    format!("{hours} h ago")
                } else {
                    format!("{hours} h {minutes} min ago")
                };
            }

            let days = hours / 24;
            let hours = hours % 24;
            if hours == 0 {
                format!("{days} d ago")
            } else {
                format!("{days} d {hours} h ago")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn amount_can_come_last() {
        assert_eq!(
            parse_conversion(&args(&["eur", "usd", "137.42"])).unwrap(),
            Conversion {
                from: "EUR".to_string(),
                to: "USD".to_string(),
                amount: 137.42
            }
        );
    }

    #[test]
    fn amount_can_come_first() {
        assert_eq!(
            parse_conversion(&args(&["42,75", "gbp", "usd"])).unwrap(),
            Conversion {
                from: "GBP".to_string(),
                to: "USD".to_string(),
                amount: 42.75
            }
        );
    }

    #[test]
    fn amount_defaults_to_one() {
        let conversion = parse_conversion(&args(&["eur", "usd"])).unwrap();
        assert_eq!(conversion.amount, 1.0);
    }

    #[test]
    fn rejects_bad_argument_sets() {
        for bad in [
            vec![],
            vec!["eur"],
            vec!["eur", "usd", "137.42", "42.75"],
            vec!["eur", "usd", "1.2.3"],
            vec!["eur", "u"],
            vec!["eur", "usd", "ein"],
        ] {
            assert!(
                parse_conversion(&args(&bad)).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn accepts_three_to_five_letter_codes() {
        assert_eq!(normalize_code(" eur ").unwrap(), "EUR");
        assert_eq!(normalize_code("eUr").unwrap(), "EUR");
        assert_eq!(normalize_code("USDT").unwrap(), "USDT");

        for bad in ["", "eu", "eurooo", "e1r", "еур", "eur-usd"] {
            assert!(normalize_code(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn tells_amounts_from_codes() {
        for amount in ["1", "137.42", "1,5", "1e5", "+5"] {
            assert!(is_numeric(amount), "{amount:?} should be an amount");
        }
        for code in ["eur", "XAU", ""] {
            assert!(!is_numeric(code), "{code:?} should be a code");
        }
    }

    #[test]
    fn formats_age() {
        assert_eq!(human_age(Duration::from_secs(10)), "less than a minute ago");
        assert_eq!(human_age(Duration::from_secs(90)), "1 min ago");
        assert_eq!(
            human_age(Duration::from_secs(3 * 3600 + 120)),
            "3 h 2 min ago"
        );
        assert_eq!(human_age(Duration::from_secs(24 * 3600)), "1 d ago");
        assert_eq!(
            human_age(Duration::from_secs(3 * 24 * 3600 + 5 * 3600)),
            "3 d 5 h ago"
        );
    }
}
