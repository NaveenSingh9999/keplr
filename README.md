# Keplr

Personal lightweight Rust IDE — canvas foundation, Zed-exact dark UX,
headless `serve` + desktop native from one codebase, Git LFS native,
LAML first-class.

Spec: `docs/superpowers/specs/2026-09-17-keplr-design.md`
Plan foundation: `docs/superpowers/plans/2026-09-18-keplr-production-foundation.md`
Plan B canvas: `docs/superpowers/plans/2026-09-18-keplr-plan-b-canvas-foundation.md`
Plan C builds: `docs/superpowers/plans/2026-09-18-keplr-build-dag.md`
Plan D index/lang/sync: `docs/superpowers/plans/2026-09-18-keplr-plan-d-index-lang-sync.md`

## Use (foundation CLI, production)

```bash
cargo run -p keplr-cli -- --root ~/LAML files "serve" --limit 20
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 20
cargo run -p keplr-cli -- --root . run check
cargo run -p keplr-cli -- --root . serve --port 7137
curl '127.0.0.1:7137/search?needle=hello&limit=5'
```

## Use (Plan B canvas foundation, production)

```bash
cargo run -p keplr-cli -- --root . ui --open Cargo.toml --width 100
cargo run -p keplr-cli -- --root . scene --open Cargo.toml | head -n 40
cargo run -p keplr-cli -- --root . ui --palette "main" --width 100
cargo run -p keplr-cli -- --root . serve --port 7137 &
curl '127.0.0.1:7137/scene?open=Cargo.toml&width=100'
curl '127.0.0.1:7137/files?query=keplr&limit=5'
curl '127.0.0.1:7137/tasks'
```

## Use (Plan C smart builds, production)

```bash
cargo run -p keplr-cli -- --root . run lint
cargo run -p keplr-cli -- --root . run --all --jobs 4
cargo run -p keplr-cli -- --root . run lint --force
cargo run -p keplr-cli -- --root . run --all --watch
curl '127.0.0.1:7137/tasks/graph'
curl -X POST 127.0.0.1:7137/tasks/run -H 'Content-Type: application/json' -d '{"all":true,"jobs":4}'
```

## Use (Plan D index + watcher, production)

```bash
cargo run -p keplr-cli -- --root . index
cargo run -p keplr-cli -- --root . index --refresh
cargo run -p keplr-cli -- --root . search --via trigram "broadcast" --limit 20
cargo run -p keplr-cli -- --root . search --via index "broadcast" --limit 20
cargo run -p keplr-cli -- --root . watch --debounce-ms 500
curl '127.0.0.1:7137/index/status'
```

## Use (Plan D lang, production)

```bash
cargo run -p keplr-cli -- --root ~/LAML diagnostics ng/src/main.lm
curl '127.0.0.1:7137/diagnostics?path=Cargo.toml'
curl '127.0.0.1:7137/highlight?path=Cargo.toml&line=1'
```

## Use (Plan D sync, production)

```bash
curl '127.0.0.1:7137/lfs/pointer?path=assets/font.woff2'
curl -X POST 127.0.0.1:7137/sync/merge -H 'Content-Type: application/json' -d '{"name":"notes","seed":"hello","updates":[]}'
curl -X POST 127.0.0.1:7137/sync/snapshot -H 'Content-Type: application/json' -d '{"name":"notes","update":[1,2,3]}'
curl '127.0.0.1:7137/sync/snapshot?name=notes'
```

CI runs `cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace` on every push.
