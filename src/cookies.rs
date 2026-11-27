//! One-time native-cookie handoff for an agent session.
//!
//! The human signs in in the native tab. `omnix browser session import` asks
//! for confirmation, copies ONLY cookies scoped to the requested host into a
//! private CDP-format file, and Lightpanda reads that file read-only. Values are
//! never printed and never enter the history index.

use anyhow::{Context, Result, bail};
use gtk::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use webkit2gtk::{CookieManagerExt, WebContextExt, WebViewExt};
use wry::{WebViewBuilderExtUnix, WebViewExtUnix};

type CallbackResult = std::rc::Rc<std::cell::RefCell<Option<Result<Vec<CookieRow>, String>>>>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CookieRow {
    name: String,
    value: String,
    domain: String,
    path: String,
    expires: i64,
    http_only: bool,
    secure: bool,
    same_site: Option<String>,
}

fn normalize_domain(input: &str) -> Result<String> {
    let domain = input.trim().trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.contains('/')
        || domain.contains(':')
        || domain.contains('*')
        || domain.chars().any(char::is_whitespace)
    {
        bail!("--domain must be one host, for example app.example.com");
    }
    Ok(domain)
}

fn cookie_allowed(cookie_domain: &str, requested: &str) -> bool {
    cookie_domain
        .trim()
        .trim_start_matches('.')
        .eq_ignore_ascii_case(requested)
}

fn keep_domain(rows: &mut Vec<CookieRow>, domain: &str) {
    rows.retain(|row| cookie_allowed(&row.domain, domain));
}

fn default_path(root: &Path, out: Option<&str>) -> PathBuf {
    match out {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => crate::browser::cookie_session_path(root),
    }
}

fn confirm(domain: &str) -> Result<bool> {
    gtk::init().context("GTK could not start for the cookie confirmation")?;
    let text = format!(
        "Copy cookies for {domain} from the native browser into this agent session?\n\n\
         Only cookies with exactly this host are copied. Password and token values are never shown. \
         The file is deleted by `omnix browser session clear`."
    );
    let dialog = gtk::MessageDialog::new(
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Question,
        gtk::ButtonsType::YesNo,
        &text,
    );
    dialog.set_title("Omnix browser session");
    let response = dialog.run();
    dialog.close();
    Ok(response == gtk::ResponseType::Yes)
}

fn read_native_cookies(domain: &str) -> Result<Vec<CookieRow>> {
    // `--yes` skips the confirmation dialog, so GTK may not be initialized yet
    // and wry would panic before opening the persistent cookie store.
    gtk::init().context("GTK could not start for the cookie import")?;
    let data_dir = crate::browser::native_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let mut context = wry::WebContext::new(Some(data_dir));
    let window = gtk::OffscreenWindow::new();
    window.set_default_size(800, 600);
    window.move_(-32000, -32000);
    let fixed = gtk::Fixed::new();
    fixed.set_size_request(800, 600);
    window.add(&fixed);
    let rect = wry::Rect {
        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
        size: wry::dpi::PhysicalSize::new(800, 600).into(),
    };
    let view = wry::WebViewBuilder::new_with_web_context(&mut context)
        .with_bounds(rect)
        .with_devtools(false)
        .build_gtk(&fixed)
        .context("WebKit could not open the native cookie store")?;
    let webview = view.webview();
    let manager = webview
        .context()
        .and_then(|context| context.cookie_manager())
        .context("WebKit has no cookie manager")?;

    let result: CallbackResult = std::rc::Rc::new(std::cell::RefCell::new(None));
    let main = gtk::glib::MainLoop::new(None, false);
    let callback_result = std::rc::Rc::clone(&result);
    let callback_main = main.clone();
    let uri = format!("https://{domain}/");
    let wanted = domain.to_string();
    manager.cookies(&uri, None::<&gtk::gio::Cancellable>, move |cookies| {
        let rows = match cookies {
            Ok(cookies) => {
                let mut rows = Vec::new();
                for mut cookie in cookies {
                    let name = cookie.name().map(|v| v.to_string()).unwrap_or_default();
                    let value = cookie.value().map(|v| v.to_string()).unwrap_or_default();
                    let cookie_domain = cookie.domain().map(|v| v.to_string()).unwrap_or_default();
                    if name.is_empty() || !cookie_allowed(&cookie_domain, &wanted) {
                        continue;
                    }
                    rows.push(CookieRow {
                        name,
                        value,
                        domain: cookie_domain.trim_start_matches('.').to_ascii_lowercase(),
                        path: cookie
                            .path()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "/".into()),
                        expires: cookie.expires().map(|time| time.to_unix()).unwrap_or(-1),
                        http_only: cookie.is_http_only(),
                        secure: cookie.is_secure(),
                        same_site: match cookie.same_site_policy() {
                            soup::SameSitePolicy::Lax => Some("Lax".into()),
                            soup::SameSitePolicy::Strict => Some("Strict".into()),
                            soup::SameSitePolicy::None => Some("None".into()),
                            _ => None,
                        },
                    });
                }
                keep_domain(&mut rows, &wanted);
                Ok(rows)
            }
            Err(err) => Err(err.to_string()),
        };
        *callback_result.borrow_mut() = Some(rows);
        callback_main.quit();
    });

    let timeout_result = std::rc::Rc::clone(&result);
    let timeout_main = main.clone();
    gtk::glib::timeout_add_seconds_local_once(15, move || {
        if timeout_result.borrow().is_none() {
            *timeout_result.borrow_mut() = Some(Err("native cookie read timed out".into()));
            timeout_main.quit();
        }
    });
    window.show_all();
    main.run();
    window.hide();
    drop(view);
    drop(context);
    match result.borrow_mut().take() {
        Some(Ok(rows)) => Ok(rows),
        Some(Err(err)) => bail!("read native cookies: {err}"),
        None => bail!("native cookie read ended without a result"),
    }
}

fn write_cookies(path: &Path, rows: &[CookieRow]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let bytes = serde_json::to_vec_pretty(rows)?;
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn import(root: &Path, domain: &str, out: Option<&str>, yes: bool) -> Result<Value> {
    let domain = normalize_domain(domain)?;
    if !yes && !confirm(&domain)? {
        bail!("cookie import cancelled");
    }
    let rows = read_native_cookies(&domain)?;
    let path = default_path(root, out);
    write_cookies(&path, &rows)?;
    crate::control::event(
        "browser-cookie-import",
        json!({ "domain": domain, "count": rows.len(), "path": path.to_string_lossy() }),
    );
    Ok(json!({
        "domain": domain,
        "count": rows.len(),
        "path": path.to_string_lossy(),
        "command": format!("omnix browser mcp --cookie {}", path.display()),
    }))
}

pub fn clear(root: &Path, domain: &str, out: Option<&str>) -> Result<Value> {
    let domain = normalize_domain(domain)?;
    let path = default_path(root, out);
    let removed = std::fs::remove_file(&path).is_ok();
    crate::control::event(
        "browser-cookie-clear",
        json!({ "domain": domain, "path": path.to_string_lossy(), "removed": removed }),
    );
    Ok(json!({
        "domain": domain,
        "path": path.to_string_lossy(),
        "removed": removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_one_host_only() {
        assert_eq!(
            normalize_domain(" App.Example.com ").unwrap(),
            "app.example.com"
        );
        assert_eq!(
            normalize_domain(".app.example.com").unwrap(),
            "app.example.com"
        );
        assert!(normalize_domain("https://app.example.com").is_err());
        assert!(normalize_domain("*.example.com").is_err());
        assert!(cookie_allowed("app.example.com", "app.example.com"));
        assert!(cookie_allowed(".app.example.com", "app.example.com"));
        assert!(!cookie_allowed(".example.com", "app.example.com"));
        assert!(!cookie_allowed("panel.example.com", "app.example.com"));
    }

    #[test]
    fn cookie_file_has_no_parent_domain_and_is_private() {
        let root = std::env::temp_dir().join(format!(
            "omnix-cookie-{}-{}",
            std::process::id(),
            crate::control::epoch()
        ));
        let path = root.join("session.json");
        let rows = vec![
            CookieRow {
                name: "sid".into(),
                value: "secret".into(),
                domain: "app.example.com".into(),
                path: "/".into(),
                expires: -1,
                http_only: true,
                secure: true,
                same_site: Some("Lax".into()),
            },
            CookieRow {
                name: "other".into(),
                value: "nope".into(),
                domain: "panel.example.com".into(),
                path: "/".into(),
                expires: -1,
                http_only: false,
                secure: true,
                same_site: None,
            },
        ];
        let mut rows = rows;
        keep_domain(&mut rows, "app.example.com");
        write_cookies(&path, &rows).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("app.example.com"));
        assert!(!text.contains("panel.example.com"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
