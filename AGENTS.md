# AGENTS.md

Project instructions for coding agents working on currcalc.

## What this is

A small Rust CLI that converts currency amounts using the reference rates
published through frankfurter.dev. Plain stdout is one number; `--json` adds
the rate, its date and the source.

## Commands

```sh
cargo build --release
cargo test
cargo test -- --ignored   # hits the live API, run sparingly
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Run `cargo fmt` before committing.

## Layout

- `src/cli.rs`: argument parsing, cache policy, output.
- `src/api.rs`: frankfurter client and response types.
- `src/cache.rs`: per-pair disk cache with TTL.
- `src/money.rs`: amount parsing and formatting.
- `src/main.rs`: binary entry point, SIGPIPE handling.
- `tests/cli.rs`: end-to-end tests; `tests/fixtures/` holds recorded responses.
- `skills/currcalc/SKILL.md`: agent skill, installable with
  `npx skills add shfaray/currcalc-rs`. Update it when the CLI surface changes.

## Rules

- Keep `cargo test` offline. Tests seed `XDG_CACHE_HOME` in a temporary
  directory and pass `--offline`; only `#[ignore]` tests may reach the network.
- Keep `README.md` and `README_RU.md` in sync. Both are user-facing.
- Examples in the READMEs and in `--help` use illustrative amounts and rates,
  not live values.
- Commit in English using Conventional Commits, one logical change per commit.
- Never run `git push` (a branch or a tag) without permission for that exact
  push. Commit locally, then stop and ask. Permission for an earlier push, for
  the session, or for "the changes" does not carry over: every push needs its
  own explicit yes, and only the refs named in that yes may be pushed.
- Do not rewrite pushed history, force-push, or change git config unless the
  user asks for it.
