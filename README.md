# ProxyLite

ProxyLite is a lightweight Rust desktop and headless proxy app for quickly running HTTP/HTTPS and SOCKS5 proxies on Windows, Linux, macOS, and VPS servers.

> Vietnamese documentation: [`README.vi.md`](README.vi.md)

## Features

- HTTP proxy and HTTPS `CONNECT` tunneling.
- SOCKS5 proxy with optional username/password authentication.
- Multiple bind IPs, including public IPv6 addresses.
- Native GUI powered by `eframe/egui`.
- Full headless CLI mode for Linux servers without a desktop environment.
- TOML config file support.
- Built-in helpers for Linux `systemd` service installation and firewall commands.

## Quick start with GUI

### Windows

```powershell
cargo run
cargo build --release
```

Release binary:

```text
target\release\proxylite.exe
```

### Linux desktop

ProxyLite GUI uses `eframe/egui` with OpenGL, X11 and Wayland support.

Ubuntu/Debian:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libx11-dev libxi-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev libxkbcommon-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
cargo build --release
```

Fedora / RHEL / Rocky Linux:

```bash
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y pkgconf-pkg-config libX11-devel libXi-devel mesa-libGL-devel mesa-libEGL-devel wayland-devel libxkbcommon-devel
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
cargo build --release
```

Release binary:

```text
target/release/proxylite
```

Run:

```bash
./target/release/proxylite
```

### macOS

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
cargo build --release
./target/release/proxylite
```

## Headless CLI for Linux servers

Headless mode does not start the GUI, so it works on VPS or Linux servers without `DISPLAY`.

Build first:

```bash
cargo build --release
```

Show all commands:

```bash
./target/release/proxylite --help
```

Run directly from command-line flags:

```bash
./target/release/proxylite headless \
  --bind 0.0.0.0 \
  --http \
  --http-port 8080 \
  --socks5 \
  --socks5-port 1080 \
  --auth \
  --username proxyuser \
  --password changeme
```

Bind multiple IPs:

```bash
./target/release/proxylite headless \
  --bind 0.0.0.0 \
  --bind 2001:db8:1234:abcd::10 \
  --config proxylite.toml
```

Stop with `Ctrl+C`.

## Config file

Create a sample config:

```bash
./target/release/proxylite init-config --output proxylite.toml
```

Example `proxylite.toml`:

```toml
bind_ips = ["0.0.0.0"]
http_enabled = true
http_port = 8080
socks5_enabled = true
socks5_port = 1080
require_auth = true
username = "proxyuser"
password = "changeme"
```

Run with config:

```bash
./target/release/proxylite headless --config proxylite.toml
```

Print a sample config to stdout:

```bash
./target/release/proxylite print-config
```

## Auto install as a Linux systemd service

Copy the release binary to a stable location:

```bash
sudo mkdir -p /opt/proxylite
sudo cp target/release/proxylite /opt/proxylite/proxylite
sudo chmod +x /opt/proxylite/proxylite
```

Install, create default config, enable, and start the service:

```bash
sudo /opt/proxylite/proxylite install-service \
  --bin /opt/proxylite/proxylite \
  --config /etc/proxylite/proxylite.toml \
  --create-config \
  --enable \
  --start
```

Check service status:

```bash
sudo systemctl status proxylite
```

View logs:

```bash
journalctl -u proxylite -f
```

Edit config:

```bash
sudo nano /etc/proxylite/proxylite.toml
sudo systemctl restart proxylite
```

Uninstall service:

```bash
sudo /opt/proxylite/proxylite uninstall-service
```

## Firewall helpers

Print UFW commands from config:

```bash
./target/release/proxylite firewall --config proxylite.toml --backend ufw
```

Print firewalld commands:

```bash
./target/release/proxylite firewall --config proxylite.toml --backend firewalld
```

Windows PowerShell example:

```powershell
New-NetFirewallRule -DisplayName "ProxyLite" -Direction Inbound -Protocol TCP -LocalPort 8080,1080 -Action Allow
```

Ubuntu/Debian:

```bash
sudo ufw allow 8080/tcp
sudo ufw allow 1080/tcp
```

CentOS/RHEL/Rocky Linux:

```bash
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --permanent --add-port=1080/tcp
sudo firewall-cmd --reload
```

## Client examples

HTTP/HTTPS:

```text
http://proxyuser:changeme@YOUR_VPS_IP:8080
```

SOCKS5:

```text
socks5://proxyuser:changeme@YOUR_VPS_IP:1080
```

Test with curl:

```bash
curl -x http://proxyuser:changeme@YOUR_VPS_IP:8080 https://ifconfig.co
curl --socks5 proxyuser:changeme@YOUR_VPS_IP:1080 https://ifconfig.co
```

For IPv6 clients, use brackets:

```text
http://proxyuser:changeme@[2001:db8:1234:abcd::10]:8080
socks5://proxyuser:changeme@[2001:db8:1234:abcd::10]:1080
```

## Check public IPv6 support

Windows PowerShell:

```powershell
Get-NetIPAddress -AddressFamily IPv6 | Select-Object IPAddress,PrefixLength,AddressState,Type,InterfaceAlias
Test-NetConnection -ComputerName ipv6.google.com -Port 80
```

Linux:

```bash
ip -6 addr show scope global
ip -6 route
curl -6 https://ifconfig.co
```

Addresses in `fe80::/10` are link-local, `::1` is loopback, and `fc00::/7` / `fd00::/8` are private ULA addresses. A public IPv6 address usually starts with `2xxx:` or `3xxx:`.

## GitHub Actions release builds

This repository includes a ready-to-use release workflow at:

```text
.github/workflows/release.yml
```

It builds and packages ProxyLite for:

- Windows x64: `proxylite-windows-x64.zip`
- Linux x64: `proxylite-linux-x64.tar.gz`
- macOS Intel x64: `proxylite-macos-x64.tar.gz`
- macOS Apple Silicon arm64: `proxylite-macos-arm64.tar.gz`

Each archive also gets a `.sha256` checksum file.

### Automatic release by tag

Commit and push your changes, then create a version tag:

```bash
git tag v1.0.0
git push origin v1.0.0
```

GitHub Actions will build all platforms and upload the archives to the GitHub Release for that tag.

### Manual workflow run

You can also run the workflow manually:

1. Open the GitHub repository.
2. Go to `Actions`.
3. Select `Release`.
4. Click `Run workflow`.

Manual runs upload build artifacts to the workflow run. Tagged runs additionally upload files to GitHub Releases.

### Required repository setting

The workflow uses `GITHUB_TOKEN` with `contents: write` permission. If release upload fails, check:

```text
Repository Settings → Actions → General → Workflow permissions → Read and write permissions
```

## Windows icon

The Windows executable icon uses `favicon.ico` from the project root. `build.rs` embeds it automatically during Windows builds.

```powershell
cargo clean
cargo build --release
```

Output:

```text
target\release\proxylite.exe
```
