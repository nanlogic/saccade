use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRiskLevel {
    Green,
    Yellow,
    Orange,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePolicy {
    pub level: SiteRiskLevel,
    pub category: &'static str,
    pub reason: &'static str,
    pub agent_read_allowed: bool,
    pub agent_fill_allowed: bool,
    pub agent_action_allowed: bool,
    pub side_effects_require_user: bool,
    pub auth_human_only: bool,
    pub screenshots_default_allowed: bool,
}

impl SitePolicy {
    const fn green(category: &'static str, reason: &'static str) -> Self {
        Self {
            level: SiteRiskLevel::Green,
            category,
            reason,
            agent_read_allowed: true,
            agent_fill_allowed: true,
            agent_action_allowed: true,
            side_effects_require_user: true,
            auth_human_only: false,
            screenshots_default_allowed: true,
        }
    }

    const fn yellow(category: &'static str, reason: &'static str) -> Self {
        Self {
            level: SiteRiskLevel::Yellow,
            category,
            reason,
            agent_read_allowed: true,
            agent_fill_allowed: true,
            agent_action_allowed: true,
            side_effects_require_user: true,
            auth_human_only: false,
            screenshots_default_allowed: false,
        }
    }

    const fn orange(category: &'static str, reason: &'static str) -> Self {
        Self {
            level: SiteRiskLevel::Orange,
            category,
            reason,
            agent_read_allowed: true,
            agent_fill_allowed: false,
            agent_action_allowed: false,
            side_effects_require_user: true,
            auth_human_only: true,
            screenshots_default_allowed: false,
        }
    }

    const fn red(category: &'static str, reason: &'static str) -> Self {
        Self {
            level: SiteRiskLevel::Red,
            category,
            reason,
            agent_read_allowed: false,
            agent_fill_allowed: false,
            agent_action_allowed: false,
            side_effects_require_user: true,
            auth_human_only: true,
            screenshots_default_allowed: false,
        }
    }
}

pub fn classify_site_url(url: &str) -> SitePolicy {
    let Ok(parsed) = Url::parse(url) else {
        return SitePolicy::yellow(
            "unknown_url",
            "URL could not be parsed; require human review",
        );
    };
    match parsed.scheme() {
        "file" | "about" => {
            return SitePolicy::green(
                "local_or_browser_page",
                "Local/browser pages are owned test surfaces",
            );
        }
        "http" | "https" => {}
        _ => {
            return SitePolicy::yellow(
                "non_web_scheme",
                "Non-HTTP URL schemes need explicit human review",
            );
        }
    }

    let Some(host) = parsed.host_str().map(normalized_host) else {
        return SitePolicy::yellow("missing_host", "URL has no host; require human review");
    };

    if is_loopback_host(host) {
        return SitePolicy::green("local_dev", "Loopback/local development target");
    }
    if host_matches(host, "login.gov")
        || host_matches(host, "id.me")
        || host_matches(host, "accounts.google.com")
        || host_matches(host, "login.microsoftonline.com")
        || host_matches(host, "appleid.apple.com")
        || host_matches(host, "idmsa.apple.com")
    {
        return SitePolicy::red(
            "auth_identity",
            "Authentication and identity proofing are human-only",
        );
    }
    if host_matches(host, "appstoreconnect.apple.com") {
        return SitePolicy::orange(
            "app_store_connect",
            "App management, review, agreements, and financial settings are high-impact",
        );
    }
    if host_matches(host, "irs.gov")
        || host_matches(host, "ssa.gov")
        || host_matches(host, "uscis.gov")
        || host_matches(host, "dmv.ca.gov")
    {
        return SitePolicy::orange(
            "government_account_or_form",
            "Government account/forms can involve identity, benefits, tax, legal, or payment impact",
        );
    }
    if host.ends_with(".gov") {
        return SitePolicy::yellow(
            "government_public",
            "Government public pages are readable; accounts/forms need human review",
        );
    }
    if host_matches_any(
        host,
        &[
            "paypal.com",
            "stripe.com",
            "coinbase.com",
            "robinhood.com",
            "chase.com",
            "bankofamerica.com",
            "wellsfargo.com",
            "capitalone.com",
            "americanexpress.com",
            "venmo.com",
            "cash.app",
        ],
    ) {
        return SitePolicy::orange(
            "financial",
            "Financial accounts, transactions, payments, trades, and withdrawals are high-risk",
        );
    }
    if host_matches_any(
        host,
        &[
            "console.aws.amazon.com",
            "portal.azure.com",
            "console.cloud.google.com",
            "cloud.google.com",
            "cloudflare.com",
            "vercel.com",
            "netlify.com",
            "render.com",
        ],
    ) {
        return SitePolicy::orange(
            "cloud_or_production_admin",
            "Cloud/admin consoles may change production, billing, IAM, or credentials",
        );
    }
    if host.contains("mychart") || host_matches_any(host, &["kp.org", "healthcare.gov"]) {
        return SitePolicy::orange(
            "healthcare",
            "Healthcare portals can expose PHI, insurance, billing, and treatment decisions",
        );
    }
    if host_matches(host, "docs.github.com")
        || host_matches(host, "developer.mozilla.org")
        || host_matches(host, "w3.org")
    {
        return SitePolicy::green(
            "public_documentation",
            "Public documentation/reference page",
        );
    }
    if host_matches_any(
        host,
        &[
            "github.com",
            "gist.github.com",
            "reddit.com",
            "linkedin.com",
            "x.com",
            "twitter.com",
            "facebook.com",
            "instagram.com",
            "youtube.com",
            "tiktok.com",
        ],
    ) {
        return SitePolicy::yellow(
            "account_reputation_or_social",
            "Drafting is okay; publish, delete, mass message, or reputation actions need confirmation",
        );
    }
    if host_matches_any(
        host,
        &[
            "amazon.com",
            "ebay.com",
            "etsy.com",
            "target.com",
            "walmart.com",
            "booking.com",
            "expedia.com",
            "airbnb.com",
        ],
    ) {
        return SitePolicy::yellow(
            "shopping_or_travel",
            "Research is okay; checkout, booking, payment, cancellation, or refund need user action",
        );
    }

    SitePolicy::green(
        "public_or_unknown_low_risk",
        "No high-risk site pattern matched",
    )
}

pub fn site_action_requires_user(
    site_url: &str,
    action_id: &str,
    label: Option<&str>,
) -> Option<&'static str> {
    let site = classify_site_url(site_url);
    action_requires_user(&site, action_id, label)
}

fn action_requires_user(
    site: &SitePolicy,
    action_id: &str,
    label: Option<&str>,
) -> Option<&'static str> {
    if site.level == SiteRiskLevel::Red {
        return Some("red_site_human_only");
    }

    let action_text = format!("{} {}", action_id, label.unwrap_or_default()).to_lowercase();
    if contains_any(
        &action_text,
        &[
            "captcha",
            "recaptcha",
            "hcaptcha",
            "2fa",
            "mfa",
            "otp",
            "one-time",
            "passkey",
            "password",
            "sign in",
            "signin",
            "login",
            "log in",
            "account recovery",
        ],
    ) {
        return Some("auth_human_only");
    }
    if contains_any(
        &action_text,
        &[
            "api key",
            "token",
            "secret",
            "credential",
            "security",
            "iam",
            "permission",
            "access key",
        ],
    ) {
        return Some("security_or_credential_change_requires_user");
    }
    if contains_any(
        &action_text,
        &[
            "purchase",
            "payment",
            "pay",
            "checkout",
            "transfer",
            "trade",
            "withdraw",
            "refund",
            "subscribe",
            "billing",
        ],
    ) {
        return Some("payment_or_financial_action_requires_user");
    }
    if contains_any(&action_text, &["delete", "remove", "destroy", "cancel"]) {
        return Some("destructive_action_confirmation_required");
    }
    if contains_any(&action_text, &["sign", "signature", "attest", "certify"]) {
        return Some("legal_attestation_human_only");
    }
    if contains_any(
        &action_text,
        &[
            "submit",
            "publish",
            "post",
            "send",
            "release",
            "deploy",
            "confirm",
            "export",
            "act_submit",
            "act_export",
        ],
    ) {
        if matches!(site.level, SiteRiskLevel::Orange) {
            return Some("high_risk_site_side_effect_requires_user");
        }
        return Some("side_effect_confirmation_required");
    }

    None
}

fn normalized_host(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1") || host.starts_with("127.")
}

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn host_matches_any(host: &str, domains: &[&str]) -> bool {
    domains.iter().any(|domain| host_matches(host, domain))
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_site_buckets() {
        assert_eq!(
            classify_site_url("http://127.0.0.1:4173/").level,
            SiteRiskLevel::Green
        );
        assert_eq!(
            classify_site_url("https://appstoreconnect.apple.com/apps").level,
            SiteRiskLevel::Orange
        );
        assert_eq!(
            classify_site_url("https://secure.login.gov/").level,
            SiteRiskLevel::Red
        );
        assert_eq!(
            classify_site_url("https://gist.github.com/new").level,
            SiteRiskLevel::Yellow
        );
    }

    #[test]
    fn blocks_high_risk_actions() {
        assert_eq!(
            site_action_requires_user("https://example.com", "act_submit", Some("Submit")),
            Some("side_effect_confirmation_required")
        );
        assert_eq!(
            site_action_requires_user(
                "https://appstoreconnect.apple.com/apps",
                "act_release",
                Some("Release")
            ),
            Some("high_risk_site_side_effect_requires_user")
        );
        assert_eq!(
            site_action_requires_user("https://accounts.google.com", "next", Some("Next")),
            Some("red_site_human_only")
        );
        assert_eq!(
            site_action_requires_user("http://localhost:3000", "act_primary", Some("Preview")),
            None
        );
    }
}
