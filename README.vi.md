# ProxyLite

ProxyLite là ứng dụng Rust nhẹ để chạy proxy HTTP/HTTPS và SOCKS5 trên Windows, Linux, macOS và VPS. Ứng dụng hỗ trợ cả giao diện GUI lẫn chế độ Headless CLI cho server Linux không có desktop.

> Tài liệu tiếng Anh chính: [`README.md`](README.md)

## Tính năng

- Proxy HTTP và tunnel HTTPS qua `CONNECT`.
- Proxy SOCKS5 có thể bật/tắt xác thực username/password.
- Hỗ trợ nhiều bind IP, bao gồm IPv6 public.
- GUI native dùng `eframe/egui`.
- Chế độ Headless CLI đầy đủ cho Linux server không GUI.
- Hỗ trợ file cấu hình TOML.
- Có lệnh hỗ trợ tự tạo Linux `systemd` service và in lệnh mở firewall.

## Chạy GUI

### Windows

```powershell
cargo run
cargo build --release
```

File release:

```text
target\release\proxylite.exe
```

### Linux desktop

ProxyLite GUI dùng `eframe/egui` với OpenGL, X11 và Wayland.

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

File release:

```text
target/release/proxylite
```

Chạy:

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

## Headless CLI cho Linux server

Chế độ headless không khởi tạo GUI, nên dùng được trên VPS/Linux server không có `DISPLAY`.

Build trước:

```bash
cargo build --release
```

Xem toàn bộ lệnh:

```bash
./target/release/proxylite --help
```

Chạy trực tiếp bằng tham số dòng lệnh:

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

Bind nhiều IP:

```bash
./target/release/proxylite headless \
  --bind 0.0.0.0 \
  --bind 2001:db8:1234:abcd::10 \
  --config proxylite.toml
```

Dừng bằng `Ctrl+C`.

## File cấu hình

Tạo file mẫu:

```bash
./target/release/proxylite init-config --output proxylite.toml
```

Ví dụ `proxylite.toml`:

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

Chạy theo config:

```bash
./target/release/proxylite headless --config proxylite.toml
```

In config mẫu ra terminal:

```bash
./target/release/proxylite print-config
```

## Tự cài thành Linux systemd service

Copy binary release vào vị trí ổn định:

```bash
sudo mkdir -p /opt/proxylite
sudo cp target/release/proxylite /opt/proxylite/proxylite
sudo chmod +x /opt/proxylite/proxylite
```

Cài service, tạo config mặc định, enable và start:

```bash
sudo /opt/proxylite/proxylite install-service \
  --bin /opt/proxylite/proxylite \
  --config /etc/proxylite/proxylite.toml \
  --create-config \
  --enable \
  --start
```

Kiểm tra trạng thái:

```bash
sudo systemctl status proxylite
```

Xem log:

```bash
journalctl -u proxylite -f
```

Sửa config và restart:

```bash
sudo nano /etc/proxylite/proxylite.toml
sudo systemctl restart proxylite
```

Gỡ service:

```bash
sudo /opt/proxylite/proxylite uninstall-service
```

## Firewall

In lệnh UFW theo config:

```bash
./target/release/proxylite firewall --config proxylite.toml --backend ufw
```

In lệnh firewalld:

```bash
./target/release/proxylite firewall --config proxylite.toml --backend firewalld
```

Windows PowerShell:

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

## Ví dụ client

HTTP/HTTPS:

```text
http://proxyuser:changeme@YOUR_VPS_IP:8080
```

SOCKS5:

```text
socks5://proxyuser:changeme@YOUR_VPS_IP:1080
```

Test bằng curl:

```bash
curl -x http://proxyuser:changeme@YOUR_VPS_IP:8080 https://ifconfig.co
curl --socks5 proxyuser:changeme@YOUR_VPS_IP:1080 https://ifconfig.co
```

Với IPv6 cần dùng dấu ngoặc vuông:

```text
http://proxyuser:changeme@[2001:db8:1234:abcd::10]:8080
socks5://proxyuser:changeme@[2001:db8:1234:abcd::10]:1080
```

## Kiểm tra IPv6 public

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

Địa chỉ `fe80::/10` là link-local, `::1` là loopback, `fc00::/7` hoặc `fd00::/8` là private/ULA. IPv6 public thường bắt đầu bằng `2xxx:` hoặc `3xxx:`.

## Build release bằng GitHub Actions

Repository đã có sẵn workflow release tại:

```text
.github/workflows/release.yml
```

Workflow này tự build và đóng gói ProxyLite cho:

- Windows x64: `proxylite-windows-x64.zip`
- Linux x64: `proxylite-linux-x64.tar.gz`
- macOS Intel x64: `proxylite-macos-x64.tar.gz`
- macOS Apple Silicon arm64: `proxylite-macos-arm64.tar.gz`

Mỗi file đóng gói có thêm file checksum `.sha256`.

### Tự động release bằng tag

Commit và push code lên GitHub, sau đó tạo tag version:

```bash
git tag v1.0.0
git push origin v1.0.0
```

GitHub Actions sẽ tự build đủ nền tảng và upload file vào GitHub Release của tag đó.

### Chạy workflow thủ công

Có thể chạy build thủ công:

1. Mở repository trên GitHub.
2. Vào `Actions`.
3. Chọn `Release`.
4. Bấm `Run workflow`.

Chạy thủ công sẽ upload file vào artifact của workflow run. Chạy bằng tag sẽ upload thêm vào GitHub Releases.

### Thiết lập quyền cần kiểm tra

Workflow dùng `GITHUB_TOKEN` với quyền `contents: write`. Nếu upload release lỗi, kiểm tra:

```text
Repository Settings → Actions → General → Workflow permissions → Read and write permissions
```

## Icon Windows

Icon Windows dùng file `favicon.ico` ở thư mục gốc. `build.rs` sẽ tự nhúng icon vào `.exe` khi build trên Windows.

```powershell
cargo clean
cargo build --release
```

Output:

```text
target\release\proxylite.exe
```
