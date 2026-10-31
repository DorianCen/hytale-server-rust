# Deployment example

`hytale-server.service` is a hardened systemd unit for a single server. Adjust
paths, user and bind address before installing.

## Layout

```text
/srv/hytale/
  assets/
    Assets.zip
    extracted/
  universe/
  share/doc/
  logs/
```

`/etc/hytale/server.env` is optional and should contain live-auth variables or
other secrets, never command-line-visible tokens.

## Install

```sh
useradd --system --home /srv/hytale --shell /usr/sbin/nologin hytale
install -d -o hytale -g hytale /srv/hytale/{assets,universe,logs,share/doc}
install -m 755 target/release/hytale-server /usr/local/bin/hytale-server
install -m 644 deploy/hytale-server.service /etc/systemd/system/
install -m 644 PACKAGING.md PLUGINS.md ASSET_MODS.md /srv/hytale/share/doc/
systemctl daemon-reload
systemctl enable --now hytale-server
```

Open UDP `5520` in the host firewall, not TCP. Keep `--bind` loopback if a QUIC
load balancer or tunnel terminates the public connection.

## Validate before starting

```sh
sudo -u hytale /usr/local/bin/hytale-server --validate-assets \
  --pack /srv/hytale/assets/Assets.zip
```

## Upgrade

Stop the service, back up `universe/`, replace the binary, run the validation
command, then start the service. See `PACKAGING.md` for rollback and backup
details.

## Windows and macOS

The unit is Linux/systemd-specific. On Windows, run the binary as a dedicated
service account with a writable universe directory; on macOS, use a LaunchAgent
or LaunchDaemon with the same paths and UDP port. No prebuilt installer is
provided by this repository.
