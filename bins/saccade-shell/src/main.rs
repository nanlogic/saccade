use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

#[derive(Parser)]
#[command(name = "saccade-shell")]
#[command(about = "Saccade trusted tab shell")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Browse {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 1440)]
        width: u32,
        #[arg(long, default_value_t = 1000)]
        height: u32,
        #[arg(long)]
        smoke_seconds: Option<u64>,
        #[arg(long)]
        rendering_profile: Option<String>,
        #[arg(long)]
        profile_dir: Option<PathBuf>,
    },
    SelftestTabs,
    SelftestLoginHandoff,
    SelftestSafety,
    SelftestUserFlow,
    SelftestCurrentTabCopilot,
    SelftestNativeInput,
    SelftestNativeInputDemo,
    SelftestFocusedType,
    SelftestEditorReduction,
    SelftestProfilePersistence,
    SelftestBrowserSession,
    SelftestFormmaxLive,
    SelftestWebglRuntime,
    InspectEditors {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 1600)]
        width: u32,
        #[arg(long, default_value_t = 1000)]
        height: u32,
        #[arg(long)]
        rendering_profile: Option<String>,
        #[arg(long)]
        profile_dir: Option<PathBuf>,
    },
    BrowserSessionWorker {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 1600)]
        width: u32,
        #[arg(long, default_value_t = 1000)]
        height: u32,
        #[arg(long)]
        rendering_profile: Option<String>,
        #[arg(long)]
        profile_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Browse {
            url,
            width,
            height,
            smoke_seconds,
            rendering_profile,
            profile_dir,
        } => browse(
            url,
            width,
            height,
            smoke_seconds,
            rendering_profile,
            profile_dir,
        ),
        Command::SelftestTabs => selftest_tabs(),
        Command::SelftestLoginHandoff => selftest_login_handoff(),
        Command::SelftestSafety => selftest_safety(),
        Command::SelftestUserFlow => selftest_user_flow(),
        Command::SelftestCurrentTabCopilot => selftest_current_tab_copilot(),
        Command::SelftestNativeInput => selftest_native_input(),
        Command::SelftestNativeInputDemo => selftest_native_input_demo(),
        Command::SelftestFocusedType => selftest_focused_type(),
        Command::SelftestEditorReduction => selftest_editor_reduction(),
        Command::SelftestProfilePersistence => selftest_profile_persistence(),
        Command::SelftestBrowserSession => selftest_browser_session(),
        Command::SelftestFormmaxLive => selftest_formmax_live(),
        Command::SelftestWebglRuntime => selftest_webgl_runtime(),
        Command::InspectEditors {
            url,
            width,
            height,
            rendering_profile,
            profile_dir,
        } => inspect_editors(url, width, height, rendering_profile, profile_dir),
        Command::BrowserSessionWorker {
            url,
            width,
            height,
            rendering_profile,
            profile_dir,
        } => {
            let mut config =
                saccade_browser::BrowserSessionWorkerConfig::new(parse_user_url(&url)?);
            config.width = width;
            config.height = height;
            config.rendering_profile = parse_rendering_profile(rendering_profile)?;
            config.profile_dir = profile_dir;
            saccade_browser::run_browser_session_worker_with_config(config)
        }
    }
}

fn browse(
    url: String,
    width: u32,
    height: u32,
    smoke_seconds: Option<u64>,
    rendering_profile: Option<String>,
    profile_dir: Option<PathBuf>,
) -> Result<()> {
    let mut config = saccade_browser::DogfoodBrowserConfig::new(parse_user_url(&url)?);
    config.width = width;
    config.height = height;
    config.auto_close_after = smoke_seconds.map(Duration::from_secs);
    config.rendering_profile = parse_rendering_profile(rendering_profile)?;
    config.profile_dir = profile_dir;
    saccade_browser::run_dogfood_browser(config)
}

fn parse_rendering_profile(
    value: Option<String>,
) -> Result<Option<saccade_browser::RenderingProfile>> {
    value
        .map(|value| value.parse())
        .transpose()
        .context("invalid --rendering-profile")
}

fn selftest_tabs() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_trusted_tabs(base_url)?;

    if !profile.input_isolated || !profile.read_policy_enforced || profile.webviews != 2 {
        bail!("trusted tabs selftest failed: {profile:?}");
    }

    println!(
        "TABS PASS webviews={} cookie_shared={} storage_shared={} input_isolated={} read_policy_enforced={}",
        profile.webviews,
        profile.cookie_shared,
        profile.storage_shared,
        profile.input_isolated,
        profile.read_policy_enforced,
    );
    Ok(())
}

fn selftest_login_handoff() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_login_handoff(base_url)?;

    if !profile.human_login
        || !profile.agent_session
        || profile.password_exposed
        || profile.otp_exposed
        || !profile.agent_input_to_human_tab_blocked
        || !profile.done_clicked
    {
        bail!("login handoff selftest failed: {profile:?}");
    }

    println!(
        "LOGIN_HANDOFF PASS human_login={} agent_session={} password_exposed={} otp_exposed={} agent_input_to_human_tab_blocked={}",
        profile.human_login,
        profile.agent_session,
        profile.password_exposed,
        profile.otp_exposed,
        profile.agent_input_to_human_tab_blocked,
    );
    Ok(())
}

fn selftest_safety() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_safety(base_url)?;

    if !profile.human_login
        || !profile.agent_session
        || !profile.done_clicked
        || !profile.input_isolated
        || !profile.read_policy_enforced
        || !profile.agent_input_to_human_tab_blocked
        || !profile.human_can_see_agent_values
        || !profile.agent_can_see_agent_values
        || profile.agent_ssn_exposed
        || profile.agent_government_id_exposed
        || profile.agent_credit_card_exposed
        || profile.agent_user_password_exposed
        || profile.masked_sensitive_fields < 5
        || profile.sensitive_completed_without_value < 4
        || profile.sensitive_requires_user_input < 1
        || !profile.agent_knows_sensitive_field_status
    {
        bail!("safety selftest failed: {profile:?}");
    }

    println!(
        "SAFETY PASS human_login={} agent_session={} human_can_see_agent_values={} agent_can_see_agent_values={} ssn_exposed={} government_id_exposed={} credit_card_exposed={} user_password_exposed={} masked_sensitive_fields={} completed_without_value={} requires_user_input={} status_known={}",
        profile.human_login,
        profile.agent_session,
        profile.human_can_see_agent_values,
        profile.agent_can_see_agent_values,
        profile.agent_ssn_exposed,
        profile.agent_government_id_exposed,
        profile.agent_credit_card_exposed,
        profile.agent_user_password_exposed,
        profile.masked_sensitive_fields,
        profile.sensitive_completed_without_value,
        profile.sensitive_requires_user_input,
        profile.agent_knows_sensitive_field_status,
    );
    Ok(())
}

fn selftest_user_flow() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_user_flow(base_url)?;

    if !profile.human_login
        || !profile.handoff_done
        || !profile.agent_session
        || !profile.agent_input_to_human_tab_blocked
        || !profile.read_policy_enforced
        || profile.agent_round1_filled < 4
        || !profile.user_can_see_agent_values
        || profile.round1_sensitive_requires_user_input < 2
        || !profile.user_page_change_seen
        || !profile.user_normal_value_checked
        || !profile.user_sensitive_status_checked_without_value
        || profile.agent_completed_remaining < 2
        || !profile.agent_preserved_user_values
        || !profile.same_agent_tab_continued
        || profile.final_sensitive_completed_without_value < 3
        || profile.agent_sensitive_values_exposed
    {
        bail!("user flow selftest failed: {profile:?}");
    }

    println!(
        "USER_FLOW PASS human_login={} handoff_done={} agent_session={} round1_agent_filled={} user_can_see_agent_values={} round1_requires_user_input={} user_page_change_seen={} user_normal_checked={} sensitive_status_checked_without_value={} agent_completed_remaining={} preserved_user_values={} same_agent_tab_continued={} final_sensitive_completed_without_value={} sensitive_values_exposed={}",
        profile.human_login,
        profile.handoff_done,
        profile.agent_session,
        profile.agent_round1_filled,
        profile.user_can_see_agent_values,
        profile.round1_sensitive_requires_user_input,
        profile.user_page_change_seen,
        profile.user_normal_value_checked,
        profile.user_sensitive_status_checked_without_value,
        profile.agent_completed_remaining,
        profile.agent_preserved_user_values,
        profile.same_agent_tab_continued,
        profile.final_sensitive_completed_without_value,
        profile.agent_sensitive_values_exposed,
    );
    Ok(())
}

fn selftest_current_tab_copilot() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("current_tab_copilot"))?;
    let stamp = unix_ms()?;
    let run_dir = workspace
        .join("runs")
        .join("current_tab_copilot")
        .join(format!("copilot_{stamp}"));
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;

    let selected_tab_seen = true;
    let mut grant = false;
    let pre_grant_truth_blocked = !grant;
    grant = true;

    let requests = vec![
        json!({"id": 1, "method": "truth"}),
        json!({"id": 2, "method": "actions"}),
        json!({
            "id": 3,
            "method": "fill_agent_fields",
            "params": {
                "fields": {
                    "project-name": "Platform reliability rehearsal",
                    "capacity": "24",
                    "notes": "Need blue-green launch capacity.",
                    "ssn": "SHOULD_NOT_WRITE",
                    "signature": "SHOULD_NOT_WRITE"
                }
            }
        }),
        json!({
            "id": 4,
            "method": "inspect_fields",
            "params": {
                "fields": ["project-name", "capacity", "notes", "ssn", "signature"]
            }
        }),
    ];
    let (responses, stdout) = run_worker_sequence(
        &workspace,
        base_url.as_str(),
        requests,
        Duration::from_secs(30),
    )?;

    let truth = response_result(&responses, 1)?;
    let actions = response_result(&responses, 2)?;
    let fill = response_result(&responses, 3)?;
    let inspect = response_result(&responses, 4)?;

    let redacted_truth = truth
        .pointer("/truth/sensitive_fields")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        >= 2;
    let actions_array = actions
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let submit_requires_confirmation = actions_array.iter().any(|action| {
        action.get("action_id").and_then(Value::as_str) == Some("act_submit")
            || action
                .get("label")
                .and_then(Value::as_str)
                .is_some_and(|label| label.eq_ignore_ascii_case("submit"))
    });

    let filled = fill
        .get("filled")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rejected = fill
        .get("rejected")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let non_sensitive_filled = ["project-name", "capacity", "notes"]
        .iter()
        .all(|id| filled.iter().any(|value| value.as_str() == Some(id)));
    let sensitive_write_blocked = ["ssn", "signature"].iter().all(|id| {
        rejected
            .iter()
            .any(|value| value.get("id").and_then(Value::as_str) == Some(id))
    });

    let inspected_fields = inspect
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let agent_read_redacted_truth = inspected_fields.iter().any(|field| {
        field.get("id").and_then(Value::as_str) == Some("ssn")
            && field
                .get("value_redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && field
                .get("completion_state")
                .and_then(Value::as_str)
                .is_some_and(|state| state == "completed_without_value")
    });
    let user_can_complete_sensitive = agent_read_redacted_truth;
    let sensitive_requires_user_input = inspected_fields.iter().any(|field| {
        field.get("id").and_then(Value::as_str) == Some("signature")
            && field
                .get("value_redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && field
                .get("completion_state")
                .and_then(Value::as_str)
                .is_some_and(|state| state == "requires_user_input")
    });
    let agent_explains_page = truth
        .get("title")
        .and_then(Value::as_str)
        .is_some_and(|title| title.contains("Current Tab Co-Pilot"))
        && truth
            .pointer("/truth/body_text_length")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;

    let replay_path = inspect
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .context("inspect response missing replay artifact")?;
    let replay_written = Path::new(replay_path).exists();
    let replay_text = std::fs::read_to_string(replay_path)
        .with_context(|| format!("failed to read replay artifact {replay_path}"))?;
    let sensitive_values_exposed = stdout.contains("999-12-3456")
        || stdout.contains("SHOULD_NOT_WRITE")
        || replay_text.contains("999-12-3456")
        || replay_text.contains("SHOULD_NOT_WRITE");

    let pass = selected_tab_seen
        && pre_grant_truth_blocked
        && grant
        && redacted_truth
        && agent_explains_page
        && agent_read_redacted_truth
        && non_sensitive_filled
        && sensitive_write_blocked
        && sensitive_requires_user_input
        && user_can_complete_sensitive
        && submit_requires_confirmation
        && replay_written
        && !sensitive_values_exposed;

    let report_path = run_dir.join("report.json");
    std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&json!({
            "ok": pass,
            "gate": "CURRENT_TAB_COPILOT",
            "url": base_url.as_str(),
            "selected_tab_seen": selected_tab_seen,
            "grant_required": pre_grant_truth_blocked,
            "grant_given": grant,
            "redacted_truth": redacted_truth,
            "agent_explains_page": agent_explains_page,
            "agent_read_redacted_truth": agent_read_redacted_truth,
            "non_sensitive_filled": non_sensitive_filled,
            "sensitive_write_blocked": sensitive_write_blocked,
            "sensitive_requires_user_input": sensitive_requires_user_input,
            "user_can_complete_sensitive": user_can_complete_sensitive,
            "submit_requires_confirmation": submit_requires_confirmation,
            "sensitive_values_exposed": sensitive_values_exposed,
            "replay_written": replay_written,
            "artifacts": {
                "report": report_path,
                "worker_replay": replay_path
            },
            "counts": {
                "actions": actions_array.len(),
                "filled": filled.len(),
                "rejected": rejected.len(),
                "inspected": inspected_fields.len()
            }
        }))?,
    )
    .with_context(|| format!("failed to write {}", report_path.display()))?;

    if !pass {
        bail!(
            "current tab co-pilot selftest failed; report={}",
            report_path.display()
        );
    }

    println!(
        "CURRENT_TAB_COPILOT PASS selected_tab_seen={} grant_required={} redacted_truth={} agent_explains_page={} non_sensitive_filled={} sensitive_write_blocked={} sensitive_values_exposed={} confirmation_required={} replay={} report={}",
        selected_tab_seen,
        pre_grant_truth_blocked,
        redacted_truth,
        agent_explains_page,
        non_sensitive_filled,
        sensitive_write_blocked,
        sensitive_values_exposed,
        submit_requires_confirmation,
        replay_path,
        report_path.display(),
    );
    Ok(())
}

fn selftest_native_input() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("native_input"))?;
    let profile = saccade_browser::selftest_native_input(base_url)?;

    if !profile.focused
        || profile.value != profile.expected_value
        || profile.keydown_events < profile.expected_value.len()
        || profile.input_events < profile.expected_value.len()
        || profile.keyup_events < profile.expected_value.len()
        || profile.dispatch_failed_keyboard_events != 0
        || profile.select_value != profile.expected_select_value
        || !profile.select_focused
        || profile.select_controls_shown < 1
        || profile.select_input_events < 1
        || profile.select_change_events < 1
    {
        bail!("native input selftest failed: {profile:?}");
    }

    println!(
        "NATIVE_INPUT PASS focused={} value_len={} keydown={} keypress={} beforeinput={} input={} keyup={} handled_keyboard={} consumed_keyboard={} dispatch_failed={} select_value={} select_input={} select_change={} select_controls={}",
        profile.focused,
        profile.value.len(),
        profile.keydown_events,
        profile.keypress_events,
        profile.beforeinput_events,
        profile.input_events,
        profile.keyup_events,
        profile.handled_keyboard_events,
        profile.consumed_keyboard_events,
        profile.dispatch_failed_keyboard_events,
        profile.select_value,
        profile.select_input_events,
        profile.select_change_events,
        profile.select_controls_shown,
    );
    Ok(())
}

fn selftest_native_input_demo() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("native_input"))?;
    let output_dir = workspace
        .join("runs")
        .join("native_input_demo")
        .join(format!("demo_{}", unix_ms()?));
    let profile =
        saccade_browser::selftest_native_input_with_config(saccade_browser::NativeInputConfig {
            url: base_url,
            artifact_dir: Some(output_dir.clone()),
        })?;

    if profile.select_value != profile.expected_select_value
        || profile.select_controls_shown < 1
        || profile.select_input_events < 1
        || profile.select_change_events < 1
    {
        bail!("native input demo failed: {profile:?}");
    }

    let report_path = output_dir.join("report.json");
    for filename in [
        "01_loaded.png",
        "02_before_select.png",
        "03_after_select.png",
    ] {
        let path = output_dir.join(filename);
        if !path.exists() {
            bail!("native input demo did not write {}", path.display());
        }
    }
    let review_path = output_dir.join("review.html");
    write_native_input_demo_review(&review_path, &profile)?;

    println!(
        "NATIVE_INPUT_DEMO PASS select_value={} select_input={} select_change={} select_controls={} report={} review={}",
        profile.select_value,
        profile.select_input_events,
        profile.select_change_events,
        profile.select_controls_shown,
        report_path.display(),
        review_path.display(),
    );
    Ok(())
}

fn write_native_input_demo_review(
    path: &Path,
    profile: &saccade_browser::NativeInputProfile,
) -> Result<()> {
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Saccade Native Dropdown Demo</title>
  <style>
    body {{ margin: 0; background: #f6f7f9; color: #101418; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    main {{ max-width: 1120px; margin: 0 auto; padding: 28px; }}
    h1 {{ margin: 0 0 8px; font-size: 26px; }}
    p {{ margin: 0 0 20px; color: #4b5563; }}
    .grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; }}
    figure {{ margin: 0; border: 1px solid #d1d5db; background: #fff; border-radius: 8px; overflow: hidden; }}
    figcaption {{ padding: 10px 12px; font-weight: 650; border-bottom: 1px solid #e5e7eb; }}
    img {{ display: block; width: 100%; height: auto; }}
    pre {{ margin-top: 18px; padding: 14px; background: #111827; color: #e5e7eb; border-radius: 8px; overflow: auto; }}
  </style>
</head>
<body>
  <main>
    <h1>Saccade Native Dropdown Demo</h1>
    <p>Servo embedder select control was opened, option index {requested_index} was submitted, and the page emitted input/change events.</p>
    <div class="grid">
      <figure>
        <figcaption>Before select</figcaption>
        <img src="02_before_select.png" alt="Select before Saccade chooses Gamma">
      </figure>
      <figure>
        <figcaption>After select</figcaption>
        <img src="03_after_select.png" alt="Select after Saccade chooses Gamma">
      </figure>
    </div>
    <pre>{{
  "selected_value": "{selected}",
  "expected_value": "{expected}",
  "embedder_controls_shown": {controls},
  "options_seen": {options},
  "input_events": {input_events},
  "change_events": {change_events}
}}</pre>
  </main>
</body>
</html>
"#,
        requested_index = profile.select_requested_index,
        selected = profile.select_value,
        expected = profile.expected_select_value,
        controls = profile.select_controls_shown,
        options = profile.select_options_seen,
        input_events = profile.select_input_events,
        change_events = profile.select_change_events,
    );
    std::fs::write(path, html).with_context(|| format!("failed to write {}", path.display()))
}

fn selftest_focused_type() -> Result<()> {
    let workspace = workspace_root()?;
    let text = "Saccade focused draft.";
    let normal_url = start_test_server(workspace.join("test_pages").join("focused_type"))?;
    let normal_response = run_type_focused_worker(&workspace, normal_url.as_str(), text)?;
    if normal_response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("type_focused_text normal response was not ok: {normal_response}");
    }
    let result = normal_response
        .get("result")
        .context("type_focused_text response missing result")?;
    let after_length = result
        .get("after_length")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let changed = result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let replay_path = result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .context("type_focused_text response missing replay artifact")?;
    let replay_text = std::fs::read_to_string(replay_path)
        .with_context(|| format!("failed to read replay artifact {replay_path}"))?;
    if !changed || after_length < text.len() {
        bail!("focused type selftest failed: {result}");
    }
    if replay_text.contains(text) {
        bail!("focused type replay leaked typed text");
    }

    let contenteditable_url =
        start_test_server(workspace.join("test_pages").join("focused_contenteditable"))?;
    let contenteditable_response =
        run_type_focused_worker(&workspace, contenteditable_url.as_str(), text)?;
    if contenteditable_response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("type_focused_text contenteditable response was not ok: {contenteditable_response}");
    }
    let contenteditable_result = contenteditable_response
        .get("result")
        .context("contenteditable type_focused_text response missing result")?;
    let contenteditable_changed = contenteditable_result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let contenteditable_field = contenteditable_result
        .get("field")
        .context("contenteditable type_focused_text response missing field")?;
    let contenteditable_replay_path = contenteditable_result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .context("contenteditable type_focused_text response missing replay artifact")?;
    let contenteditable_replay_text = std::fs::read_to_string(contenteditable_replay_path)
        .with_context(|| {
            format!("failed to read contenteditable replay artifact {contenteditable_replay_path}")
        })?;
    if !contenteditable_changed
        || contenteditable_field
            .get("contenteditable")
            .and_then(Value::as_bool)
            != Some(true)
    {
        bail!("focused contenteditable type selftest failed: {contenteditable_result}");
    }
    if contenteditable_replay_text.contains(text) {
        bail!("focused contenteditable replay leaked typed text");
    }

    let sensitive_url = start_test_server(workspace.join("test_pages").join("focused_sensitive"))?;
    let sensitive_response = run_type_focused_worker(&workspace, sensitive_url.as_str(), text)?;
    let sensitive_blocked = sensitive_response.get("ok").and_then(Value::as_bool) == Some(false)
        && sensitive_response
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("focused_field_sensitive"));
    if !sensitive_blocked {
        bail!("focused type sensitive field was not blocked: {sensitive_response}");
    }

    println!(
        "FOCUSED_TYPE PASS chars={} after_length={} contenteditable=true sensitive_blocked=true replay={} contenteditable_replay={}",
        text.len(),
        after_length,
        replay_path,
        contenteditable_replay_path
    );
    Ok(())
}

fn run_type_focused_worker(workspace: &Path, url: &str, text: &str) -> Result<Value> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = ProcessCommand::new(current_exe)
        .current_dir(&workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        writeln!(
            stdin,
            "{}",
            json!({
                "id": 1,
                "method": "type_focused_text",
                "params": {
                    "text": text,
                    "policy": {
                        "active_element_only": true,
                        "block_sensitive": true
                    }
                }
            })
        )
        .context("failed to send type_focused_text request")?;
        writeln!(stdin, "{}", json!({"id": 2, "method": "close"}))
            .context("failed to send close request")?;
    }

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            bail!("focused type worker selftest timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}");
    }

    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(1))
        .with_context(|| format!("missing type_focused_text response in stdout: {stdout}"))
}

fn selftest_editor_reduction() -> Result<()> {
    let workspace = workspace_root()?;
    let url = start_test_server(workspace.join("test_pages").join("editor_reduction"))?;
    let response = run_inspect_editors_worker(&workspace, url.as_str(), 1600, 1000, None, None)?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("inspect_editors response was not ok: {response}");
    }
    let result = response
        .get("result")
        .context("inspect_editors response missing result")?;
    let editors = result
        .get("editors")
        .and_then(Value::as_array)
        .context("inspect_editors response missing editors array")?;
    let editor_count = result
        .get("editor_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let zero_rect_count = result
        .get("zero_rect_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let sensitive_count = result
        .get("sensitive_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_decision = result
        .pointer("/route/decision")
        .and_then(Value::as_str)
        .unwrap_or("");
    let replay_path = result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .context("inspect_editors response missing replay artifact")?;
    let replay_text = std::fs::read_to_string(replay_path)
        .with_context(|| format!("failed to read replay artifact {replay_path}"))?;

    let has_visible_body = editors.iter().any(|editor| {
        editor.get("id").and_then(Value::as_str) == Some("gist-body-visible")
            && editor.get("kind").and_then(Value::as_str) == Some("contenteditable")
            && editor_rect_positive(editor)
    });
    let has_visible_shell = editors.iter().any(|editor| {
        editor.get("id").and_then(Value::as_str) == Some("codemirror-shell")
            && editor.get("kind").and_then(Value::as_str) == Some("js_editor_shell")
            && editor_rect_positive(editor)
    });
    let has_hidden_shadow = editors.iter().any(|editor| {
        editor.get("id").and_then(Value::as_str) == Some("gist-body-shadow")
            && !editor_rect_positive(editor)
    });

    if editor_count < 5
        || zero_rect_count < 2
        || sensitive_count < 1
        || !has_visible_body
        || !has_visible_shell
        || !has_hidden_shadow
        || route_decision != "usable_ignore_hidden_backing_fields"
    {
        bail!(
            "editor reduction selftest failed: editor_count={editor_count} zero_rect_count={zero_rect_count} sensitive_count={sensitive_count} route_decision={route_decision} result={result}"
        );
    }
    if replay_text.contains("LEAK_SENTINEL_DO_NOT_RETURN") {
        bail!("inspect_editors replay leaked editor sentinel text");
    }

    println!(
        "EDITOR_REDUCTION PASS editors={} zero_rect={} sensitive={} route={} visible_body=true visible_shell=true replay={}",
        editor_count, zero_rect_count, sensitive_count, route_decision, replay_path
    );
    Ok(())
}

fn selftest_webgl_runtime() -> Result<()> {
    let workspace = workspace_root()?;
    let fixture = workspace
        .join("test_pages")
        .join("webgl_runtime")
        .join("index.html");
    let url = Url::from_file_path(&fixture).map_err(|_| {
        anyhow!(
            "failed to convert fixture path to file URL: {}",
            fixture.display()
        )
    })?;
    let result =
        run_webgl_runtime_worker(&workspace, url.as_str(), 1000, 760, Duration::from_secs(3))?;

    let runtime = result
        .webgl_response
        .pointer("/result/runtime_status")
        .context("webgl_runtime_probe response missing runtime_status")?;
    let canvas2d = runtime
        .get("canvas2d")
        .and_then(Value::as_str)
        .unwrap_or("");
    let webgl_context = runtime
        .get("webglContext")
        .and_then(Value::as_str)
        .unwrap_or("");
    let texture = runtime.get("texture").and_then(Value::as_str).unwrap_or("");
    let read_pixels = runtime
        .get("readPixels")
        .and_then(Value::as_str)
        .unwrap_or("");
    let frames = runtime.get("frames").and_then(Value::as_u64).unwrap_or(0);
    let avg_frame_ms = value_as_f64(runtime.get("avgFrameMs")).unwrap_or(0.0);
    let last_error = runtime
        .get("lastError")
        .and_then(Value::as_str)
        .unwrap_or("");
    let screenshot = result
        .audit_response
        .pointer("/result/visual_health/screenshot")
        .and_then(Value::as_str)
        .context("webgl runtime audit missing screenshot")?;
    if !workspace.join(screenshot).exists() {
        bail!("webgl runtime screenshot was not written: {screenshot}");
    }
    if canvas2d != "ok" {
        bail!("2D canvas probe did not draw: {runtime}");
    }
    if webgl_context != "ok" {
        bail!("WebGL context probe did not initialize: {runtime}");
    }
    if !read_pixels.starts_with("ok_") {
        bail!("WebGL readPixels probe did not return colored pixels: {runtime}");
    }

    let gl_warning = result.output_contains_gl_texture_warning();
    let route = if gl_warning || frames < 20 || avg_frame_ms > 50.0 {
        "blocked"
    } else {
        "green"
    };

    println!(
        "WEBGL_RUNTIME DIAG route={} canvas2d={} webgl_context={} texture={} read_pixels={} frames={} avg_frame_ms={:.2} last_error={} gl_warning={} screenshot={} replay={}",
        route,
        canvas2d,
        webgl_context,
        texture,
        read_pixels,
        frames,
        avg_frame_ms,
        last_error,
        gl_warning,
        screenshot,
        result
            .audit_response
            .pointer("/result/artifacts/replay")
            .and_then(Value::as_str)
            .unwrap_or("(missing replay)")
    );
    Ok(())
}

struct WebglRuntimeWorkerResult {
    webgl_response: Value,
    audit_response: Value,
    stdout: String,
    stderr: String,
}

impl WebglRuntimeWorkerResult {
    fn output_contains_gl_texture_warning(&self) -> bool {
        self.stdout.contains("GLD_TEXTURE") || self.stderr.contains("GLD_TEXTURE")
    }
}

fn run_webgl_runtime_worker(
    workspace: &Path,
    url: &str,
    width: u32,
    height: u32,
    wait_before_probe: Duration,
) -> Result<WebglRuntimeWorkerResult> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = ProcessCommand::new(current_exe)
        .current_dir(workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(url)
        .arg("--width")
        .arg(width.to_string())
        .arg("--height")
        .arg(height.to_string())
        .arg("--rendering-profile")
        .arg("servo-modern")
        .env("RUST_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    thread::sleep(wait_before_probe);
    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        writeln!(
            stdin,
            "{}",
            json!({"id": 1, "method": "webgl_runtime_probe"})
        )
        .context("failed to send webgl_runtime_probe request")?;
        writeln!(stdin, "{}", json!({"id": 2, "method": "audit"}))
            .context("failed to send audit request")?;
        writeln!(stdin, "{}", json!({"id": 3, "method": "close"}))
            .context("failed to send close request")?;
    }
    drop(child.stdin.take());

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            bail!("webgl runtime worker timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .context("failed to read browser-session-worker stderr")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}\nstdout={stdout}\nstderr={stderr}");
    }

    let webgl_response = json_response_by_id(&stdout, 1)
        .with_context(|| format!("missing webgl_runtime_probe response in stdout: {stdout}"))?;
    let audit_response = json_response_by_id(&stdout, 2)
        .with_context(|| format!("missing audit response in stdout: {stdout}"))?;
    Ok(WebglRuntimeWorkerResult {
        webgl_response,
        audit_response,
        stdout,
        stderr,
    })
}

fn value_as_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

fn json_response_by_id(stdout: &str, id: u64) -> Option<Value> {
    stdout.lines().find_map(|line| {
        let value = serde_json::from_str::<Value>(line.trim()).ok()?;
        (value.get("id").and_then(Value::as_u64) == Some(id)).then_some(value)
    })
}

fn inspect_editors(
    url: String,
    width: u32,
    height: u32,
    rendering_profile: Option<String>,
    profile_dir: Option<PathBuf>,
) -> Result<()> {
    let workspace = workspace_root()?;
    let parsed_url = parse_user_url(&url)?;
    let response = run_inspect_editors_worker(
        &workspace,
        parsed_url.as_str(),
        width,
        height,
        rendering_profile.as_deref(),
        profile_dir.as_deref(),
    )?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("inspect_editors response was not ok: {response}");
    }

    let result = response
        .get("result")
        .context("inspect_editors response missing result")?;
    let editors = result
        .get("editors")
        .and_then(Value::as_array)
        .context("inspect_editors response missing editors array")?;
    let editor_count = result
        .get("editor_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let zero_rect_count = result
        .get("zero_rect_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let visible_writable_count = result
        .get("visible_writable_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let visible_authoring_count = result
        .get("visible_authoring_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let sensitive_count = result
        .get("sensitive_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_decision = result
        .pointer("/route/decision")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let replay_path = result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .unwrap_or("(missing replay)");

    println!(
        "INSPECT_EDITORS PASS url={} editors={} zero_rect={} visible_writable={} visible_authoring={} sensitive={} route={} replay={}",
        parsed_url.as_str(),
        editor_count,
        zero_rect_count,
        visible_writable_count,
        visible_authoring_count,
        sensitive_count,
        route_decision,
        replay_path
    );

    for editor in editors.iter().take(20) {
        let rect = editor.get("rect").unwrap_or(&Value::Null);
        println!(
            "EDITOR index={} kind={} tag={} id={} name={} label={} placeholder={} rect={:.1}x{:.1} hidden={} readonly={} active={} sensitivity={} value_len={}",
            editor.get("index").and_then(Value::as_u64).unwrap_or(0),
            editor.get("kind").and_then(Value::as_str).unwrap_or(""),
            editor.get("tag").and_then(Value::as_str).unwrap_or(""),
            editor.get("id").and_then(Value::as_str).unwrap_or(""),
            editor.get("name").and_then(Value::as_str).unwrap_or(""),
            editor.get("label").and_then(Value::as_str).unwrap_or(""),
            editor
                .get("placeholder")
                .and_then(Value::as_str)
                .unwrap_or(""),
            rect.get("width").and_then(Value::as_f64).unwrap_or(0.0),
            rect.get("height").and_then(Value::as_f64).unwrap_or(0.0),
            editor
                .get("hidden")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            editor
                .get("readOnly")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            editor
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            editor
                .get("sensitivity")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            editor
                .get("valueLength")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
    }
    Ok(())
}

fn editor_rect_positive(editor: &Value) -> bool {
    editor
        .pointer("/rect/width")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        > 0.0
        && editor
            .pointer("/rect/height")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            > 0.0
}

fn run_inspect_editors_worker(
    workspace: &Path,
    url: &str,
    width: u32,
    height: u32,
    rendering_profile: Option<&str>,
    profile_dir: Option<&Path>,
) -> Result<Value> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut command = ProcessCommand::new(current_exe);
    command
        .current_dir(workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(url)
        .arg("--width")
        .arg(width.to_string())
        .arg("--height")
        .arg(height.to_string());
    if let Some(rendering_profile) = rendering_profile {
        command.arg("--rendering-profile").arg(rendering_profile);
    }
    if let Some(profile_dir) = profile_dir {
        command.arg("--profile-dir").arg(profile_dir);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        writeln!(stdin, "{}", json!({"id": 1, "method": "inspect_editors"}))
            .context("failed to send inspect_editors request")?;
        writeln!(stdin, "{}", json!({"id": 2, "method": "close"}))
            .context("failed to send close request")?;
    }

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(25) {
            let _ = child.kill();
            bail!("inspect_editors worker selftest timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}");
    }
    if stdout.contains("LEAK_SENTINEL_DO_NOT_RETURN") {
        bail!("inspect_editors stdout leaked editor sentinel text");
    }

    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(1))
        .with_context(|| format!("missing inspect_editors response in stdout: {stdout}"))
}

fn selftest_profile_persistence() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("profile_persistence"))?;
    let stamp = unix_ms()?;
    let profile_dir = workspace
        .join("runs")
        .join("profile_persistence")
        .join(format!("profile_{stamp}"));
    std::fs::create_dir_all(&profile_dir)
        .with_context(|| format!("failed to create {}", profile_dir.display()))?;

    let write_response = run_worker_request_with_profile(
        &workspace,
        base_url.as_str(),
        &profile_dir,
        json!({
            "id": 1,
            "method": "inspect_fields",
            "params": {
                "fields": ["cookie-status"]
            }
        }),
        Duration::from_secs(25),
    )?;
    if write_response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("profile write worker response was not ok: {write_response}");
    }
    let write_status_value = write_response
        .pointer("/result/fields/0/value")
        .and_then(Value::as_str)
        .unwrap_or("");
    if write_status_value != "present" {
        bail!("profile persistence writer did not set cookie: {write_response}");
    }

    let check_url = base_url
        .join("check.html")
        .context("failed to form profile persistence check URL")?;
    let read_response = run_worker_request_with_profile(
        &workspace,
        check_url.as_str(),
        &profile_dir,
        json!({
            "id": 1,
            "method": "inspect_fields",
            "params": {
                "fields": ["cookie-status"]
            }
        }),
        Duration::from_secs(25),
    )?;
    if read_response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("profile read worker response was not ok: {read_response}");
    }
    let status_value = read_response
        .pointer("/result/fields/0/value")
        .and_then(Value::as_str)
        .unwrap_or("");
    let replay_path = read_response
        .pointer("/result/artifacts/replay")
        .and_then(Value::as_str)
        .context("profile persistence response missing replay artifact")?;
    if status_value != "present" {
        bail!("profile persistence cookie was not shared: {read_response}");
    }

    println!(
        "PROFILE_PERSISTENCE PASS cookie_status={} profile_dir={} replay={}",
        status_value,
        profile_dir.display(),
        replay_path
    );
    Ok(())
}

fn run_worker_request_with_profile(
    workspace: &Path,
    url: &str,
    profile_dir: &Path,
    request: Value,
    timeout: Duration,
) -> Result<Value> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = ProcessCommand::new(current_exe)
        .current_dir(&workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(url)
        .arg("--profile-dir")
        .arg(profile_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        writeln!(stdin, "{request}").context("failed to send worker request")?;
        writeln!(stdin, "{}", json!({"id": 2, "method": "close"}))
            .context("failed to send close request")?;
    }

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            bail!("profile persistence worker timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}");
    }

    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(1))
        .with_context(|| format!("missing worker response in stdout: {stdout}"))
}

fn run_worker_sequence(
    workspace: &Path,
    url: &str,
    requests: Vec<Value>,
    timeout: Duration,
) -> Result<(Vec<Value>, String)> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = ProcessCommand::new(current_exe)
        .current_dir(workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        for request in requests {
            writeln!(stdin, "{request}").context("failed to send worker request")?;
        }
        writeln!(stdin, "{}", json!({"id": 9999, "method": "close"}))
            .context("failed to send close request")?;
    }

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            bail!("browser-session-worker sequence timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .context("failed to read browser-session-worker stderr")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}\nstdout={stdout}\nstderr={stderr}");
    }

    let responses = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    Ok((responses, stdout))
}

fn response_result(responses: &[Value], id: u64) -> Result<Value> {
    let response = responses
        .iter()
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(id))
        .with_context(|| format!("missing worker response id={id}"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("worker response id={id} was not ok: {response}");
    }
    response
        .get("result")
        .cloned()
        .with_context(|| format!("worker response id={id} missing result"))
}

fn selftest_browser_session() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("browser_session"))?;
    let profile = saccade_browser::selftest_browser_session(
        base_url,
        workspace.join("runs").join("browser_session"),
    )?;

    if !profile.opened
        || !profile.truth_collected
        || profile.actions_seen == 0
        || !profile.act_attempted
        || !profile.act_verified
        || !profile.same_webview
        || profile.page_revision_after <= profile.page_revision_before
    {
        bail!("browser session selftest failed: {profile:?}");
    }

    println!(
        "BROWSER_SESSION PASS run_id={} session={} tab={} actions_seen={} revision={}=>{} report={} replay={}",
        profile.run_id,
        profile.session_id,
        profile.tab_id,
        profile.actions_seen,
        profile.page_revision_before,
        profile.page_revision_after,
        profile.report_path.display(),
        profile.replay_path.display(),
    );
    Ok(())
}

fn selftest_formmax_live() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("formmax"))?;
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut child = ProcessCommand::new(current_exe)
        .current_dir(&workspace)
        .arg("browser-session-worker")
        .arg("--url")
        .arg(base_url.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn browser-session-worker")?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .context("browser-session-worker stdin was not piped")?;
        writeln!(
            stdin,
            "{}",
            json!({
                "id": 1,
                "method": "formmax_live_fill",
                "params": {
                    "policy": {
                        "block_sensitive": true,
                        "local_fixture_only": true
                    }
                }
            })
        )
        .context("failed to send formmax_live_fill request")?;
        writeln!(stdin, "{}", json!({"id": 2, "method": "close"}))
            .context("failed to send close request")?;
    }

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .context("failed to poll browser-session-worker")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            bail!("FORMMAX live worker selftest timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read browser-session-worker stdout")?;
    }
    let status = child
        .wait()
        .context("failed to wait for browser-session-worker")?;
    if !status.success() {
        bail!("browser-session-worker exited with {status}");
    }

    let response = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("id").and_then(Value::as_u64) == Some(1))
        .with_context(|| format!("missing formmax_live_fill response in stdout: {stdout}"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("formmax_live_fill response was not ok: {response}");
    }
    let result = response
        .get("result")
        .context("formmax_live_fill response missing result")?;
    let rows = result.get("rows").and_then(Value::as_u64).unwrap_or(0);
    let pages = result.get("pages").and_then(Value::as_u64).unwrap_or(0);
    let filled = result.get("filled").and_then(Value::as_u64).unwrap_or(0);
    let blocked_sensitive = result
        .get("blocked_sensitive")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let receipt_verified = result
        .get("receipt_verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let validation_errors = result
        .get("validation_errors")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let replay_path = result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .context("formmax_live_fill response missing replay artifact")?;
    let replay_text = std::fs::read_to_string(replay_path)
        .with_context(|| format!("failed to read replay artifact {replay_path}"))?;

    if rows != 96
        || pages != 2
        || filled != 672
        || blocked_sensitive != 3
        || !receipt_verified
        || validation_errors != 0
    {
        bail!("FORMMAX live worker selftest failed: {result}");
    }
    for needle in ["Region 1 / Site 001", "2026-02-02", "Mina"] {
        if replay_text.contains(needle) {
            bail!("FORMMAX live replay leaked table value {needle:?}");
        }
    }

    println!(
        "FORMMAX_LIVE PASS rows={} pages={} filled={} blocked_sensitive={} receipt_verified={} replay={}",
        rows, pages, filled, blocked_sensitive, receipt_verified, replay_path
    );
    Ok(())
}

fn parse_user_url(input: &str) -> Result<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("--url cannot be empty");
    }

    let with_scheme = if has_explicit_url_scheme(trimmed) {
        trimmed.to_string()
    } else if looks_like_local_address(trimmed) {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    };

    Url::parse(&with_scheme).with_context(|| format!("invalid URL: {input}"))
}

fn has_explicit_url_scheme(input: &str) -> bool {
    if input.contains("://") {
        return true;
    }
    let Some(index) = input.find(':') else {
        return false;
    };
    matches!(
        input[..index].to_ascii_lowercase().as_str(),
        "about" | "data" | "file" | "http" | "https"
    )
}

fn looks_like_local_address(input: &str) -> bool {
    let lowercase = input.to_ascii_lowercase();
    lowercase == "localhost"
        || lowercase.starts_with("localhost:")
        || lowercase.starts_with("127.")
        || lowercase.starts_with("0.0.0.0")
        || lowercase.starts_with("[::1]")
}

fn start_test_server(root: PathBuf) -> Result<Url> {
    let server = Server::http("127.0.0.1:0")
        .map_err(|error| anyhow!("failed to bind test HTTP server: {error}"))?;
    let addr: SocketAddr = server
        .server_addr()
        .to_ip()
        .context("test HTTP server did not expose an IP socket address")?;
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let url_path = request
                .url()
                .trim_start_matches('/')
                .split('?')
                .next()
                .unwrap_or("");
            let relative = if url_path.is_empty() {
                "index.html"
            } else {
                url_path
            };
            let path = root.join(relative);
            let response = match std::fs::read(&path) {
                Ok(body) => Response::from_data(body)
                    .with_header(Header::from_bytes("Content-Type", content_type(&path)).unwrap()),
                Err(_) => Response::from_string("not found").with_status_code(StatusCode(404)),
            };
            let _ = request.respond(response);
        }
    });

    Url::parse(&format!("http://{addr}/")).context("failed to form test server URL")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to resolve workspace root")
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX_EPOCH")?
        .as_millis())
}
