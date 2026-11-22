---
name: currcalc
description: Convert currencies, look up a reference rate, or find ISO currency codes with the currcalc CLI. Use when a task needs a converted amount, an exchange rate for a specific date, or the code behind a currency name, and when a script must convert money without hardcoding a rate.
---

# currcalc

currcalc prints a converted amount using the reference rates that
[frankfurter.dev](https://frankfurter.dev) collects from central banks. It is a
single binary with no configuration file and no API key.

## Before the first call

```sh
command -v currcalc
```

If the command is missing and the working directory is this repository, build
it with `cargo build --release` and call `target/release/currcalc`. When
neither exists, tell the user the tool is not installed. Keep the conversion
inside currcalc instead of scraping a rate site or hand-writing a request to
the API, because only the binary carries the cache, timeouts and error mapping
described below.

## Commands

| Task | Command |
| --- | --- |
| Convert | `currcalc <from> <to> [amount]` |
| Rate only | `currcalc <from> <to>` |
| List currency codes | `currcalc list [query]` |
| Include crypto and metals | `currcalc list --all` |
| Conversion with details | `currcalc --json <from> <to> <amount>` |
| Historical rate | `currcalc --date YYYY-MM-DD <from> <to> <amount>` |

The amount defaults to 1 and may stand before or after the pair:
`currcalc 137.42 eur usd` and `currcalc eur usd 137.42` are the same call.
Codes are case insensitive. Fiat codes are the three-letter ISO 4217 form; the
API also knows assets such as `BTC` and `XAU` when `list --all` shows them.

## Output contract

Plain mode writes one number to stdout and nothing else:

```sh
$ currcalc --date 2023-07-14 eur usd 137.42
153.81
```

The value is rounded to `--precision`, which defaults to 2. Raise the
precision before using the number in further arithmetic (`--precision 8`), or
take the unrounded `rate` from `--json` and multiply once in the calling code.

`--json` carries the provenance:

```json
{"amount":137.42,"date":"2023-07-14","from":"EUR","rate":1.1193,"result":153.81,"source":"frankfurter.dev","to":"USD"}
```

Read fields by name; key order is not part of the contract. `result` is
rounded, `rate` is not.

Warnings and errors go to stderr, so `$(currcalc …)` stays clean. Exit code 0
means success, 1 a failed conversion, 2 bad arguments. Check the exit status
instead of parsing stderr.

## Reporting a conversion

- Quote the date and the rate next to a converted amount, taken from
  `--json`. A bare number cannot be checked later.
- Pass `--date` when the result lands in a document, test or report, so the
  number stays reproducible.
- Omit `--date` when the user asks for the current rate.
- Call these mid-market reference rates, updated once or a few times a day.
  Mention that when the user plans to trade, invoice or file taxes on the
  result.

## Finding a code

`list` matches any substring of the code or the name:

```sh
currcalc list franc
currcalc list --json dollar
```

Use the list instead of guessing a code. An unknown code still costs a request
and fails with exit 1 plus a hint pointing back at `list`.

## Cache and network

Each pair is cached on disk: current rates for an hour, historical rates
forever, the currency list for a week.

- `--offline` refuses the network and reads the cache, even past the TTL. Use
  it in tests or when the network is known to be unavailable.
- `--no-cache` ignores the cache and fetches a fresh rate.
- When a request fails and an older entry exists, currcalc prints a warning
  and returns the stale rate. The date in the output tells you how old it is,
  so mention it in the answer.

The machine may have no network at all. Treat a non-zero exit as a failed
conversion and report it, rather than substituting a remembered rate.

## Amounts

Plain numbers work: `42`, `137.42`, `18,75`. Grouping spaces are ignored.
Strip currency symbols, units and parentheses before the call; currcalc takes
digits and separators only. Negative amounts are rejected.

Quote values that contain spaces or commas:

```sh
currcalc --precision 4 eur usd "2 480,15"
```

## Recipes

```sh
# amount for a later step, rounded once at the end
converted=$(currcalc --precision 8 eur usd 918.40)

# rate plus date for a report
currcalc --json --date 2023-07-14 eur usd 137.42

# verified code before a conversion
currcalc list --json swiss
```
