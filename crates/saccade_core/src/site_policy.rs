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
            side_effects_require_user: false,
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
            side_effects_require_user: false,
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
            agent_fill_allowed: true,
            agent_action_allowed: true,
            side_effects_require_user: false,
            auth_human_only: false,
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
    classify_site_url_with_owned_domains(url, &[])
}

pub fn classify_site_url_with_owned_domains(url: &str, owned_domains: &[String]) -> SitePolicy {
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
    if host_matches_any(host, &["admob.google.com", "adsense.google.com"]) {
        return SitePolicy::yellow(
            "monetization_admin",
            "Task-authorized ordinary actions are allowed; billing, payment, credentials, and account security remain user-controlled",
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
            "Task-authorized ordinary publish and messaging actions are allowed; account security, payment, and irreversible account deletion remain user-controlled",
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
    if host_matches_owned_domain(host, owned_domains) {
        return SitePolicy::green(
            "owned_domain",
            "Explicitly allowlisted owned domain; high-risk site classes still take precedence",
        );
    }

    SitePolicy::yellow(
        "unmeasured_unknown",
        "No site-specific evidence yet; complete task-authorized ordinary actions while preserving highest-risk user boundaries",
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
            "account recovery",
        ],
    ) {
        return Some("auth_human_only");
    }
    if contains_any(
        &action_text,
        &[
            "create api key",
            "rotate api key",
            "revoke api key",
            "delete api key",
            "create access key",
            "rotate secret",
            "revoke token",
            "change password",
            "reset password",
            "grant permission",
            "edit permission",
            "change owner",
            "transfer ownership",
            "add administrator",
            "remove administrator",
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
    if contains_any(
        &action_text,
        &[
            "delete account",
            "close account",
            "destroy production",
            "permanently delete",
            "irreversible delete",
            "erase all",
        ],
    ) {
        return Some("irreversible_destructive_action_requires_user");
    }
    if contains_any(
        &action_text,
        &[
            "sign agreement",
            "sign document",
            "legal signature",
            "signature",
            "attest",
            "certify",
        ],
    ) {
        return Some("legal_attestation_human_only");
    }
    if contains_any(
        &action_text,
        &[
            "deploy production",
            "production deploy",
            "release to production",
            "publish to production",
            "go live",
        ],
    ) {
        return Some("production_release_or_deployment_requires_user");
    }
    if matches!(
        site.category,
        "app_store_connect" | "cloud_or_production_admin"
    ) && contains_any(
        &action_text,
        &[
            "release",
            "deploy",
            "publish app",
            "submit for review",
            "go live",
        ],
    ) {
        return Some("production_release_or_deployment_requires_user");
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

fn host_matches_owned_domain(host: &str, owned_domains: &[String]) -> bool {
    owned_domains.iter().any(|domain| {
        let domain = normalized_host(domain.trim().trim_start_matches('.'));
        !domain.is_empty() && host_matches(host, domain)
    })
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_site_buckets() {
        let local = classify_site_url("http://127.0.0.1:4173/");
        assert_eq!(local.level, SiteRiskLevel::Green);
        assert!(!local.side_effects_require_user);

        let app_store = classify_site_url("https://appstoreconnect.apple.com/apps");
        assert_eq!(app_store.level, SiteRiskLevel::Orange);
        assert!(app_store.agent_fill_allowed);
        assert!(app_store.agent_action_allowed);
        assert!(!app_store.side_effects_require_user);
        assert!(!app_store.auth_human_only);
        assert_eq!(
            classify_site_url("https://secure.login.gov/").level,
            SiteRiskLevel::Red
        );
        assert_eq!(
            classify_site_url("https://gist.github.com/new").level,
            SiteRiskLevel::Yellow
        );
        let admob = classify_site_url("https://admob.google.com/v2/apps/1195435270/overview");
        assert_eq!(admob.level, SiteRiskLevel::Yellow);
        assert_eq!(admob.category, "monetization_admin");
        assert!(!admob.screenshots_default_allowed);

        let unknown = classify_site_url("https://example.com/workflow");
        assert_eq!(unknown.level, SiteRiskLevel::Yellow);
        assert_eq!(unknown.category, "unmeasured_unknown");
        assert!(!unknown.screenshots_default_allowed);
        assert!(!unknown.side_effects_require_user);
    }

    #[test]
    fn allows_task_authorized_ordinary_side_effects() {
        assert_eq!(
            site_action_requires_user("https://example.com", "act_submit", Some("Submit")),
            None
        );
        assert_eq!(
            site_action_requires_user("https://signpath.org/apply", "apply", Some("Apply")),
            None
        );
        assert_eq!(
            site_action_requires_user(
                "https://signpath.org/apply",
                "act_submit",
                Some("Submit application")
            ),
            None
        );
        assert_eq!(
            site_action_requires_user("https://signpath.org", "home", Some("SignPath")),
            None
        );
        assert_eq!(
            site_action_requires_user("https://example.com", "login", Some("Sign in")),
            None
        );
        assert_eq!(
            site_action_requires_user(
                "https://github.com/example/repo",
                "act_publish",
                Some("Publish")
            ),
            None
        );
        assert_eq!(
            site_action_requires_user("https://admob.google.com", "act_save", Some("Save")),
            None
        );
        assert_eq!(
            site_action_requires_user(
                "https://admob.google.com",
                "act_check_for_updates",
                Some("Check for updates")
            ),
            None
        );
    }

    #[test]
    fn blocks_only_highest_risk_actions() {
        assert_eq!(
            site_action_requires_user(
                "https://appstoreconnect.apple.com/apps",
                "act_save",
                Some("Save metadata")
            ),
            None
        );
        assert_eq!(
            site_action_requires_user(
                "https://appstoreconnect.apple.com/apps",
                "act_release",
                Some("Release")
            ),
            Some("production_release_or_deployment_requires_user")
        );
        assert_eq!(
            site_action_requires_user("https://accounts.google.com", "next", Some("Next")),
            Some("red_site_human_only")
        );
        assert_eq!(
            site_action_requires_user("https://example.com", "act_pay", Some("Pay now")),
            Some("payment_or_financial_action_requires_user")
        );
        assert_eq!(
            site_action_requires_user(
                "https://example.com",
                "act_attest",
                Some("Sign legal attestation")
            ),
            Some("legal_attestation_human_only")
        );
        assert_eq!(
            site_action_requires_user(
                "https://example.com",
                "act_delete_account",
                Some("Permanently delete account")
            ),
            Some("irreversible_destructive_action_requires_user")
        );
        assert_eq!(
            site_action_requires_user(
                "https://example.com",
                "act_deploy",
                Some("Deploy production")
            ),
            Some("production_release_or_deployment_requires_user")
        );
        assert_eq!(
            site_action_requires_user("https://example.com", "nav_security", Some("Security")),
            None
        );
        assert_eq!(
            site_action_requires_user("http://localhost:3000", "act_primary", Some("Preview")),
            None
        );
    }

    #[test]
    fn owned_domains_are_green_without_overriding_high_risk() {
        let owned = vec!["nanmesh.ai".to_string(), "accounts.google.com".to_string()];
        let owned_policy =
            classify_site_url_with_owned_domains("https://app.nanmesh.ai/demo", &owned);
        assert_eq!(owned_policy.level, SiteRiskLevel::Green);
        assert_eq!(owned_policy.category, "owned_domain");

        let auth_policy =
            classify_site_url_with_owned_domains("https://accounts.google.com/signin", &owned);
        assert_eq!(auth_policy.level, SiteRiskLevel::Red);
        assert_eq!(auth_policy.category, "auth_identity");
    }
}
