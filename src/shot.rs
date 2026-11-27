//! Native WebView screenshots for review work.
//!
//! Lightpanda does not paint pixels; a design review needs the same WebKit the
//! human sees. This module opens an off-screen GTK window, waits for the page,
//! asks WebKit for a real snapshot, and closes everything before returning.
//! Nothing stays alive after the command exits.

use anyhow::{Context, Result, bail};
use gtk::prelude::*;
use serde_json::{Value, json};
use std::env;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::time::Duration;
use webkit2gtk::{WebViewExt, glib};
use wry::{WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

/// One viewport preset. Values mirror a typical web-review preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
}

pub const VIEWPORTS: &[Viewport] = &[
    Viewport {
        name: "laptop",
        width: 1366,
        height: 768,
        mobile: false,
    },
    Viewport {
        name: "laptop-hd",
        width: 1280,
        height: 800,
        mobile: false,
    },
    Viewport {
        name: "iphone-se",
        width: 375,
        height: 812,
        mobile: true,
    },
    Viewport {
        name: "iphone",
        width: 390,
        height: 844,
        mobile: true,
    },
    Viewport {
        name: "pixel",
        width: 412,
        height: 915,
        mobile: true,
    },
];

pub fn viewport(name: &str) -> Option<Viewport> {
    VIEWPORTS.iter().copied().find(|v| v.name == name)
}

/// iPhone Safari UA is enough for layouts that branch on a phone; Omnix never
/// pretends to be a specific Chrome build.
pub const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

#[derive(Debug, Clone)]
pub struct ShotOptions {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
    pub dark: bool,
    pub full_page: bool,
    pub out: PathBuf,
}

impl ShotOptions {
    pub fn validate(&self) -> Result<()> {
        crate::browser::check_url(&self.url)?;
        if !(160..=4096).contains(&self.width) || !(160..=4096).contains(&self.height) {
            bail!("screenshot width and height must be between 160 and 4096");
        }
        if self.out.as_os_str().is_empty() {
            bail!("screenshot --out is required");
        }
        Ok(())
    }
}

/// C ABI symbols from the same WebKitGTK/GTK libraries wry already links.
mod ffi {
    use std::ffi::c_void;

    pub const REGION_VISIBLE: i32 = 0;
    pub const REGION_FULL_DOCUMENT: i32 = 1;
    pub const OPTIONS_NONE: i32 = 0;

    pub type Callback = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);

    unsafe extern "C" {
        pub fn webkit_web_view_get_snapshot(
            web_view: *mut c_void,
            region: i32,
            options: i32,
            cancellable: *mut c_void,
            callback: Option<Callback>,
            user_data: *mut c_void,
        );
        pub fn webkit_web_view_get_snapshot_finish(
            web_view: *mut c_void,
            result: *mut c_void,
            error: *mut *mut c_void,
        ) -> *mut c_void;
        pub fn cairo_surface_write_to_png(
            surface: *mut c_void,
            filename: *const std::ffi::c_char,
        ) -> i32;
        pub fn cairo_surface_destroy(surface: *mut c_void);
        pub fn cairo_image_surface_get_width(surface: *mut c_void) -> i32;
        pub fn cairo_image_surface_get_height(surface: *mut c_void) -> i32;
    }
}

struct SnapState {
    out: PathBuf,
    full_page: bool,
    outcome: std::rc::Rc<std::cell::RefCell<Option<Result<Value, String>>>>,
    main: glib::MainLoop,
}

unsafe extern "C" fn snapshot_done(view: *mut c_void, result: *mut c_void, user_data: *mut c_void) {
    let state = unsafe { Box::from_raw(user_data as *mut SnapState) };
    let surface =
        unsafe { ffi::webkit_web_view_get_snapshot_finish(view, result, std::ptr::null_mut()) };
    if surface.is_null() {
        *state.outcome.borrow_mut() = Some(Err("WebKit returned an empty snapshot".into()));
        state.main.quit();
        return;
    }
    let width = unsafe { ffi::cairo_image_surface_get_width(surface) };
    let height = unsafe { ffi::cairo_image_surface_get_height(surface) };
    let written = match CString::new(state.out.to_string_lossy().as_bytes()) {
        Ok(path) => unsafe { ffi::cairo_surface_write_to_png(surface, path.as_ptr()) },
        Err(_) => -1,
    };
    unsafe { ffi::cairo_surface_destroy(surface) };
    *state.outcome.borrow_mut() = Some(if written == 0 {
        Ok(json!({
            "path": state.out.to_string_lossy(),
            "width": width,
            "height": height,
            "fullPage": state.full_page,
        }))
    } else {
        Err(format!("could not write PNG to {}", state.out.display()))
    });
    state.main.quit();
}

/// A screenshot must map one CSS pixel to one PNG pixel. WebKitGTK inherits
/// the compositor scale (usually 2x on this desktop), which halves `innerWidth`
/// and doubles a full-page image. An X11 backend with a fixed 96 DPI gives a
/// deterministic 1x surface; fall back to the session default without XWayland.
fn force_unscaled_output() {
    if env::var_os("GDK_BACKEND").is_some() {
        return;
    }
    let Some(display) = env::var_os("DISPLAY") else {
        return;
    };
    let display = display.to_string_lossy();
    let number = display
        .split(':')
        .nth(1)
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse::<u32>().ok());
    if let Some(number) = number {
        if Path::new("/tmp/.X11-unix")
            .join(format!("X{number}"))
            .exists()
        {
            // SAFETY: screenshots run in their own short-lived CLI process,
            // before any GTK call, so changing the backend cannot race a thread.
            unsafe {
                env::set_var("GDK_BACKEND", "x11");
                env::set_var("GDK_SCALE", "1");
                env::set_var("GDK_DPI_SCALE", "1");
            }
        }
    }
}

pub fn capture(opts: &ShotOptions) -> Result<Value> {
    opts.validate()?;
    if let Some(dir) = opts.out.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("create screenshot folder {}", dir.display()))?;
        }
    }

    force_unscaled_output();
    gtk::init().context("GTK could not start for a native screenshot")?;
    let old_dpi = gtk::Settings::default().map(|settings| settings.gtk_xft_dpi());
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_xft_dpi(96 * 1024);
    }
    // WebKitGTK 2.54 has no `WebKitSettings:prefers-color-scheme`; it derives
    // the CSS media query from the GTK dark-theme preference. Setting this
    // before the view is created is therefore the supported path.
    let old_dark_pref =
        gtk::Settings::default().map(|settings| settings.is_gtk_application_prefer_dark_theme());
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(opts.dark);
    }

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.set_default_size(opts.width as i32, opts.height as i32);
    window.move_(-32000, -32000);
    let fixed = gtk::Fixed::new();
    fixed.set_size_request(opts.width as i32, opts.height as i32);
    window.add(&fixed);

    let rect = wry::Rect {
        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
        size: wry::dpi::PhysicalSize::new(opts.width, opts.height).into(),
    };
    let mut builder = WebViewBuilder::new()
        .with_bounds(rect)
        .with_devtools(false)
        .with_navigation_handler(|target| crate::browser::check_url(&target).is_ok());
    if opts.mobile {
        builder = builder.with_user_agent(MOBILE_UA);
    }
    let view = builder
        .build_gtk(&fixed)
        .context("WebKit could not create the off-screen view")?;
    let webview = view.webview();

    let outcome = std::rc::Rc::new(std::cell::RefCell::new(None));
    let main = glib::MainLoop::new(None, false);

    let loaded = std::rc::Rc::new(std::cell::Cell::new(false));
    let loaded_slot = std::rc::Rc::clone(&loaded);
    let shot_out = opts.out.clone();
    let shot_full = opts.full_page;
    let shot_outcome = std::rc::Rc::clone(&outcome);
    let shot_main = main.clone();
    let shot_view = webview.clone();
    webview.connect_load_changed(move |_, event| {
        if event != webkit2gtk::LoadEvent::Finished || loaded_slot.get() {
            return;
        }
        loaded_slot.set(true);
        let out = shot_out.clone();
        let outcome = std::rc::Rc::clone(&shot_outcome);
        let main = shot_main.clone();
        let view = shot_view.clone();
        // One short settle tick catches the first paint after `load` without
        // turning into a polling loop.
        glib::timeout_add_local_once(Duration::from_millis(350), move || {
            let state = Box::new(SnapState {
                out,
                full_page: shot_full,
                outcome,
                main,
            });
            unsafe {
                ffi::webkit_web_view_get_snapshot(
                    view.as_ptr() as *mut c_void,
                    if shot_full {
                        ffi::REGION_FULL_DOCUMENT
                    } else {
                        ffi::REGION_VISIBLE
                    },
                    ffi::OPTIONS_NONE,
                    std::ptr::null_mut(),
                    Some(snapshot_done),
                    Box::into_raw(state) as *mut c_void,
                );
            }
        });
    });

    let timeout_outcome = std::rc::Rc::clone(&outcome);
    let timeout_main = main.clone();
    glib::timeout_add_seconds_local_once(25, move || {
        if timeout_outcome.borrow().is_none() {
            *timeout_outcome.borrow_mut() =
                Some(Err("native screenshot timed out after 25s".into()));
            timeout_main.quit();
        }
    });

    window.show_all();
    view.load_url(&opts.url)
        .with_context(|| format!("load {}", opts.url))?;
    main.run();

    window.hide();
    drop(view);
    // A following command in the same process (selftest) must not inherit the
    // screenshot's color preference.
    if let (Some(settings), Some(previous)) = (gtk::Settings::default(), old_dark_pref) {
        settings.set_gtk_application_prefer_dark_theme(previous);
    }
    if let (Some(settings), Some(dpi)) = (gtk::Settings::default(), old_dpi) {
        settings.set_gtk_xft_dpi(dpi);
    }
    match outcome.borrow_mut().take() {
        Some(Ok(value)) => Ok(value),
        Some(Err(err)) => bail!("{err}"),
        None => bail!("native screenshot ended without a result"),
    }
}

#[allow(dead_code)]
pub fn path_for(root: &Path, name: &str) -> PathBuf {
    root.join(".omnix")
        .join("shots")
        .join(format!("{name}.png"))
}

/// Parse `omnix browser screenshot` flags. Kept separate from capture so tests
/// can exercise every combination without opening a window.
pub fn parse_args(args: &[String]) -> Result<ShotOptions> {
    let mut url: Option<String> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut preset: Option<String> = None;
    let mut mobile = false;
    let mut dark = false;
    let mut full_page = false;
    let mut out: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--native" => {}
            "--mobile" => mobile = true,
            "--dark" => dark = true,
            "--full-page" => full_page = true,
            "--width" | "--height" | "--preset" | "--out" | "--engine" => {
                let value = args
                    .get(i + 1)
                    .with_context(|| format!("{arg} needs a value"))?;
                match arg {
                    "--width" => width = Some(value.parse().context("--width must be a number")?),
                    "--height" => {
                        height = Some(value.parse().context("--height must be a number")?)
                    }
                    "--preset" => preset = Some(value.clone()),
                    "--out" => out = Some(PathBuf::from(value)),
                    "--engine" if value != "native" => {
                        bail!("screenshot uses the native WebView; drop --engine {value}")
                    }
                    _ => {}
                }
                i += 2;
                continue;
            }
            other if other.starts_with('-') => bail!("unknown screenshot flag '{other}'"),
            other => {
                if url.is_some() {
                    bail!("screenshot takes one URL");
                }
                url = Some(other.to_string());
            }
        }
        i += 1;
    }
    let url = url.context("usage: omnix browser screenshot <url> --out FILE")?;
    let mut opts = ShotOptions {
        url,
        width: width.unwrap_or(1366),
        height: height.unwrap_or(768),
        mobile,
        dark,
        full_page,
        out: out.context("screenshot --out FILE is required")?,
    };
    if let Some(name) = preset.as_deref() {
        let vp = viewport(name).with_context(|| {
            let names: Vec<&str> = VIEWPORTS.iter().map(|v| v.name).collect();
            format!("unknown viewport '{name}' (use {})", names.join(", "))
        })?;
        opts.width = vp.width;
        opts.height = vp.height;
        opts.mobile = opts.mobile || vp.mobile;
    }
    if opts.mobile && width.is_none() && height.is_none() && preset.is_none() {
        opts.width = 390;
        opts.height = 844;
    }
    opts.validate()?;
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_presets_are_stable() {
        assert_eq!(viewport("laptop").unwrap().width, 1366);
        assert_eq!(viewport("laptop-hd").unwrap().height, 800);
        let phone = viewport("iphone").unwrap();
        assert!(phone.mobile);
        assert_eq!((phone.width, phone.height), (390, 844));
        assert!(viewport("cinema").is_none());
    }

    #[test]
    fn options_reject_bad_input_without_opening_a_window() {
        let mut opts = ShotOptions {
            url: "https://example.com".into(),
            width: 1366,
            height: 768,
            mobile: false,
            dark: false,
            full_page: false,
            out: PathBuf::from("/tmp/omnix-shot.png"),
        };
        assert!(opts.validate().is_ok());
        opts.url = "file:///etc/passwd".into();
        assert!(opts.validate().is_err());
        opts.url = "https://example.com".into();
        opts.width = 10;
        assert!(opts.validate().is_err());
        opts.width = 1366;
        opts.out = PathBuf::new();
        assert!(opts.validate().is_err());
    }

    #[test]
    fn screenshot_flags_select_a_phone_and_dark_mode() {
        let args: Vec<String> = [
            "https://app.example.com/leads",
            "--preset",
            "iphone",
            "--dark",
            "--full-page",
            "--out",
            "/tmp/leads.png",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let opts = parse_args(&args).unwrap();
        assert_eq!((opts.width, opts.height), (390, 844));
        assert!(opts.mobile && opts.dark && opts.full_page);
        assert_eq!(opts.out, PathBuf::from("/tmp/leads.png"));
    }

    #[test]
    fn screenshot_rejects_lightpanda_engine_and_missing_out() {
        let args: Vec<String> = ["https://example.com", "--out", "/tmp/a.png"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&args).is_ok());
        let mut bad = args.clone();
        bad.push("--preset".into());
        bad.push("cinema".into());
        assert!(parse_args(&bad).is_err());
        let no_out: Vec<String> = ["https://example.com"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&no_out).is_err());
        let engine: Vec<String> = [
            "https://example.com",
            "--out",
            "/tmp/a.png",
            "--engine",
            "lightpanda",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(parse_args(&engine).is_err());
    }
}
