use anyhow::{Context, Result, bail};
use std::env;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Lightpanda,
    Native,
}

impl Engine {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "lightpanda" => Ok(Self::Lightpanda),
            "native" => Ok(Self::Native),
            _ => bail!("unknown browser engine '{s}' (native|lightpanda)"),
        }
    }
}

pub fn binary() -> String {
    env::var("PERCH_LIGHTPANDA").unwrap_or_else(|_| "lightpanda".into())
}

fn lightpanda(args: &[&str]) -> Result<std::process::ExitStatus> {
    Command::new(binary()).args(args).status().with_context(|| {
        format!(
            "could not run '{}'. Install Lightpanda or set PERCH_LIGHTPANDA. See docs/browser.md",
            binary()
        )
    })
}

pub fn fetch(url: &str, engine: Engine, dump: &str) -> Result<()> {
    match engine {
        Engine::Lightpanda => {
            let status = lightpanda(&["fetch", url, "--dump", dump])?;
            if !status.success() {
                bail!("lightpanda fetch exited with {status}");
            }
            Ok(())
        }
        Engine::Native => bail!(
            "the native engine is the app's embedded WebView. The CLI cannot draw it.\n\
             Wire the harness in docs/browser.md, or use --engine lightpanda for headless runs."
        ),
    }
}

pub fn serve(port: u16) -> Result<u16> {
    let port_s = port.to_string();
    let child = Command::new(binary())
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port_s,
            "--remote-allow-origins",
            "http://localhost",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "could not start '{}'. Install Lightpanda or set PERCH_LIGHTPANDA",
                binary()
            )
        })?;
    println!(
        "lightpanda pid {}  cdp http://127.0.0.1:{port}\nconnect: ws://127.0.0.1:{port}",
        child.id()
    );
    drop(child);
    Ok(port)
}

pub fn doctor() -> Result<()> {
    println!("engine: lightpanda");
    println!("binary: {}", binary());
    let out = Command::new(binary()).arg("--version").output();
    match out {
        Ok(out) if out.status.success() => {
            println!("version: {}", String::from_utf8_lossy(&out.stdout).trim());
        }
        _ => println!("version: not found (native app WebView is still usable)"),
    }
    println!("note: Lightpanda does not paint pixels; captureScreenshot is a placeholder.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names_are_exact() {
        assert_eq!(Engine::parse("native").unwrap(), Engine::Native);
        assert_eq!(Engine::parse("lightpanda").unwrap(), Engine::Lightpanda);
        assert!(Engine::parse("chrome").is_err());
    }
}
