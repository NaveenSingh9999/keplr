# Keplr remote operations

## Serve behind Tailscale

```bash
tailscale up
cargo run -p keplr-cli -- --root ~/LAML token --save
cargo run -p keplr-cli -- --root ~/LAML serve --bind 0.0.0.0 --port 7137
```

`--bind` defaults to `127.0.0.1`. Binding anything else without a token (flag, `KEPLR_TOKEN`, or `.keplr/token`) refuses to start unless `--allow-open-lan` is passed. Ten bad auth attempts per minute per IP get HTTP 429.

## systemd unit

See `docs/keplr-serve.service` — copy to `~/.config/systemd/user/`, edit paths, `systemctl --user enable --now keplr-serve`.

## Caddy reverse proxy (optional TLS)

```caddy
keplr.example.com {
    reverse_proxy 127.0.0.1:7137
}
```

Keep serve on loopback and let Caddy terminate TLS.

## Tokens

- `keplr token` prints one; `--save` writes `.keplr/token` (0600); `--rotate` replaces it.
- The browser page accepts `?token=`; API clients send `Authorization: Bearer`.

## Backups

Back up `keplr.json` and `.keplr/*.json` (journal, index). Skip `.keplr/cas` and `.keplr/snapshots` — content-addressed and re-fetchable.

## IME / touch

Terminal input (TUI, browser terminal) is key-event based: no IME composition or touch keyboard integration. The browser editor (CodeMirror) supports mobile keyboards and IME where the OS provides them. Native IME needs the OS window shell.
