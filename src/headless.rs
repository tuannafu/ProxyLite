use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use clap::{CommandFactory, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::proxy::{ProxyConfig, ProxyMode};

const DEFAULT_CONFIG_PATH: &str = "proxylite.toml";

#[derive(Parser, Debug)]
#[command(name = "proxylite")]
#[command(version)]
#[command(about = "Lightweight HTTP/HTTPS and SOCKS5 proxy for VPS servers.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    /// Run without GUI using command line flags or a TOML config file.
    Headless(HeadlessArgs),
    /// Create a sample proxylite.toml config file.
    InitConfig(InitConfigArgs),
    /// Print an example config to stdout.
    PrintConfig,
    /// Install a Linux systemd service for headless mode.
    InstallService(ServiceArgs),
    /// Remove the Linux systemd service.
    UninstallService(ServiceRemoveArgs),
    /// Print shell commands to open firewall ports.
    Firewall(FirewallArgs),
}

#[derive(Parser, Debug)]
pub struct HeadlessArgs {
    /// Path to a TOML config file.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Bind IP. Repeat this flag to bind multiple IPs.
    #[arg(long = "bind")]
    pub bind_ips: Vec<String>,
    /// Enable HTTP/HTTPS proxy.
    #[arg(long)]
    pub http: bool,
    /// Disable HTTP/HTTPS proxy.
    #[arg(long)]
    pub no_http: bool,
    /// HTTP/HTTPS proxy port.
    #[arg(long)]
    pub http_port: Option<u16>,
    /// Enable SOCKS5 proxy.
    #[arg(long)]
    pub socks5: bool,
    /// Disable SOCKS5 proxy.
    #[arg(long)]
    pub no_socks5: bool,
    /// SOCKS5 proxy port.
    #[arg(long)]
    pub socks5_port: Option<u16>,
    /// Require username/password authentication.
    #[arg(long)]
    pub auth: bool,
    /// Disable authentication.
    #[arg(long)]
    pub no_auth: bool,
    /// Authentication username.
    #[arg(long)]
    pub username: Option<String>,
    /// Authentication password.
    #[arg(long)]
    pub password: Option<String>,
}

#[derive(Parser, Debug)]
pub struct InitConfigArgs {
    /// Output config path.
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH)]
    pub output: PathBuf,
    /// Overwrite the file if it already exists.
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser, Debug)]
pub struct ServiceArgs {
    /// Path to the proxylite binary used by systemd.
    #[arg(long)]
    pub bin: Option<PathBuf>,
    /// Path to the config file used by systemd.
    #[arg(short, long, default_value = "/etc/proxylite/proxylite.toml")]
    pub config: PathBuf,
    /// systemd service name.
    #[arg(long, default_value = "proxylite")]
    pub name: String,
    /// Linux user that runs the service.
    #[arg(long, default_value = "root")]
    pub user: String,
    /// Create default config if it does not exist.
    #[arg(long)]
    pub create_config: bool,
    /// Enable service after installing.
    #[arg(long)]
    pub enable: bool,
    /// Start service after installing.
    #[arg(long)]
    pub start: bool,
}

#[derive(Parser, Debug)]
pub struct ServiceRemoveArgs {
    /// systemd service name.
    #[arg(long, default_value = "proxylite")]
    pub name: String,
}

#[derive(Parser, Debug)]
pub struct FirewallArgs {
    /// Config file to read ports from.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Firewall backend.
    #[arg(long, default_value = "ufw")]
    pub backend: FirewallBackend,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum FirewallBackend {
    Ufw,
    Firewalld,
    Windows,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeadlessConfig {
    pub bind_ips: Vec<String>,
    pub http_enabled: bool,
    pub http_port: u16,
    pub socks5_enabled: bool,
    pub socks5_port: u16,
    pub require_auth: bool,
    pub username: String,
    pub password: String,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            bind_ips: vec!["0.0.0.0".to_owned()],
            http_enabled: true,
            http_port: 8080,
            socks5_enabled: true,
            socks5_port: 1080,
            require_auth: true,
            username: "proxyuser".to_owned(),
            password: "changeme".to_owned(),
        }
    }
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}

pub fn has_cli_args() -> bool {
    std::env::args_os().len() > 1
}

pub fn print_help() {
    let mut command = Cli::command();
    let _ = command.print_help();
    println!();
}

pub fn run_command(command: CliCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        CliCommand::Headless(args) => run_headless(args),
        CliCommand::InitConfig(args) => init_config(args),
        CliCommand::PrintConfig => {
            print_config();
            Ok(())
        }
        CliCommand::InstallService(args) => install_service(args),
        CliCommand::UninstallService(args) => uninstall_service(args),
        CliCommand::Firewall(args) => print_firewall(args),
    }
}

fn run_headless(args: HeadlessArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_effective_config(args)?;
    validate_config(&config)?;

    let listener_count = config.bind_ips.len()
        * usize::from(config.http_enabled || config.socks5_enabled)
        * if config.http_enabled && config.socks5_enabled {
            2
        } else {
            1
        };
    println!("ProxyLite headless starting with {listener_count} listener(s).");
    println!("Press Ctrl+C to stop.");

    let (log_tx, log_rx) = mpsc::channel::<String>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let runtime = tokio::runtime::Runtime::new()?;

    for bind_ip in &config.bind_ips {
        if config.http_enabled {
            let proxy_config = to_proxy_config(&config, bind_ip, ProxyMode::Http, config.http_port);
            let tx = log_tx.clone();
            let rx = shutdown_rx.clone();
            runtime.spawn(async move {
                if let Err(error) = crate::proxy::run_proxy(proxy_config, tx.clone(), rx).await {
                    let _ = tx.send(format!("Proxy stopped with error: {error}"));
                }
            });
        }
        if config.socks5_enabled {
            let proxy_config =
                to_proxy_config(&config, bind_ip, ProxyMode::Socks5, config.socks5_port);
            let tx = log_tx.clone();
            let rx = shutdown_rx.clone();
            runtime.spawn(async move {
                if let Err(error) = crate::proxy::run_proxy(proxy_config, tx.clone(), rx).await {
                    let _ = tx.send(format!("Proxy stopped with error: {error}"));
                }
            });
        }
    }

    runtime.block_on(async move {
        let log_task = tokio::task::spawn_blocking(move || {
            while let Ok(message) = log_rx.recv() {
                if !message.starts_with("__STAT__|") {
                    println!("{message}");
                }
            }
        });

        let _ = tokio::signal::ctrl_c().await;
        println!("Shutdown requested.");
        let _ = shutdown_tx.send(true);
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        drop(log_tx);
        let _ = log_task.await;
    });

    Ok(())
}

fn to_proxy_config(
    config: &HeadlessConfig,
    bind_ip: &str,
    mode: ProxyMode,
    port: u16,
) -> ProxyConfig {
    ProxyConfig {
        bind_host: bind_ip.to_owned(),
        port,
        mode,
        require_auth: config.require_auth,
        username: config.username.clone(),
        password: config.password.clone(),
    }
}

fn load_effective_config(args: HeadlessArgs) -> Result<HeadlessConfig, Box<dyn std::error::Error>> {
    let mut config = match args.config {
        Some(path) => read_config(&path)?,
        None => HeadlessConfig::default(),
    };

    if !args.bind_ips.is_empty() {
        config.bind_ips = args.bind_ips;
    }
    if args.http {
        config.http_enabled = true;
    }
    if args.no_http {
        config.http_enabled = false;
    }
    if args.socks5 {
        config.socks5_enabled = true;
    }
    if args.no_socks5 {
        config.socks5_enabled = false;
    }
    if args.auth {
        config.require_auth = true;
    }
    if args.no_auth {
        config.require_auth = false;
    }
    if let Some(http_port) = args.http_port {
        config.http_port = http_port;
    }
    if let Some(socks5_port) = args.socks5_port {
        config.socks5_port = socks5_port;
    }
    if let Some(username) = args.username {
        config.username = username;
    }
    if let Some(password) = args.password {
        config.password = password;
    }

    Ok(config)
}

fn read_config(path: &Path) -> Result<HeadlessConfig, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

fn validate_config(config: &HeadlessConfig) -> Result<(), Box<dyn std::error::Error>> {
    if config.bind_ips.is_empty() {
        return Err("bind_ips must contain at least one IP".into());
    }
    if !config.http_enabled && !config.socks5_enabled {
        return Err("at least one service must be enabled".into());
    }
    if config.http_enabled && config.socks5_enabled && config.http_port == config.socks5_port {
        return Err(
            "http_port and socks5_port must be different when both services are enabled".into(),
        );
    }
    if config.require_auth
        && (config.username.trim().is_empty() || config.password.trim().is_empty())
    {
        return Err("username and password are required when require_auth is true".into());
    }
    Ok(())
}

fn init_config(args: InitConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.output.exists() && !args.force {
        return Err(format!(
            "{} already exists. Use --force to overwrite.",
            args.output.display()
        )
        .into());
    }
    if let Some(parent) = args
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, sample_config_text())?;
    println!("Created {}", args.output.display());
    Ok(())
}

fn print_config() {
    print!("{}", sample_config_text());
}

fn sample_config_text() -> String {
    toml::to_string_pretty(&HeadlessConfig::default()).unwrap_or_else(|_| {
        r#"bind_ips = ["0.0.0.0"]
http_enabled = true
http_port = 8080
socks5_enabled = true
socks5_port = 1080
require_auth = true
username = "proxyuser"
password = "changeme"
"#
        .to_owned()
    })
}

fn install_service(args: ServiceArgs) -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "linux") {
        return Err("install-service is only supported on Linux systemd hosts".into());
    }

    let binary = match args.bin {
        Some(path) => path,
        None => std::env::current_exe()?,
    };

    if args.create_config && !args.config.exists() {
        if let Some(parent) = args.config.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&args.config, sample_config_text())?;
        println!("Created {}", args.config.display());
    }

    let service_path = PathBuf::from(format!("/etc/systemd/system/{}.service", args.name));
    let service_text = format!(
        "[Unit]\nDescription=ProxyLite Headless Proxy\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={}\nExecStart={} headless --config {}\nRestart=always\nRestartSec=3\nLimitNOFILE=1048576\n\n[Install]\nWantedBy=multi-user.target\n",
        args.user,
        shell_escape(&binary),
        shell_escape(&args.config)
    );

    fs::write(&service_path, service_text)?;
    println!("Created {}", service_path.display());
    run_systemctl(&["daemon-reload"])?;
    if args.enable {
        run_systemctl(&["enable", &args.name])?;
    }
    if args.start {
        run_systemctl(&["restart", &args.name])?;
    }
    println!(
        "Done. Check status with: sudo systemctl status {}",
        args.name
    );
    Ok(())
}

fn uninstall_service(args: ServiceRemoveArgs) -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "linux") {
        return Err("uninstall-service is only supported on Linux systemd hosts".into());
    }
    let _ = run_systemctl(&["stop", &args.name]);
    let _ = run_systemctl(&["disable", &args.name]);
    let service_path = PathBuf::from(format!("/etc/systemd/system/{}.service", args.name));
    if service_path.exists() {
        fs::remove_file(&service_path)?;
        println!("Removed {}", service_path.display());
    }
    run_systemctl(&["daemon-reload"])?;
    Ok(())
}

fn print_firewall(args: FirewallArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = match args.config {
        Some(path) => read_config(&path)?,
        None => HeadlessConfig::default(),
    };
    let mut ports = BTreeSet::new();
    if config.http_enabled {
        ports.insert(config.http_port);
    }
    if config.socks5_enabled {
        ports.insert(config.socks5_port);
    }
    for port in ports {
        match args.backend {
            FirewallBackend::Ufw => println!("sudo ufw allow {port}/tcp"),
            FirewallBackend::Firewalld => {
                println!("sudo firewall-cmd --permanent --add-port={port}/tcp")
            }
            FirewallBackend::Windows => println!(
                "New-NetFirewallRule -DisplayName \"ProxyLite {port}\" -Direction Inbound -Protocol TCP -LocalPort {port} -Action Allow"
            ),
        }
    }
    if matches!(args.backend, FirewallBackend::Firewalld) {
        println!("sudo firewall-cmd --reload");
    }
    Ok(())
}

fn run_systemctl(args: &[&str]) -> io::Result<()> {
    let status = Command::new("systemctl").args(args).status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "systemctl {} failed",
            args.join(" ")
        )));
    }
    Ok(())
}

fn shell_escape(path: &Path) -> String {
    let value = path.display().to_string();
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
