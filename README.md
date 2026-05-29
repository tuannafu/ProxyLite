# ProxyLite

ProxyLite là app Rust GUI nhẹ để tạo nhanh proxy HTTP/HTTPS và SOCKS5 trên VPS Windows/Linux.

## Chạy và build trên Windows

```powershell
cargo run
cargo build --release
```

File build release nằm tại:

```text
target\release\proxylite.exe
```

## Build kèm icon

Icon Windows của app dùng file `favicon.ico` ở thư mục gốc project. Build script `build.rs` sẽ tự nhúng icon này vào file `.exe` khi build trên Windows.

```powershell
cargo build --release
```

Nếu muốn đổi icon, thay `favicon.ico` bằng file `.ico` mới rồi chạy lại:

```powershell
cargo clean
cargo build --release
```

Sau khi build xong, kiểm tra icon ở:

```text
target\release\proxylite.exe
```

## Tạo nhiều proxy IPv6

Có thể tạo nhiều proxy IPv6 nếu VPS có nhiều địa chỉ IPv6 public đã được gán/routed vào card mạng. Nhập mỗi IPv6 trên một dòng trong ô `Bind IP(s)`, hoặc ngăn cách bằng dấu phẩy. ProxyLite sẽ tạo listener riêng cho từng IP và từng dịch vụ đang bật.

Ví dụ:

```text
2001:db8:1234:abcd::10
2001:db8:1234:abcd::11
2001:db8:1234:abcd::12
```

Client sẽ dùng dạng:

```text
http://user:pass@[2001:db8:1234:abcd::10]:8080
socks5://user:pass@[2001:db8:1234:abcd::10]:1080
```

Mặc định app bật đồng thời HTTP/HTTPS port `8080` và SOCKS5 port `1080`. Có thể bật/tắt từng dịch vụ trong tab `Cấu hình`. Tab `Thống kê` hiển thị realtime connect, packet, lỗi, từ chối auth và traffic hai chiều.

## Kiểm tra VPS có hỗ trợ IPv6 public

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

Địa chỉ `fe80::/10` chỉ là link-local, `::1` là loopback, `fc00::/7` hoặc `fd00::/8` là private/ULA. Các địa chỉ này không phải IPv6 public để người ngoài Internet kết nối vào proxy. VPS cần có IPv6 global, thường bắt đầu bằng `2xxx:` hoặc `3xxx:`.

## Mở firewall

Windows PowerShell:

```powershell
New-NetFirewallRule -DisplayName "ProxyLite" -Direction Inbound -Protocol TCP -LocalPort 8080,1080 -Action Allow
```

Ubuntu/Debian:

```bash
sudo ufw allow 8080/tcp
sudo ufw allow 1080/tcp
```

CentOS/RHEL:

```bash
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --permanent --add-port=1080/tcp
sudo firewall-cmd --reload
```

## Ghi chú

Máy Windows đang test hiện tại có IPv6 stack nhưng chưa thấy IPv6 public, chỉ có `fe80::`, `::1` và IPv6 private của Tailscale. Vì vậy có thể build/chạy app, nhưng để test proxy IPv6 public cần VPS có IPv6 global thật.