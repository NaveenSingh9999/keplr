# Bundled LAML interpreter

Keplr's event service is a LAML program, so a Keplr install carries the
interpreter with it. `scripts/fetch-laml.sh` downloads the pinned release from
<https://github.com/NaveenSingh9999/LAML/releases> into this directory.

The binaries themselves are not committed. The runtime resolves the interpreter
in this order:

1. `KEPLR_LAML`, an explicit path
2. `laml` next to the Keplr executable, which is where a package puts it
3. `laml` under `../lib/keplr`, which is where a deb or rpm puts it
4. `laml` on `PATH`, for development
5. this directory, so a source checkout works

If none of them exist, the event service is simply unavailable and Keplr runs
without it: terminals, files, and processes do not depend on LAML.
