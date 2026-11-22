# currcalc

Currency conversion in the terminal, using the reference rates from
[frankfurter.dev](https://frankfurter.dev). No API key and no configuration
file.

[Русская версия](README_RU.md)

## Install

Requires Rust 1.85 or newer (edition 2024).

```sh
cargo install --path .
```

The binary goes to `~/.cargo/bin`. To build without installing:

```sh
cargo build --release
./target/release/currcalc usd rub 10
```

## Usage

```sh
$ currcalc usd rub 10
820.00

$ currcalc 10 usd rub
820.00

$ currcalc usd rub
82.00
```

The amount defaults to 1 and can stand before or after the currency pair.
Currency codes are case insensitive.

Plain output is a single number, so it drops into scripts:

```sh
total=$(currcalc eur rub 500)
```

Add `--json` when you need the rate, its date and the source:

```sh
$ currcalc --json usd rub 10
{"amount":10.0,"date":"2026-11-22","from":"USD","rate":82.0,"result":820.0,"source":"frankfurter.dev","to":"RUB"}
```

`result` is rounded to `--precision`, `rate` is the unrounded value returned
by the API. Rates in these examples are from 22 November 2026.

## Amount formats

| Input | Value |
| --- | --- |
| `10` | 10 |
| `321.32` | 321.32 |
| `51,51` | 51.51 |
| `1 234,56` | 1234.56 |
| `1,234.56` | 1234.56 |
| `1.234,56` | 1234.56 |
| `1,234,567` | 1234567 |

Spaces, including non-breaking ones, are ignored. When both separators appear,
the last one is the decimal separator and the other one groups digits. A
separator repeated without the other always groups, and each group must hold
three digits, so `1,2,3` is rejected instead of being read as some other
number. Negative amounts are not accepted.

## Listing currencies

```sh
$ currcalc list
AED   United Arab Emirates Dirham
AFN   Afghan Afghani
...
$ currcalc list rub
AWG   Aruban Florin
BYN   Belarusian Ruble
RUB   Russian Ruble
```

`list` filters by any substring of the code or the name, which is why `rub`
also matches Aruban Florin. The default list holds the 161 fiat currencies the
API covers.
Add `--all` to include crypto and metals such as BTC and XAU; those rows gain a
type column. `--json` prints an array of objects with `code`, `name`, `symbol`
and `type`.

## Options

| Option | Description |
| --- | --- |
| `--json` | Print JSON instead of a bare number |
| `--precision <N>` | Fraction digits in the result, 0 to 8, default 2 |
| `--date <YYYY-MM-DD>` | Use the rate published on that date |
| `--no-cache` | Ignore cached data and fetch a fresh rate |
| `--offline` | Never touch the network, use cached data only |

Options may be placed before or after the currencies.

Historical lookups work for any date the API covers for that pair. A date
outside the coverage returns an error. For the example pair the API answers
`--date 2024-05-15` with 91.38.

## Cache

Rates are stored in `$XDG_CACHE_HOME/currcalc` (`~/.cache/currcalc` when the
variable is unset), one small JSON file per pair and date. Current rates stay
valid for an hour, historical rates forever, and the currency list for a week.

When a request fails and an older entry exists, currcalc prints a warning to
stderr and uses that entry. `--offline` skips the request entirely: it prints
the cached rate even if the entry is older than the TTL, and fails when nothing
is cached. `--no-cache` ignores existing entries, so combining it with
`--offline` leaves nothing to read.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Conversion error, such as an unknown currency or no network and no cache |
| 2 | Bad arguments, or no arguments at all (help is printed) |

The result goes to stdout and errors go to stderr.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test -- --ignored
```

The last command hits the live API. Everything else runs offline: the CLI
tests seed a temporary cache and pass `--offline`.

## Data source

Frankfurter collects reference rates published by central banks and other
official sources. The values are mid-market rates updated once or a few times
a day, not live trading quotes. Daily history goes back to 1948 for some
currencies.

If you use the output in a report, a line such as "Exchange rates: Frankfurter
(frankfurter.dev)" is appreciated, though not required.

## Notes

The code was written with AI assistance and reviewed by a human.

## License

MIT, see [LICENSE](LICENSE).
