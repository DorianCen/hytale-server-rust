# Packaging and deployment

This document covers the v1 Rust CoreServer release artifact, supported build
platforms, service deployment and the reproducible packaging command.

The server is a native Rust binary. It does **not** ship a JVM, load Java
plugins, or claim Java plugin compatibility. Asset packs are data and are
covered separately in [`ASSET_MODS.md`](ASSET_MODS.md).

## Supported build hosts

| Host | Status |
|---|---|
| Linux x86_64 | Primary target; the release script and systemd unit are tested here. |
| Linux aarch64 | Build from source with a Rust target; no prebuilt artifact is published in this repo. |
| Windows x86_64 | Build from source with MSVC; use the same `--locked` command. The systemd unit does not apply. |
| macOS | Build from source. No service manager unit is provided. |

The tested toolchain is recorded by `rustc --version` in `BUILD-INFO.txt` inside
the artifact. Use `rustup` and the same stable toolchain when reproducing a
release.

## Build from source

Prerequisites: Rust stable, a C toolchain, and the base-game asset pack at
`Assets/Assets.zip` (or an extracted directory pack).

```sh
cargo build --release --locked -p hytale-server -p client-bot
./target/release/hytale-server --help
```

The binary needs an asset pack and a writable universe directory. Validate the
pack before serving:

```sh
./target/release/hytale-server --validate-assets --pack Assets/Assets.zip
```

## Release artifact

```sh
tools/release.sh
```

The script:

1. builds `hytale-server` and `client-bot` with `--release --locked`;
2. stages the binaries, deployment unit and user docs;
3. creates a deterministic `tar.gz` (sorted entries, fixed mtime, no gzip
   timestamp);
4. writes a `.sha256` checksum next to the archive.

Output defaults to `dist/`; override with `HYTALE_RELEASE_OUT`. The staged tree
looks like:

```text
hytale-server-<version>-<target>/
  bin/hytale-server
  bin/client-bot
  share/doc/PACKAGING.md
  share/doc/PLUGINS.md
  share/doc/ASSET_MODS.md
  share/doc/TESTING.md
  share/deploy/hytale-server.service
  BUILD-INFO.txt
```

Reproducibility: the archive layout and checksum are reproducible for the same
source revision, toolchain and `SOURCE_DATE_EPOCH` (defaults to the git commit
time). Bit-identical binaries across different hosts also require the same
compiler, target and link environment; record `BUILD-INFO.txt` with any
comparison.

## Server layout

A service deployment normally uses:

```text
/srv/hytale/
  assets/
    Assets.zip           # or an extracted directory pack
    extracted/           # prefabs, environments, tint gradients
  universe/              # universe.json + one directory per world
  logs/
```

The process writes player saves, chunk files, `config.json`, `logs/` and
telemetry under its working directory/universe. Back up the universe directory
and the world save directories together; do not copy a live region file without
quiescing the server.

## Linux systemd

Install the binary and unit:

```sh
install -Dm755 target/release/hytale-server /usr/local/bin/hytale-server
install -Dm644 deploy/hytale-server.service /etc/systemd/system/hytale-server.service
install -d -o hytale -g hytale /srv/hytale /srv/hytale/assets /srv/hytale/logs
systemctl daemon-reload
systemctl enable --now hytale-server
```

Create `/etc/hytale/server.env` for secrets such as live auth tokens. The unit
runs as the unprivileged `hytale` user, restricts writes to `/srv/hytale`, and
opens UDP `5520` only through the configured bind address.

Check status and logs:

```sh
systemctl status hytale-server
journalctl -u hytale-server -f
```

## Network

- Default bind: `127.0.0.1:5520`; set `--bind 0.0.0.0:5520` or `[::]:5520` to
  listen publicly.
- QUIC uses UDP; open the UDP port, not TCP.
- The loopback companion listener covers both `127.0.0.1` and `::1` when the
  bind address is loopback and the port is not ephemeral.
- Put a firewall in front of the service and do not expose the process as root.

## Configuration and auth

The server reads its universe at `--universe` and the selected world with
`--world`. `--auth-mode insecure` is the development path; `local` uses a local
Ed25519 authority; `authenticated` performs the live Hytale session-service flow
or consumes `HYTALE_SERVER_SESSION_TOKEN` / `HYTALE_SERVER_IDENTITY_TOKEN`.

Live credentials belong in the service environment file, never in the universe
directory or command line visible in `ps`.

## Upgrades and rollback

1. Stop the service (`systemctl stop hytale-server`).
2. Back up `universe/` and the world save directories.
3. Replace the binary and docs from the new artifact.
4. Run `--validate-assets` against the deployed pack.
5. Start the service and check `journalctl` for session/asset errors.
6. To roll back, restore the previous binary and universe backup; do not mix
   worlds written by a newer schema with an older binary without a backup.

## Verification checklist

```sh
hytale-server --version
hytale-server --validate-assets --pack /srv/hytale/assets/Assets.zip
hytale-server serve --universe /srv/hytale/universe --world default --auth-mode insecure
```

For release packaging checks:

```sh
tools/release.sh
(cd dist && sha256sum -c hytale-server-*.tar.gz.sha256)
tar -tzf dist/hytale-server-*.tar.gz | head
```
