use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::mpsc::Sender;

use base64::Engine;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyMode {
    Http,
    Socks5,
}

impl fmt::Display for ProxyMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyMode::Http => write!(formatter, "HTTP/HTTPS"),
            ProxyMode::Socks5 => write!(formatter, "SOCKS5"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProxyConfig {
    pub bind_host: String,
    pub port: u16,
    pub mode: ProxyMode,
    pub require_auth: bool,
    pub username: String,
    pub password: String,
}

impl ProxyConfig {
    pub fn bind_addr(&self) -> String {
        host_port(self.bind_host.trim(), self.port)
    }
}

pub fn host_port(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{}]:{}", host, port),
        _ => format!("{}:{}", host, port),
    }
}

pub async fn run_proxy(
    config: ProxyConfig,
    log_tx: Sender<String>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> io::Result<()> {
    let listener = TcpListener::bind(config.bind_addr()).await?;
    log(
        &log_tx,
        format!(
            "Listening for {} on {}",
            config.mode,
            listener.local_addr()?
        ),
    );

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            accept_result = listener.accept() => {
                let (client, peer_addr) = accept_result?;
                stat(&log_tx, format!("accept|{}", config.mode));
                let client_config = config.clone();
                let client_log_tx = log_tx.clone();
                tokio::spawn(async move {
                    let result = match client_config.mode {
                        ProxyMode::Http => handle_http_client(client, peer_addr, client_config, client_log_tx.clone()).await,
                        ProxyMode::Socks5 => handle_socks5_client(client, peer_addr, client_config, client_log_tx.clone()).await,
                    };

                    if let Err(error) = result {
                        stat(&client_log_tx, "error".to_owned());
                        log(&client_log_tx, format!("{} error: {}", peer_addr, error));
                    }
                    stat(&client_log_tx, "close".to_owned());
                });
            }
        }
    }

    log(&log_tx, "Proxy stopped".to_owned());
    Ok(())
}

async fn handle_http_client(
    mut client: TcpStream,
    peer_addr: SocketAddr,
    config: ProxyConfig,
    log_tx: Sender<String>,
) -> io::Result<()> {
    let header = read_http_header(&mut client).await?;
    let header_text = String::from_utf8_lossy(&header);

    if config.require_auth && !http_auth_ok(&header_text, &config.username, &config.password) {
        client
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"ProxyLite\"\r\nContent-Length: 0\r\n\r\n")
            .await?;
        stat(&log_tx, "auth_reject".to_owned());
        log(
            &log_tx,
            format!("{} rejected due to invalid HTTP authentication", peer_addr),
        );
        return Ok(());
    }

    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid HTTP request",
        ));
    }

    if parts[0].eq_ignore_ascii_case("CONNECT") {
        let target = parts[1];
        let mut upstream = connect_target(target, 443).await?;
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        log(&log_tx, format!("{} CONNECT {}", peer_addr, target));
        stat(&log_tx, "http_connect".to_owned());
        if let Ok((from_client, from_upstream)) =
            tokio::io::copy_bidirectional(&mut client, &mut upstream).await
        {
            stat(&log_tx, format!("bytes|{}|{}", from_client, from_upstream));
        }
        return Ok(());
    }

    let (host, port, path) = parse_http_absolute_uri(parts[1])?;
    let target = host_port(&host, port);
    let mut upstream = connect_target(&target, port).await?;
    let rewritten = rewrite_http_request(&header_text, parts[0], &path, parts[2]);
    upstream.write_all(rewritten.as_bytes()).await?;
    log(
        &log_tx,
        format!("{} HTTP {} {}", peer_addr, parts[0], target),
    );
    stat(&log_tx, "http_request".to_owned());
    if let Ok((from_client, from_upstream)) =
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await
    {
        stat(&log_tx, format!("bytes|{}|{}", from_client, from_upstream));
    }
    Ok(())
}

async fn read_http_header(client: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut header = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];

    while header.len() < 64 * 1024 {
        let read_count = timeout(Duration::from_secs(15), client.read(&mut byte)).await??;
        if read_count == 0 {
            break;
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            return Ok(header);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "HTTP header is too large or incomplete",
    ))
}

fn http_auth_ok(header_text: &str, username: &str, password: &str) -> bool {
    let expected =
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
    header_text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("proxy-authorization: basic ") && line.trim_end().ends_with(&expected)
    })
}

fn parse_http_absolute_uri(uri: &str) -> io::Result<(String, u16, String)> {
    let without_scheme = uri.strip_prefix("http://").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "HTTP proxy only supports http:// URLs or CONNECT for HTTPS",
        )
    })?;

    let (authority, path) = match without_scheme.split_once('/') {
        Some((authority, rest)) => (authority, format!("/{}", rest)),
        None => (without_scheme, "/".to_owned()),
    };

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port_text)) => (host.to_owned(), port_text.parse::<u16>().unwrap_or(80)),
        None => (authority.to_owned(), 80),
    };

    Ok((host, port, path))
}

fn rewrite_http_request(header_text: &str, method: &str, path: &str, version: &str) -> String {
    let mut rewritten = format!("{} {} {}\r\n", method, path, version);

    for line in header_text.split("\r\n").skip(1) {
        if line.is_empty() {
            break;
        }

        let lower = line.to_ascii_lowercase();
        if lower.starts_with("proxy-authorization:") || lower.starts_with("proxy-connection:") {
            continue;
        }

        rewritten.push_str(line);
        rewritten.push_str("\r\n");
    }

    rewritten.push_str("\r\n");
    rewritten
}

async fn handle_socks5_client(
    mut client: TcpStream,
    peer_addr: SocketAddr,
    config: ProxyConfig,
    log_tx: Sender<String>,
) -> io::Result<()> {
    let mut greeting = [0_u8; 2];
    client.read_exact(&mut greeting).await?;
    if greeting[0] != 5 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a SOCKS5 request",
        ));
    }

    let methods_len = greeting[1] as usize;
    let mut methods = vec![0_u8; methods_len];
    client.read_exact(&mut methods).await?;

    let selected_method = if config.require_auth {
        if methods.contains(&0x02) { 0x02 } else { 0xff }
    } else if methods.contains(&0x00) {
        0x00
    } else {
        0xff
    };

    client.write_all(&[0x05, selected_method]).await?;
    if selected_method == 0xff {
        return Ok(());
    }

    if selected_method == 0x02 {
        authenticate_socks5(&mut client, &config.username, &config.password).await?;
    }

    let mut request_head = [0_u8; 4];
    client.read_exact(&mut request_head).await?;
    if request_head[0] != 5 || request_head[1] != 1 {
        send_socks5_reply(&mut client, 0x07).await?;
        return Ok(());
    }

    let host = match request_head[3] {
        0x01 => {
            let mut bytes = [0_u8; 4];
            client.read_exact(&mut bytes).await?;
            IpAddr::V4(Ipv4Addr::from(bytes)).to_string()
        }
        0x03 => {
            let mut len = [0_u8; 1];
            client.read_exact(&mut len).await?;
            let mut bytes = vec![0_u8; len[0] as usize];
            client.read_exact(&mut bytes).await?;
            String::from_utf8_lossy(&bytes).to_string()
        }
        0x04 => {
            let mut bytes = [0_u8; 16];
            client.read_exact(&mut bytes).await?;
            IpAddr::V6(Ipv6Addr::from(bytes)).to_string()
        }
        _ => {
            send_socks5_reply(&mut client, 0x08).await?;
            return Ok(());
        }
    };

    let mut port_bytes = [0_u8; 2];
    client.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);
    let target = format!("{}:{}", host, port);

    match connect_target(&target, port).await {
        Ok(mut upstream) => {
            send_socks5_reply(&mut client, 0x00).await?;
            log(&log_tx, format!("{} SOCKS5 {}", peer_addr, target));
            stat(&log_tx, "socks5".to_owned());
            if let Ok((from_client, from_upstream)) =
                tokio::io::copy_bidirectional(&mut client, &mut upstream).await
            {
                stat(&log_tx, format!("bytes|{}|{}", from_client, from_upstream));
            }
        }
        Err(error) => {
            send_socks5_reply(&mut client, 0x05).await?;
            return Err(error);
        }
    }

    Ok(())
}

async fn authenticate_socks5(
    client: &mut TcpStream,
    username: &str,
    password: &str,
) -> io::Result<()> {
    let mut version = [0_u8; 1];
    client.read_exact(&mut version).await?;

    let mut username_len = [0_u8; 1];
    client.read_exact(&mut username_len).await?;
    let mut username_bytes = vec![0_u8; username_len[0] as usize];
    client.read_exact(&mut username_bytes).await?;

    let mut password_len = [0_u8; 1];
    client.read_exact(&mut password_len).await?;
    let mut password_bytes = vec![0_u8; password_len[0] as usize];
    client.read_exact(&mut password_bytes).await?;

    let provided_username = String::from_utf8_lossy(&username_bytes);
    let provided_password = String::from_utf8_lossy(&password_bytes);
    let success = version[0] == 1 && provided_username == username && provided_password == password;

    client
        .write_all(&[0x01, if success { 0x00 } else { 0x01 }])
        .await?;
    if success {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid SOCKS5 username/password",
        ))
    }
}

async fn send_socks5_reply(client: &mut TcpStream, status: u8) -> io::Result<()> {
    client
        .write_all(&[0x05, status, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
}

async fn connect_target(target: &str, default_port: u16) -> io::Result<TcpStream> {
    let target = if target.starts_with('[') || target.matches(':').count() == 1 {
        target.to_owned()
    } else if target.parse::<Ipv6Addr>().is_ok() {
        format!("[{}]:{}", target, default_port)
    } else if target.contains(':') {
        target.to_owned()
    } else {
        format!("{}:{}", target, default_port)
    };

    timeout(Duration::from_secs(12), TcpStream::connect(target)).await?
}

fn log(log_tx: &Sender<String>, message: String) {
    let _ = log_tx.send(message);
}

fn stat(log_tx: &Sender<String>, message: String) {
    let _ = log_tx.send(format!("__STAT__|{}", message));
}
