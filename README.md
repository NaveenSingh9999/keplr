# Keplr

Personal lightweight Rust IDE — canvas foundation, Zed-exact dark UX,
headless `serve` + desktop native from one codebase, Git LFS native,
LAML first-class.

Spec: `docs/superpowers/specs/2026-09-17-keplr-design.md`
Plan foundation: `docs/superpowers/plans/2026-09-18-keplr-production-foundation.md`
Plan B canvas: `docs/superpowers/plans/2026-09-18-keplr-plan-b-canvas-foundation.md`

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

CI runs `cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace` on every push.
