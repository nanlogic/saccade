//! Fixed-purpose browser lifecycle wake-up for a disconnected zero-window route.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const ROUTE_FILE: &str = "browser-wake-route.json";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserWakeRoute {
    pub schema: String,
    pub browser_family: String,
    pub development: bool,
    pub wake_url: String,
}

impl BrowserWakeRoute {
    pub fn new(browser_family: &str, development: bool, wake_url: &str) -> Result<Self> {
        let route = Self {
            schema: "saccade.browser-wake-route/1".into(),
            browser_family: browser_family.into(),
            development,
            wake_url: wake_url.into(),
        };
        route.validate()?;
        Ok(route)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != "saccade.browser-wake-route/1" {
            bail!("browser wake route used the wrong schema");
        }
        if !matches!(self.browser_family.as_str(), "chrome" | "edge") {
            bail!("browser wake route has an unsupported browser family");
        }
        let Some(rest) = self.wake_url.strip_prefix("chrome-extension://") else {
            bail!("browser wake URL must be an Extension URL");
        };
        let Some(id) = rest.strip_suffix("/popup.html") else {
            bail!("browser wake URL must target the Saccade popup");
        };
        if id.len() != 32 || !id.bytes().all(|byte| matches!(byte, b'a'..=b'p')) {
            bail!("browser wake URL has an invalid Extension identity");
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn application_name(&self) -> &'static str {
        match (self.browser_family.as_str(), self.development) {
            ("chrome", true) => "Google Chrome for Testing",
            ("chrome", false) => "Google Chrome",
            ("edge", _) => "Microsoft Edge",
            _ => unreachable!("validated browser family"),
        }
    }
}

pub fn write_route(runtime_dir: &Path, route: &BrowserWakeRoute) -> Result<()> {
    route.validate()?;
    let destination = runtime_dir.join(ROUTE_FILE);
    let temporary = runtime_dir.join(format!("{ROUTE_FILE}.installing"));
    fs::write(&temporary, serde_json::to_vec_pretty(route)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, destination)?;
    Ok(())
}

pub fn wake(runtime_dir: &Path) -> Result<()> {
    let path = runtime_dir.join(ROUTE_FILE);
    let route: BrowserWakeRoute = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    route.validate()?;
    wake_route(&route)
}

#[cfg(target_os = "macos")]
fn wake_route(route: &BrowserWakeRoute) -> Result<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-a", route.application_name(), &route.wake_url])
        .status()
        .context("wake the configured Saccade browser")?;
    if !status.success() {
        bail!("configured Saccade browser could not be woken");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn wake_route(_route: &BrowserWakeRoute) -> Result<()> {
    bail!("browser lifecycle wake is available only in the macOS Preview")
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop/popup.html";

    #[test]
    fn route_accepts_only_fixed_browser_and_extension_wake_surfaces() {
        assert!(BrowserWakeRoute::new("chrome", false, URL).is_ok());
        assert!(BrowserWakeRoute::new("edge", false, URL).is_ok());
        assert!(BrowserWakeRoute::new("safari", false, URL).is_err());
        assert!(BrowserWakeRoute::new("chrome", false, "https://example.com").is_err());
        assert!(BrowserWakeRoute::new(
            "chrome",
            false,
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/options.html"
        )
        .is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_application_selection_is_finite() {
        assert_eq!(
            BrowserWakeRoute::new("chrome", true, URL)
                .unwrap()
                .application_name(),
            "Google Chrome for Testing"
        );
        assert_eq!(
            BrowserWakeRoute::new("chrome", false, URL)
                .unwrap()
                .application_name(),
            "Google Chrome"
        );
        assert_eq!(
            BrowserWakeRoute::new("edge", true, URL)
                .unwrap()
                .application_name(),
            "Microsoft Edge"
        );
    }
}
