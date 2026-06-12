use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use url::Url;

use crate::devmax_probe::devmax_probe;

#[derive(Debug, Clone)]
pub struct BrowserSessionProfile {
    pub run_id: String,
    pub session_id: String,
    pub tab_id: String,
    pub url: String,
    pub report_path: PathBuf,
    pub replay_path: PathBuf,
    pub opened: bool,
    pub truth_collected: bool,
    pub actions_seen: usize,
    pub act_attempted: bool,
    pub act_verified: bool,
    pub same_webview: bool,
    pub page_revision_before: u64,
    pub page_revision_after: u64,
}

pub fn selftest_browser_session(
    url: Url,
    output_root: impl AsRef<Path>,
) -> Result<BrowserSessionProfile> {
    let stamp = unix_ms()?;
    let run_id = format!("session_{stamp}");
    let session_id = format!("saccade-session-{stamp}");
    let tab_id = "agent-tab-1".to_string();
    let output_dir = output_root.as_ref().join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let probe = devmax_probe(url.clone())?;
    let after_click = probe.get("afterClick").cloned().unwrap_or(Value::Null);
    let click_verification = probe
        .get("clickVerification")
        .cloned()
        .unwrap_or(Value::Null);
    let actions_seen = probe
        .get("actions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let act_attempted = click_verification
        .get("attempted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let act_verified = act_attempted
        && click_verification
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !click_verification
            .get("no_effect")
            .and_then(Value::as_bool)
            .unwrap_or(true);
    let page_revision_before = click_verification
        .get("before_page_revision")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            probe
                .get("pageRevision")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        });
    let page_revision_after = click_verification
        .get("after_page_revision")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            after_click
                .get("pageRevision")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        });
    let truth_collected = probe
        .get("engine")
        .and_then(Value::as_str)
        .is_some_and(|engine| engine.contains("servo"))
        && after_click.is_object();
    let same_webview = true;

    if !truth_collected {
        bail!("browser session smoke did not collect baseline and post-action truth");
    }
    if actions_seen == 0 {
        bail!("browser session smoke found no actions");
    }
    if !act_verified {
        bail!("browser session smoke action was not verified: {click_verification}");
    }

    let report_path = output_dir.join("report.json");
    let replay_path = output_dir.join("replay.jsonl");
    let public_click_verification = compact_click_verification(&click_verification);
    let report = json!({
        "run_id": run_id,
        "engine": "saccade-browser-session-smoke-v0",
        "url": url.to_string(),
        "session": {
            "session_id": session_id,
            "tab_id": tab_id,
            "owner": "agent",
            "same_webview": same_webview,
            "same_webview_evidence": "devmax_probe keeps one Servo WebView alive across baseline truth, native act, and verification truth",
            "phases": ["open", "truth", "actions", "act", "truth_after_act"],
        },
        "checks": {
            "opened": true,
            "truth_collected": truth_collected,
            "actions_seen": actions_seen,
            "act_attempted": act_attempted,
            "act_verified": act_verified,
            "page_revision_before": page_revision_before,
            "page_revision_after": page_revision_after,
            "page_revision_advanced": page_revision_after > page_revision_before,
        },
        "baseline": compact_probe(&probe),
        "after_action": compact_probe(&after_click),
        "click_verification": public_click_verification,
        "artifacts": {
            "report": report_path.display().to_string(),
            "replay": replay_path.display().to_string(),
        },
    });
    write_json(&report_path, &report)?;
    write_replay(
        &replay_path,
        &run_id,
        &session_id,
        &tab_id,
        &url,
        &probe,
        &after_click,
        &click_verification,
    )?;

    Ok(BrowserSessionProfile {
        run_id,
        session_id,
        tab_id,
        url: url.to_string(),
        report_path,
        replay_path,
        opened: true,
        truth_collected,
        actions_seen,
        act_attempted,
        act_verified,
        same_webview,
        page_revision_before,
        page_revision_after,
    })
}

fn compact_probe(probe: &Value) -> Value {
    let actions = probe
        .get("actions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|action| {
                    json!({
                        "index": action.get("index").cloned().unwrap_or(Value::Null),
                        "label": action.get("label").cloned().unwrap_or(Value::Null),
                        "tag": action.get("tag").cloned().unwrap_or(Value::Null),
                        "visible": action.get("visible").cloned().unwrap_or(Value::Null),
                        "disabled": action.get("disabled").cloned().unwrap_or(Value::Null),
                        "offscreen": action.get("offscreen").cloned().unwrap_or(Value::Null),
                        "blockedBy": action.get("blockedBy").cloned().unwrap_or(Value::Null),
                        "rect": action.get("rect").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "engine": probe.get("engine").cloned().unwrap_or(Value::Null),
        "title": probe.get("title").cloned().unwrap_or(Value::Null),
        "url": probe.get("url").cloned().unwrap_or(Value::Null),
        "viewport": probe.get("viewport").cloned().unwrap_or(Value::Null),
        "body_text_length": probe.get("bodyTextLength").cloned().unwrap_or(Value::Null),
        "body_child_count": probe.get("bodyChildCount").cloned().unwrap_or(Value::Null),
        "page_revision": probe.get("pageRevision").cloned().unwrap_or(Value::Null),
        "actions": actions,
        "runtime": probe.get("runtime").cloned().unwrap_or(Value::Null),
        "screenshot": probe.get("screenshot").cloned().unwrap_or(Value::Null),
    })
}

fn compact_click_verification(value: &Value) -> Value {
    json!({
        "attempted": value.get("attempted").cloned().unwrap_or(Value::Null),
        "action": value.get("action").cloned().unwrap_or(Value::Null),
        "body_text_changed": value.get("body_text_changed").cloned().unwrap_or(Value::Null),
        "url_changed": value.get("url_changed").cloned().unwrap_or(Value::Null),
        "body_child_count_changed": value.get("body_child_count_changed").cloned().unwrap_or(Value::Null),
        "page_revision_changed": value.get("page_revision_changed").cloned().unwrap_or(Value::Null),
        "before_page_revision": value.get("before_page_revision").cloned().unwrap_or(Value::Null),
        "after_page_revision": value.get("after_page_revision").cloned().unwrap_or(Value::Null),
        "changed": value.get("changed").cloned().unwrap_or(Value::Null),
        "no_effect": value.get("no_effect").cloned().unwrap_or(Value::Null),
    })
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let pretty = serde_json::to_vec_pretty(value)?;
    fs::write(path, pretty).with_context(|| format!("failed to write {}", path.display()))
}

fn write_replay(
    path: &Path,
    run_id: &str,
    session_id: &str,
    tab_id: &str,
    url: &Url,
    probe: &Value,
    after_click: &Value,
    click_verification: &Value,
) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let before_revision = click_verification
        .get("before_page_revision")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let after_revision = click_verification
        .get("after_page_revision")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    writeln!(
        file,
        "{}",
        json!({
            "kind": "browser_session_started",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "url": url.to_string(),
        })
    )?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "tab_opened",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "owner": "agent",
            "url": url.to_string(),
        })
    )?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "truth_collected",
            "phase": "baseline",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "page_revision": before_revision,
            "actions_seen": probe.get("actions").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "body_text_length": probe.get("bodyTextLength").and_then(Value::as_u64).unwrap_or(0),
        })
    )?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "action_dispatched",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "action": click_verification.get("action").cloned().unwrap_or(Value::Null),
            "native_input": "servo_mouse",
        })
    )?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "truth_collected",
            "phase": "after_action",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "page_revision": after_revision,
            "body_text_length": after_click.get("bodyTextLength").and_then(Value::as_u64).unwrap_or(0),
        })
    )?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "browser_session_finished",
            "run_id": run_id,
            "session_id": session_id,
            "tab_id": tab_id,
            "same_webview": true,
            "act_verified": click_verification.get("changed").and_then(Value::as_bool).unwrap_or(false),
            "page_revision_before": before_revision,
            "page_revision_after": after_revision,
        })
    )?;
    Ok(())
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX_EPOCH")?
        .as_millis())
}
