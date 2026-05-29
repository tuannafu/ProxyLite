#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod proxy;

use std::fs;
use std::net::TcpListener as StdTcpListener;
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, RichText};
use proxy::{ProxyConfig, ProxyMode};
use tokio::sync::watch;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ProxyLite")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([960.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ProxyLite",
        options,
        Box::new(|creation_context| {
            setup_fonts(&creation_context.egui_ctx);
            apply_theme(&creation_context.egui_ctx);
            creation_context.egui_ctx.set_theme(egui::Theme::Dark);
            creation_context
                .egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
            Ok(Box::new(ProxyLiteApp::default()))
        }),
    )
}

struct ServerHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handles: Vec<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Overview,
    Statistics,
    Configuration,
    Firewall,
    Client,
    Logs,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Tổng quan",
            Tab::Statistics => "Thống kê",
            Tab::Configuration => "Cấu hình",
            Tab::Firewall => "Firewall",
            Tab::Client => "Client",
            Tab::Logs => "Nhật ký",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Overview => "□",
            Tab::Statistics => "▤",
            Tab::Configuration => "⚙",
            Tab::Firewall => "▣",
            Tab::Client => "↗",
            Tab::Logs => "≡",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Tab::Overview => "Tổng quan nhanh trạng thái proxy",
            Tab::Statistics => "Connect, packet và traffic realtime",
            Tab::Configuration => "Bind IP, cổng, chế độ và xác thực",
            Tab::Firewall => "Lệnh mở port cho VPS Windows / Linux",
            Tab::Client => "URL proxy mẫu để dùng ngay",
            Tab::Logs => "Nhật ký kết nối thời gian thực",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct RuntimeStats {
    accepted_connections: u64,
    active_connections: u64,
    http_connects: u64,
    http_requests: u64,
    socks5_connects: u64,
    auth_rejections: u64,
    errors: u64,
    bytes_from_client: u64,
    bytes_from_upstream: u64,
}

impl RuntimeStats {
    fn total_packets(&self) -> u64 {
        self.http_connects + self.http_requests + self.socks5_connects
    }

    fn total_bytes(&self) -> u64 {
        self.bytes_from_client + self.bytes_from_upstream
    }
}

struct ProxyLiteApp {
    active_tab: Tab,
    bind_host: String,
    enable_http: bool,
    enable_socks5: bool,
    http_port_text: String,
    socks5_port_text: String,
    require_auth: bool,
    username: String,
    password: String,
    ipv6_status: String,
    status: String,
    stats: RuntimeStats,
    logs: Vec<String>,
    log_rx: Option<Receiver<String>>,
    server_handle: Option<ServerHandle>,
}

impl Default for ProxyLiteApp {
    fn default() -> Self {
        Self {
            active_tab: Tab::Overview,
            bind_host: "0.0.0.0".to_owned(),
            enable_http: true,
            enable_socks5: true,
            http_port_text: "8080".to_owned(),
            socks5_port_text: "1080".to_owned(),
            require_auth: true,
            username: "proxyuser".to_owned(),
            password: "changeme".to_owned(),
            ipv6_status: check_ipv6_support(),
            status: "Sẵn sàng".to_owned(),
            stats: RuntimeStats::default(),
            logs: vec![format!("{} ProxyLite sẵn sàng", timestamp())],
            log_rx: None,
            server_handle: None,
        }
    }
}

impl eframe::App for ProxyLiteApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_logs();
        if self.is_running() {
            context.request_repaint_after(Duration::from_millis(250));
        }
        self.render_sidebar(context);
        self.render_topbar(context);
        self.render_content(context);
    }
}

impl ProxyLiteApp {
    fn render_sidebar(&mut self, context: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .exact_width(228.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(10, 15, 26))
                    .inner_margin(egui::Margin::symmetric(16, 22)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("◈")
                            .size(22.0)
                            .color(Color32::from_rgb(96, 165, 250)),
                    );
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ProxyLite")
                                .size(18.0)
                                .strong()
                                .color(Color32::from_rgb(236, 244, 255)),
                        );
                        ui.label(
                            RichText::new("Rust · egui · 2026")
                                .size(11.0)
                                .color(Color32::from_rgb(120, 138, 168)),
                        );
                    });
                });
                ui.add_space(24.0);
                ui.label(
                    RichText::new("ĐIỀU HƯỚNG")
                        .size(10.0)
                        .color(Color32::from_rgb(105, 122, 152)),
                );
                ui.add_space(10.0);

                for tab in [
                    Tab::Overview,
                    Tab::Statistics,
                    Tab::Configuration,
                    Tab::Firewall,
                    Tab::Client,
                    Tab::Logs,
                ] {
                    sidebar_item(ui, tab, &mut self.active_tab);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new("© 2026 · by TuanTep")
                            .size(11.0)
                            .color(Color32::from_rgb(130, 148, 178)),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("HTTP · HTTPS · SOCKS5")
                            .size(11.0)
                            .color(Color32::from_rgb(110, 128, 158)),
                    );
                    ui.label(
                        RichText::new("VPS Windows · Linux")
                            .size(11.0)
                            .color(Color32::from_rgb(110, 128, 158)),
                    );
                    ui.add_space(4.0);
                });
            });
    }

    fn render_topbar(&self, context: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(72.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(14, 20, 34))
                    .inner_margin(egui::Margin::symmetric(28, 18)),
            )
            .show(context, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.active_tab.label())
                                .size(20.0)
                                .strong()
                                .color(Color32::from_rgb(236, 244, 255)),
                        );
                        ui.label(
                            RichText::new(self.active_tab.subtitle())
                                .size(12.0)
                                .color(Color32::from_rgb(140, 158, 188)),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_pill(
                            ui,
                            if self.is_running() {
                                "● Đang chạy"
                            } else {
                                "○ Đã dừng"
                            },
                            self.is_running(),
                        );
                    });
                });
            });
    }

    fn render_content(&mut self, context: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(12, 18, 30))
                    .inner_margin(egui::Margin::symmetric(28, 24)),
            )
            .show(context, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.active_tab {
                        Tab::Overview => self.render_overview(ui),
                        Tab::Statistics => self.render_statistics(ui),
                        Tab::Configuration => self.render_configuration(ui),
                        Tab::Firewall => self.render_firewall(ui),
                        Tab::Client => self.render_client(ui),
                        Tab::Logs => self.render_logs(ui),
                    });
            });
    }

    fn render_overview(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section_title(
                ui,
                "Điều khiển nhanh",
                "Khởi động hoặc dừng proxy ngay tại đây",
            );
            ui.horizontal(|ui| {
                let start_enabled = !self.is_running();
                if ui
                    .add_enabled(
                        start_enabled,
                        egui::Button::new(
                            RichText::new("▶  Khởi động proxy")
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(37, 99, 235))
                        .min_size(egui::vec2(190.0, 38.0)),
                    )
                    .clicked()
                {
                    self.start_proxy();
                }
                ui.add_space(10.0);
                let stop_enabled = self.is_running();
                if ui
                    .add_enabled(
                        stop_enabled,
                        egui::Button::new(RichText::new("■  Dừng").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(220, 38, 65))
                            .min_size(egui::vec2(130.0, 38.0)),
                    )
                    .clicked()
                {
                    self.stop_proxy();
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new(format!("Trạng thái: {}", self.status))
                        .size(12.5)
                        .color(Color32::from_rgb(160, 178, 208)),
                );
            });
        });

        ui.horizontal(|ui| {
            stat_card(
                ui,
                "Trạng thái",
                if self.is_running() {
                    "Đang chạy"
                } else {
                    "Đã dừng"
                },
                if self.is_running() {
                    Color32::from_rgb(74, 222, 128)
                } else {
                    Color32::from_rgb(148, 163, 184)
                },
            );
            stat_card(
                ui,
                "Dịch vụ",
                &self.service_summary(),
                Color32::from_rgb(96, 165, 250),
            );
            stat_card(
                ui,
                "Packet",
                &self.stats.total_packets().to_string(),
                Color32::from_rgb(192, 132, 252),
            );
            stat_card(
                ui,
                "Listener",
                &self.listener_count().to_string(),
                Color32::from_rgb(250, 204, 21),
            );
        });

        card(ui, |ui| {
            section_title(
                ui,
                "Endpoint công khai",
                "Địa chỉ để client kết nối tới proxy",
            );
            let hosts = parse_bind_hosts(&self.bind_host);
            if hosts.is_empty() {
                ui.label(
                    RichText::new("Chưa cấu hình bind IP")
                        .italics()
                        .color(Color32::from_rgb(160, 174, 196)),
                );
            } else {
                for host in hosts {
                    if self.enable_http {
                        ui.label(
                            RichText::new(format!(
                                "•  HTTP    {}",
                                public_endpoint_hint(&host, self.http_port_text.trim())
                            ))
                            .monospace()
                            .size(13.5),
                        );
                    }
                    if self.enable_socks5 {
                        ui.label(
                            RichText::new(format!(
                                "•  SOCKS5  {}",
                                public_endpoint_hint(&host, self.socks5_port_text.trim())
                            ))
                            .monospace()
                            .size(13.5),
                        );
                    }
                }
            }
        });

        card(ui, |ui| {
            section_title(ui, "IPv6", "Khả năng bind IPv6 trên máy hiện tại");
            ui.label(RichText::new(&self.ipv6_status).size(13.0));
            ui.add_space(8.0);
            if ui.button("Kiểm tra lại").clicked() {
                self.ipv6_status = check_ipv6_support();
            }
        });
    }

    fn render_statistics(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            stat_card(
                ui,
                "Connect tổng",
                &self.stats.accepted_connections.to_string(),
                Color32::from_rgb(96, 165, 250),
            );
            stat_card(
                ui,
                "Đang mở",
                &self.stats.active_connections.to_string(),
                Color32::from_rgb(74, 222, 128),
            );
            stat_card(
                ui,
                "Packet",
                &self.stats.total_packets().to_string(),
                Color32::from_rgb(192, 132, 252),
            );
            stat_card(
                ui,
                "Traffic",
                &format_bytes(self.stats.total_bytes()),
                Color32::from_rgb(250, 204, 21),
            );
        });

        card(ui, |ui| {
            section_title(ui, "Phân rã realtime", "HTTP, SOCKS5, xác thực và lỗi");
            metric_grid(ui, "HTTP CONNECT", self.stats.http_connects);
            metric_grid(ui, "HTTP request", self.stats.http_requests);
            metric_grid(ui, "SOCKS5 connect", self.stats.socks5_connects);
            metric_grid(ui, "Từ chối auth", self.stats.auth_rejections);
            metric_grid(ui, "Lỗi", self.stats.errors);
        });

        card(ui, |ui| {
            section_title(ui, "Traffic hai chiều", "Byte đã proxy qua tunnel");
            ui.horizontal(|ui| {
                stat_card(
                    ui,
                    "Client → Upstream",
                    &format_bytes(self.stats.bytes_from_client),
                    Color32::from_rgb(56, 189, 248),
                );
                stat_card(
                    ui,
                    "Upstream → Client",
                    &format_bytes(self.stats.bytes_from_upstream),
                    Color32::from_rgb(45, 212, 191),
                );
            });
            ui.add_space(8.0);
            if ui.button("Reset thống kê").clicked() {
                self.stats = RuntimeStats::default();
            }
        });
    }

    fn render_configuration(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section_title(ui, "Endpoint", "Bind IP và cổng lắng nghe");
            field_label(ui, "Bind IP(s)");
            ui.add(
                egui::TextEdit::multiline(&mut self.bind_host)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("0.0.0.0 hoặc mỗi IPv6 public trên một dòng"),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.enable_http, "Bật HTTP/HTTPS");
                    ui.add_enabled_ui(self.enable_http, |ui| {
                        field_label(ui, "Cổng HTTP/HTTPS");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.http_port_text)
                                .desired_width(150.0),
                        );
                    });
                });
                ui.add_space(28.0);
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.enable_socks5, "Bật SOCKS5");
                    ui.add_enabled_ui(self.enable_socks5, |ui| {
                        field_label(ui, "Cổng SOCKS5");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.socks5_port_text)
                                .desired_width(150.0),
                        );
                    });
                });
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("Mặc định bật cả HTTP/HTTPS :8080 và SOCKS5 :1080")
                    .size(12.0)
                    .color(Color32::from_rgb(150, 168, 198)),
            );
        });

        card(ui, |ui| {
            section_title(ui, "Xác thực", "Bật để yêu cầu username/password từ client");
            ui.checkbox(&mut self.require_auth, "Bật xác thực username/password");
            ui.add_enabled_ui(self.require_auth, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        field_label(ui, "Tài khoản");
                        ui.add(egui::TextEdit::singleline(&mut self.username).desired_width(240.0));
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        field_label(ui, "Mật khẩu");
                        ui.add(egui::TextEdit::singleline(&mut self.password).desired_width(240.0));
                    });
                });
            });
        });

        card(ui, |ui| {
            section_title(ui, "Hành động", "Khởi động hoặc dừng proxy");
            ui.horizontal(|ui| {
                let start_enabled = !self.is_running();
                if ui
                    .add_enabled(
                        start_enabled,
                        egui::Button::new(
                            RichText::new("▶  Khởi động proxy")
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(37, 99, 235))
                        .min_size(egui::vec2(190.0, 38.0)),
                    )
                    .clicked()
                {
                    self.start_proxy();
                }
                ui.add_space(10.0);
                let stop_enabled = self.is_running();
                if ui
                    .add_enabled(
                        stop_enabled,
                        egui::Button::new(RichText::new("■  Dừng").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(220, 38, 65))
                            .min_size(egui::vec2(130.0, 38.0)),
                    )
                    .clicked()
                {
                    self.stop_proxy();
                }
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Trạng thái hiện tại: {}", self.status))
                    .size(12.0)
                    .color(Color32::from_rgb(150, 168, 198)),
            );
        });
    }

    fn render_firewall(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section_title(
                ui,
                "Mở cổng trên VPS",
                "Chạy với quyền admin/root để cho phép kết nối từ Internet",
            );
            let ports = self.enabled_ports();
            let port_list = ports.join(",");
            command_box(
                ui,
                "Windows PowerShell",
                &format!(
                    "New-NetFirewallRule -DisplayName \"ProxyLite\" -Direction Inbound -Protocol TCP -LocalPort {} -Action Allow",
                    port_list
                ),
            );
            command_box(
                ui,
                "Ubuntu / Debian (ufw)",
                &ports
                    .iter()
                    .map(|port| format!("sudo ufw allow {}/tcp", port))
                    .collect::<Vec<_>>()
                    .join(" && "),
            );
            command_box(
                ui,
                "CentOS / RHEL (firewalld)",
                &format!(
                    "{} && sudo firewall-cmd --reload",
                    ports
                        .iter()
                        .map(|port| format!(
                            "sudo firewall-cmd --permanent --add-port={}/tcp",
                            port
                        ))
                        .collect::<Vec<_>>()
                        .join(" && ")
                ),
            );
        });

        card(ui, |ui| {
            section_title(
                ui,
                "Gợi ý bảo mật",
                "Khuyến nghị khi public proxy ra Internet",
            );
            for tip in [
                "Luôn bật xác thực username/password ở tab Cấu hình",
                "Giới hạn IP nguồn ở firewall nếu chỉ một số client cần kết nối",
                "Đặt cổng khác mặc định 8080/1080 để giảm scan tự động",
                "Theo dõi tab Nhật ký để phát hiện truy cập bất thường",
            ] {
                ui.label(RichText::new(format!("•  {}", tip)).size(13.0));
            }
        });
    }

    fn render_client(&mut self, ui: &mut egui::Ui) {
        let auth = if self.require_auth {
            format!("{}:{}@", self.username.trim(), self.password.trim())
        } else {
            String::new()
        };
        let hosts = parse_bind_hosts(&self.bind_host);

        card(ui, |ui| {
            section_title(
                ui,
                "URL proxy",
                "Dán vào trình duyệt, curl hoặc client SOCKS5",
            );
            if hosts.is_empty() {
                ui.label(
                    RichText::new("Chưa cấu hình bind IP. Hãy điền ở tab Cấu hình.")
                        .italics()
                        .color(Color32::from_rgb(160, 174, 196)),
                );
            }
            for host in &hosts {
                if self.enable_http {
                    command_box(
                        ui,
                        &format!("HTTP - {}", host),
                        &format!(
                            "http://{}{}",
                            auth,
                            public_endpoint_hint(host, self.http_port_text.trim())
                        ),
                    );
                }
                if self.enable_socks5 {
                    command_box(
                        ui,
                        &format!("SOCKS5 - {}", host),
                        &format!(
                            "socks5://{}{}",
                            auth,
                            public_endpoint_hint(host, self.socks5_port_text.trim())
                        ),
                    );
                }
            }
        });

        card(ui, |ui| {
            section_title(ui, "Ví dụ curl", "Kiểm tra nhanh proxy từ máy khác");
            let example_host = hosts
                .first()
                .cloned()
                .unwrap_or_else(|| "YOUR_VPS_IP".to_owned());
            if self.enable_http {
                command_box(
                    ui,
                    "HTTP",
                    &format!(
                        "curl -x http://{}{} https://ifconfig.co",
                        auth,
                        public_endpoint_hint(&example_host, self.http_port_text.trim())
                    ),
                );
            }
            if self.enable_socks5 {
                command_box(
                    ui,
                    "SOCKS5",
                    &format!(
                        "curl --socks5 {}{} https://ifconfig.co",
                        auth,
                        public_endpoint_hint(&example_host, self.socks5_port_text.trim())
                    ),
                );
            }
            if !self.enable_http && !self.enable_socks5 {
                ui.label(
                    RichText::new("Chưa bật dịch vụ nào ở tab Cấu hình.")
                        .italics()
                        .color(Color32::from_rgb(160, 174, 196)),
                );
            }
        });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, "Nhật ký kết nối", "Luồng truy cập và lỗi proxy");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Xóa nhật ký").color(Color32::WHITE))
                                .fill(Color32::from_rgb(55, 65, 81)),
                        )
                        .clicked()
                    {
                        self.logs.clear();
                    }
                });
            });

            egui::Frame::default()
                .fill(Color32::from_rgb(8, 13, 22))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(30, 41, 64)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .max_height(480.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            for line in &self.logs {
                                ui.label(
                                    RichText::new(line)
                                        .monospace()
                                        .size(12.5)
                                        .color(Color32::from_rgb(190, 205, 225)),
                                );
                            }
                        });
                });
        });
    }

    fn enabled_services(&self) -> Result<Vec<(ProxyMode, u16)>, String> {
        let mut services = Vec::new();
        if self.enable_http {
            services.push((
                ProxyMode::Http,
                parse_port(&self.http_port_text, "HTTP/HTTPS")?,
            ));
        }
        if self.enable_socks5 {
            services.push((
                ProxyMode::Socks5,
                parse_port(&self.socks5_port_text, "SOCKS5")?,
            ));
        }
        if services.is_empty() {
            return Err("Cần bật ít nhất HTTP/HTTPS hoặc SOCKS5".to_owned());
        }
        if services.len() == 2 && services[0].1 == services[1].1 {
            return Err(
                "HTTP/HTTPS và SOCKS5 không thể dùng cùng một cổng trên cùng IP".to_owned(),
            );
        }
        Ok(services)
    }

    fn enabled_ports(&self) -> Vec<String> {
        let mut ports = Vec::new();
        if self.enable_http {
            ports.push(self.http_port_text.trim().to_owned());
        }
        if self.enable_socks5 {
            ports.push(self.socks5_port_text.trim().to_owned());
        }
        if ports.is_empty() {
            ports.push("8080".to_owned());
        }
        ports
    }

    fn service_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.enable_http {
            parts.push(format!("HTTP:{}", self.http_port_text.trim()));
        }
        if self.enable_socks5 {
            parts.push(format!("SOCKS5:{}", self.socks5_port_text.trim()));
        }
        if parts.is_empty() {
            "Chưa bật".to_owned()
        } else {
            parts.join(" + ")
        }
    }

    fn listener_count(&self) -> usize {
        let service_count = usize::from(self.enable_http) + usize::from(self.enable_socks5);
        parse_bind_hosts(&self.bind_host).len() * service_count
    }

    fn start_proxy(&mut self) {
        let services = match self.enabled_services() {
            Ok(services) => services,
            Err(message) => {
                self.push_log(message);
                return;
            }
        };

        if self.require_auth && (self.username.trim().is_empty() || self.password.trim().is_empty())
        {
            self.push_log("Username/password không được để trống khi bật xác thực".to_owned());
            return;
        }

        let bind_hosts = parse_bind_hosts(&self.bind_host);
        if bind_hosts.is_empty() {
            self.push_log("Bind IP không được để trống".to_owned());
            return;
        }

        let (log_tx, log_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut join_handles = Vec::new();

        for bind_host in bind_hosts.iter().cloned() {
            for (mode, port) in services.iter().copied() {
                let config = ProxyConfig {
                    bind_host: bind_host.clone(),
                    port,
                    mode,
                    require_auth: self.require_auth,
                    username: self.username.trim().to_owned(),
                    password: self.password.trim().to_owned(),
                };
                let thread_log_tx = log_tx.clone();
                let thread_shutdown_rx = shutdown_rx.clone();
                join_handles.push(thread::spawn(move || {
                    let runtime = match tokio::runtime::Runtime::new() {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ =
                                thread_log_tx.send(format!("Không tạo được runtime: {}", error));
                            return;
                        }
                    };

                    if let Err(error) = runtime.block_on(proxy::run_proxy(
                        config,
                        thread_log_tx.clone(),
                        thread_shutdown_rx,
                    )) {
                        let _ = thread_log_tx.send(format!("Proxy dừng do lỗi: {}", error));
                    }
                }));
            }
        }

        self.log_rx = Some(log_rx);
        self.server_handle = Some(ServerHandle {
            shutdown_tx,
            join_handles,
        });
        self.status = format!(
            "Đang chạy {} listener ({})",
            bind_hosts.len() * services.len(),
            self.service_summary()
        );
        self.push_log(format!(
            "Khởi động {} trên {} bind IP",
            self.service_summary(),
            bind_hosts.len()
        ));
    }

    fn stop_proxy(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            let _ = handle.shutdown_tx.send(true);
            thread::spawn(move || {
                for join_handle in handle.join_handles {
                    let _ = join_handle.join();
                }
            });
        }
        self.status = "Đã dừng".to_owned();
        self.push_log("Đang yêu cầu dừng proxy".to_owned());
    }

    fn drain_logs(&mut self) {
        let mut pending = Vec::new();
        if let Some(log_rx) = &self.log_rx {
            while let Ok(message) = log_rx.try_recv() {
                pending.push(message);
            }
        }

        for message in pending {
            if let Some(event) = message.strip_prefix("__STAT__|") {
                self.record_stat_event(event);
            } else {
                self.push_log(message);
            }
        }

        if self.logs.len() > 500 {
            let remove_count = self.logs.len() - 500;
            self.logs.drain(0..remove_count);
        }
    }

    fn push_log(&mut self, message: String) {
        self.logs.push(format!("{} {}", timestamp(), message));
    }

    fn record_stat_event(&mut self, event: &str) {
        let mut parts = event.split('|');
        match parts.next().unwrap_or_default() {
            "accept" => {
                self.stats.accepted_connections += 1;
                self.stats.active_connections += 1;
            }
            "close" => {
                self.stats.active_connections = self.stats.active_connections.saturating_sub(1);
            }
            "http_connect" => {
                self.stats.http_connects += 1;
                self.add_transfer_bytes(parts.next(), parts.next());
            }
            "http_request" => {
                self.stats.http_requests += 1;
                self.add_transfer_bytes(parts.next(), parts.next());
            }
            "socks5" => {
                self.stats.socks5_connects += 1;
                self.add_transfer_bytes(parts.next(), parts.next());
            }
            "bytes" => self.add_transfer_bytes(parts.next(), parts.next()),
            "auth_reject" => self.stats.auth_rejections += 1,
            "error" => self.stats.errors += 1,
            _ => {}
        }
    }

    fn add_transfer_bytes(&mut self, from_client: Option<&str>, from_upstream: Option<&str>) {
        if let Some(value) = from_client.and_then(|value| value.parse::<u64>().ok()) {
            self.stats.bytes_from_client += value;
        }
        if let Some(value) = from_upstream.and_then(|value| value.parse::<u64>().ok()) {
            self.stats.bytes_from_upstream += value;
        }
    }

    fn is_running(&self) -> bool {
        self.server_handle.is_some()
    }
}

fn command_box(ui: &mut egui::Ui, label: &str, command: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(11.5)
                .color(Color32::from_rgb(160, 178, 208)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Sao chép").size(11.5).color(Color32::WHITE))
                        .fill(Color32::from_rgb(37, 99, 235))
                        .min_size(egui::vec2(70.0, 22.0)),
                )
                .clicked()
            {
                ui.ctx().copy_text(command.to_owned());
            }
        });
    });
    let mut text = command.to_owned();
    ui.add(
        egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(8.0);
}

fn public_endpoint_hint(bind_host: &str, port: &str) -> String {
    match bind_host.trim() {
        "" | "0.0.0.0" | "::" => format!("YOUR_VPS_IP:{}", port),
        value if value.contains(':') && !value.starts_with('[') => format!("[{}]:{}", value, port),
        value => format!("{}:{}", value, port),
    }
}

fn parse_bind_hosts(bind_hosts: &str) -> Vec<String> {
    bind_hosts
        .split(|character| matches!(character, '\n' | '\r' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_port(text: &str, label: &str) -> Result<u16, String> {
    match text.trim().parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(format!("Cổng {} không hợp lệ", label)),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

fn check_ipv6_support() -> String {
    match StdTcpListener::bind("[::1]:0") {
        Ok(listener) => format!(
            "có hỗ trợ bind IPv6 local ({})",
            listener.local_addr().unwrap()
        ),
        Err(error) => format!("chưa bind được IPv6 local: {}", error),
    }
}

fn setup_fonts(context: &egui::Context) {
    let text_font_paths = [
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    ];
    let symbol_font_paths = [
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\segmdl2.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ];

    let Some(text_bytes) = text_font_paths.iter().find_map(|path| fs::read(path).ok()) else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "proxy_lite_text".to_owned(),
        Arc::new(FontData::from_owned(text_bytes)),
    );

    let mut family_order = vec!["proxy_lite_text".to_owned()];
    if let Some(symbol_bytes) = symbol_font_paths
        .iter()
        .find_map(|path| fs::read(path).ok())
    {
        fonts.font_data.insert(
            "proxy_lite_symbol".to_owned(),
            Arc::new(FontData::from_owned(symbol_bytes)),
        );
        family_order.push("proxy_lite_symbol".to_owned());
    }

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(font_names) = fonts.families.get_mut(&family) {
            for (index, name) in family_order.iter().enumerate() {
                font_names.insert(index, name.clone());
            }
        }
    }

    context.set_fonts(fonts);
}

fn apply_theme(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(12, 18, 30);
    visuals.window_fill = Color32::from_rgb(17, 25, 40);
    visuals.extreme_bg_color = Color32::from_rgb(8, 13, 22);
    visuals.faint_bg_color = Color32::from_rgb(25, 36, 56);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(26, 38, 58);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 54, 82);
    visuals.widgets.active.bg_fill = Color32::from_rgb(37, 99, 235);
    visuals.selection.bg_fill = Color32::from_rgb(37, 99, 235);
    visuals.override_text_color = Some(Color32::from_rgb(226, 235, 248));

    let radius = egui::CornerRadius::same(8);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;
    visuals.menu_corner_radius = radius;
    visuals.window_corner_radius = egui::CornerRadius::same(12);

    context.set_visuals(visuals);

    let mut style = (*context.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size = egui::vec2(36.0, 30.0);
    context.set_style(style);
}

fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::default()
        .fill(Color32::from_rgb(17, 25, 40))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(36, 50, 76)))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(title)
                .size(17.0)
                .strong()
                .color(Color32::from_rgb(232, 240, 252)),
        );
        ui.label(
            RichText::new(subtitle)
                .size(12.5)
                .color(Color32::from_rgb(145, 162, 188)),
        );
    });
    ui.add_space(12.0);
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.5)
            .color(Color32::from_rgb(160, 178, 208)),
    );
    ui.add_space(2.0);
}

fn metric_grid(ui: &mut egui::Ui, label: &str, value: u64) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(13.0)
                .color(Color32::from_rgb(176, 190, 214)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value.to_string())
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(232, 240, 252)),
            );
        });
    });
    ui.separator();
}

fn status_pill(ui: &mut egui::Ui, text: &str, running: bool) {
    let (fill, text_color) = if running {
        (
            Color32::from_rgb(22, 101, 52),
            Color32::from_rgb(187, 247, 208),
        )
    } else {
        (
            Color32::from_rgb(30, 41, 59),
            Color32::from_rgb(203, 213, 225),
        )
    };
    egui::Frame::default()
        .fill(fill)
        .corner_radius(999.0)
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(text_color).size(12.5).strong());
        });
}

fn sidebar_item(ui: &mut egui::Ui, tab: Tab, active: &mut Tab) {
    let is_active = *active == tab;
    let fill = if is_active {
        Color32::from_rgb(37, 99, 235)
    } else {
        Color32::TRANSPARENT
    };
    let text_color = if is_active {
        Color32::WHITE
    } else {
        Color32::from_rgb(178, 192, 218)
    };

    let response = egui::Frame::default()
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(tab.icon()).size(15.0).color(text_color));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tab.label())
                        .size(13.5)
                        .strong()
                        .color(text_color),
                );
            });
        })
        .response
        .interact(egui::Sense::click());

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        *active = tab;
    }
    ui.add_space(4.0);
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: &str, accent: Color32) {
    egui::Frame::default()
        .fill(Color32::from_rgb(17, 25, 40))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(36, 50, 76)))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(160.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(label)
                        .size(11.5)
                        .color(Color32::from_rgb(150, 168, 198)),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(value).size(20.0).strong().color(accent));
            });
        });
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or_default();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
