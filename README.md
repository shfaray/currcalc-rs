# currcalc

Currency conversion in the terminal, using the reference rates from
[frankfurter.dev](https://frankfurter.dev). No API key and no configuration
file.

[Русская версия](README_RU.md)

## Install

Requires Rust 1.88 or newer (edition 2024).

```sh
cargo install --path .
```

The binary goes to `~/.cargo/bin`. To build without installing:

```sh
cargo build --release
./target/release/currcalc eur usd 137.42
```

## Usage

```sh
$ currcalc eur usd 137.42
149.40

$ currcalc 42,75 gbp usd
54.06

$ currcalc eur usd
1.09
```

The amount defaults to 1 and can stand before or after the currency pair.
Currency codes are case insensitive. Numbers in the examples are
illustrative; a real call returns the current reference rate.

Plain output is a single number, so it drops into scripts:

```sh
total=$(currcalc chf eur 918.40)
```

Add `--json` when you need the rate, its date and the source:

```sh
$ currcalc --json eur usd 137.42
{"amount":137.42,"date":"2025-02-11","from":"EUR","rate":1.0872,"result":149.40,"source":"frankfurter.dev","to":"USD"}
```

`result` is rounded to `--precision`, `rate` is the unrounded value returned
by the API.

## Amount formats

| Input | Value |
| --- | --- |
| `42` | 42 |
| `137.42` | 137.42 |
| `18,75` | 18.75 |
| `2 480,15` | 2480.15 |
| `1,284.60` | 1284.6 |
| `1.284,60` | 1284.6 |
| `2,480,150` | 2480150 |

Spaces, including non-breaking ones, are ignored. When both separators appear,
the last one is the decimal separator and the other one groups digits. A
separator repeated without the other always groups, and each group must hold
three digits, so `12,34,56` is rejected instead of being read as some other
number. Negative amounts are not accepted.

## Listing currencies

```sh
$ currcalc list
AED   United Arab Emirates Dirham
AFN   Afghan Afghani
...
$ currcalc list euro
EUR   Euro
```

`list` matches any part of the code or the name: `list euro` finds EUR, and
`list dollar` finds every currency with Dollar in its name. The default list
holds the 161 fiat currencies the API covers. Add `--all` to include crypto and
metals such as BTC and XAU; those rows gain a type column. `--json` prints an
array of objects with `code`, `name`, `symbol` and `type`.

## Options

| Option | Description |
| --- | --- |
| `--json` | Print JSON instead of a bare number |
| `--precision <N>` | Fraction digits in the result, 0 to 8, default 2 |
| `--date <YYYY-MM-DD>` | Use the rate published on that date |
| `--no-cache` | Ignore cached data and fetch a fresh rate |
| `--offline` | Never touch the network, use cached data only |

Options may be placed before or after the currencies. `--date` belongs to
conversions; `list` rejects it instead of ignoring it.

Historical lookups work for any date the API covers for that pair. A date
outside the coverage returns an error. For example,
`--precision 4 --date 2024-05-15 eur usd` prints 1.0824.

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
| 1 | The call was valid but failed: unknown currency, no network and no cache, no currencies match the filter |
| 2 | Bad arguments: wrong number of currencies, an invalid code, amount, date or option; help is printed when no arguments are given |

The result goes to stdout and errors go to stderr.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test -- --ignored
```

The last command hits the live API. Everything else runs offline: the CLI
tests seed a temporary cache and pass `--offline`. A few tests set
`CURRCALC_API_BASE` to a closed port to check the fallback to cached data
when the network fails; the same variable points the binary at another API
host.

The agent skill in `skills/currcalc` explains the command surface to coding
agents and installs with `npx skills add shfaray/currcalc-rs`.

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
