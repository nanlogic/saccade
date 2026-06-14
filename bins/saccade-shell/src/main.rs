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
    SelftestNativeInput,
    SelftestNativeInputDemo,
    SelftestFocusedType,
    SelftestEditorReduction,
    SelftestProfilePersistence,
    SelftestBrowserSession,
    SelftestFormmaxLive,
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
        Command::SelftestNativeInput => selftest_native_input(),
        Command::SelftestNativeInputDemo => selftest_native_input_demo(),
        Command::SelftestFocusedType => selftest_focused_type(),
        Command::SelftestEditorReduction => selftest_editor_reduction(),
        Command::SelftestProfilePersistence => selftest_profile_persistence(),
        Command::SelftestBrowserSession => selftest_browser_session(),
        Command::SelftestFormmaxLive => selftest_formmax_live(),
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
    let response = run_inspect_editors_worker(&workspace, url.as_str())?;
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
    {
        bail!(
            "editor reduction selftest failed: editor_count={editor_count} zero_rect_count={zero_rect_count} sensitive_count={sensitive_count} result={result}"
        );
    }
    if replay_text.contains("LEAK_SENTINEL_DO_NOT_RETURN") {
        bail!("inspect_editors replay leaked editor sentinel text");
    }

    println!(
        "EDITOR_REDUCTION PASS editors={} zero_rect={} sensitive={} visible_body=true visible_shell=true replay={}",
        editor_count, zero_rect_count, sensitive_count, replay_path
    );
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

fn run_inspect_editors_worker(workspace: &Path, url: &str) -> Result<Value> {
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

    let with_scheme =
        if trimmed.contains("://") || trimmed.starts_with("about:") || trimmed.starts_with("file:")
        {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };

    Url::parse(&with_scheme).with_context(|| format!("invalid URL: {input}"))
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
