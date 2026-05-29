#[cfg(target_os = "windows")]
fn main() {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("favicon.ico");
    resource.set("FileDescription", "ProxyLite");
    resource.set("ProductName", "ProxyLite");
    resource.set("CompanyName", "TuanTep");
    resource.set("LegalCopyright", "© 2026 TuanTep");

    if let Err(error) = resource.compile() {
        panic!("Không nhúng được icon/favicon.ico vào exe: {error}");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {}
