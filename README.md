# Keplr

Personal lightweight Rust IDE — canvas renderer, Zed-exact dark UX,
headless `serve` + desktop native from one codebase, Git LFS native,
LAML first-class.

Spec: `docs/superpowers/specs/2026-09-17-keplr-design.md`
Plan: `docs/superpowers/plans/2026-09-18-keplr-production-foundation.md`

## Use (foundation CLI, production)

```bash
cargo run -p keplr-cli -- --root ~/LAML files "serve" --limit 20
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 20
cargo run -p keplr-cli -- --root . run check
cargo run -p keplr-cli -- --root . serve --port 7137
curl '127.0.0.1:7137/search?needle=hello&limit=5'
```

CI runs `cargo test`, `clippy -D warnings`, and `cargo build` on every push.
