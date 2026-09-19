# Keplr perf numbers

Measured 2026-09-19 on GitHub `ubuntu-latest` via
`keplr bench --files 200 --lines 20 --json` (synth Rust-like corpus, 200 files).

| metric | value |
|---|---|
| files_made | 200 |
| files_walked | 200 |
| gen_ms | 7 |
| walk_ms | 8 |
| index_ms | 7 |
| fuzzy_p50_us | 347 |
| fuzzy_p95_us | 451 |
| grep_p50_us | 83632 |
| grep_p95_us | 84387 |
| cas_put_per_s | 162414 |
| build_rerun | 2 |
| build_skipped | 2 |

Notes:

- `grep_*` rebuilds the trigram index per query today (walk + read + exact scan), so ~84ms on 200 files is dominated by index construction, not matching. A persistent hot index would move this toward the fuzzy numbers.
- `build_skipped: 2/2` shows the content-hash skip working: the second identical run executes nothing.
- `cas_put_per_s` is same-content puts (dedup hit path).

Budgets (spec §3): 100k files / 5M LOC, 500MB soft RAM, <16ms result chunks. Re-run locally with larger `--files` for ceiling numbers (CI keeps the smoke run small).
