# Run a Relay

This guide covers how to run `grid-relayd` for relay-first NAT connectivity,
including local hosting, cloud deployment, firewall requirements, and relay
validation steps.

- [Prerequisites](#prerequisites)
- [Build and Run](#build-and-run)
- [Host Locally](#host-locally)
- [Host in the Cloud](#host-in-the-cloud)
- [Firewall and Network Rules](#firewall-and-network-rules)
- [Configure Zodes to Use the Relay](#configure-zodes-to-use-the-relay)
- [Test Relay Connectivity](#test-relay-connectivity)
- [Troubleshooting](#troubleshooting)

## Prerequisites

- Rust stable toolchain (edition 2021).
- Publicly reachable host for production use (VPS or cloud VM).
- A fixed public IP or DNS name for your relay host.

Build the relay binary:

```sh
cargo build --release -p grid-relayd
```

Binary path:

- Linux/macOS: `target/release/grid-relayd`
- Windows: `target\release\grid-relayd.exe`

## Build and Run

Minimal run command:

```sh
cargo run -p grid-relayd -- --listen /ip4/0.0.0.0/tcp/3691
```

With environment variables:

```sh
GRID_RELAY_LISTEN=/ip4/0.0.0.0/tcp/3691 \
GRID_RELAY_LOG=info \
GRID_RELAY_MAX_RESERVATIONS=256 \
GRID_RELAY_MAX_CIRCUITS=512 \
cargo run -p grid-relayd
```

Notes:

- CLI values override `GRID_RELAY_*` environment variables.
- `GRID_RELAY_LOG` falls back to `RUST_LOG` when unset.
- Relay limits are currently parsed and validated for ops safety checks.

## Host Locally

Use local hosting for development and LAN tests.

1. Start relay:

   ```sh
   cargo run -p grid-relayd -- --listen /ip4/0.0.0.0/tcp/3691
   ```

2. Get relay peer ID from logs or by running one zode and reading connected peer IDs.
3. Configure local zodes with:

   - `--enable-relay`
   - `--relay /ip4/127.0.0.1/tcp/3691/p2p/<relay-peer-id>`

## Host in the Cloud

Use a VM with a public IP (AWS EC2, GCP Compute Engine, Azure VM, DigitalOcean, etc.).

Recommended baseline:

- 1 vCPU / 1 GB RAM for small test traffic.
- Linux host.
- systemd service for auto-restart.

Example systemd unit (`/etc/systemd/system/grid-relayd.service`):

```ini
[Unit]
Description=GRID relay service
After=network-online.target
Wants=network-online.target

[Service]
User=grid
Group=grid
WorkingDirectory=/opt/the-grid
Environment=GRID_RELAY_LISTEN=/ip4/0.0.0.0/tcp/3691
Environment=GRID_RELAY_LOG=info
Environment=GRID_RELAY_MAX_RESERVATIONS=256
Environment=GRID_RELAY_MAX_CIRCUITS=512
ExecStart=/opt/the-grid/target/release/grid-relayd
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

Then:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now grid-relayd
sudo systemctl status grid-relayd
```

## Firewall and Network Rules

You must allow inbound traffic on your relay listen port.

If relay listens on `/ip4/0.0.0.0/tcp/3691`, configure:

- Cloud security group / network ACL: allow inbound `TCP 3691`.
- Host firewall (ufw/firewalld/iptables/Windows Defender): allow inbound `TCP 3691`.
- Outbound: allow established egress (default allow is usually fine).

If you run relay on a UDP QUIC multiaddr instead, allow UDP on that port.

Linux `ufw` example:

```sh
sudo ufw allow 3691/tcp
sudo ufw status
```

Windows PowerShell example (Admin):

```powershell
New-NetFirewallRule -DisplayName "GRID Relay TCP 3691" -Direction Inbound -Action Allow -Protocol TCP -LocalPort 3691
```

## Configure Zodes to Use the Relay

CLI example:

```sh
cargo run -p zode-cli -- \
  --enable-relay \
  --relay /ip4/<relay-public-ip>/tcp/3691/p2p/<relay-peer-id> \
  --bootstrap /ip4/<bootstrap-ip>/udp/3690/quic-v1/p2p/<bootstrap-peer-id>
```

`zode-app` example:

- Open Settings -> Relay.
- Enable relay transport.
- Add relay multiaddr with `/p2p/<relay-peer-id>`.
- Restart ZODE from Settings.

## Test Relay Connectivity

Use two NAT-restricted (or no direct route) zodes and one public relay.

### Quick functional check

1. Start relay on public host.
2. Start zode A and zode B with same relay and at least one bootstrap peer.
3. Confirm both connect and discover peers in logs/UI.
4. Send traffic (for example Interlink messages) and verify delivery.

### Verification checklist

- Relay process shows listening address.
- Zodes show peer connections after startup.
- No persistent dial errors to relay address.
- Data propagation works even when direct peer-to-peer path is unavailable.

## Troubleshooting

- `dial failure` to relay:
  - Check public IP/DNS.
  - Check `/p2p/<relay-peer-id>` suffix exists and matches relay identity.
  - Verify firewall/security-group inbound rules.
- Relay starts but no incoming peers:
  - Confirm external reachability from another host (`nc -vz <host> 3691` for TCP).
  - Check cloud provider network ACL and host firewall.
- Invalid env values:
  - `grid-relayd` exits fast with explicit variable name and parse error.
- Too much log noise:
  - Set `GRID_RELAY_LOG=warn` or `RUST_LOG=warn`.
