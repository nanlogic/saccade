use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use image::{Rgba, RgbaImage};
use saccade_browser::{ArenaRunConfig, RealRunConfig, RealSiteRecon};
use saccade_core::{BenchmarkResult, ClickOutcome, CssRect, Histogram, InputSpace, LatencyPair};
use saccade_replay::{ReplayEvent, read_events};
use serde_json::Value;
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const FRAME_BUDGET_MS: f32 = 16.667;
const INPUT_DISPATCH_P95_LIMIT_MS: f32 = 5.0;

#[derive(Parser)]
#[command(name = "mousemax")]
#[command(about = "Saccade MOUSEMAX harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    SelftestBoot,
    Calibrate,
    SelftestPages,
    Run {
        #[arg(long, default_value = "arena")]
        site: String,
        #[arg(long, default_value = "Epic")]
        spawn_speed: String,
        #[arg(long, default_value = "Tiny")]
        target_size: String,
        #[arg(long, default_value_t = 15)]
        duration: u32,
        #[arg(long, default_value_t = 1920)]
        window_width: u32,
        #[arg(long, default_value_t = 1080)]
        window_height: u32,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long)]
        replay: bool,
        #[arg(long, default_value = "observe_only")]
        instrumentation: String,
    },
    Replay {
        log: PathBuf,
        #[arg(long)]
        summary: bool,
        #[arg(long)]
        show_clicks: bool,
        #[arg(long)]
        render_summary: Option<PathBuf>,
    },
    ValidateRun {
        run_dir: PathBuf,
        #[arg(long)]
        require_click_map: bool,
    },
    ReconReal {
        #[arg(long, default_value = "https://mouseaccuracy.com/classic/")]
        url: Url,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::SelftestBoot => selftest_boot(),
        Command::Calibrate => calibrate(),
        Command::SelftestPages => selftest_pages(),
        Command::Run {
            site,
            spawn_speed,
            target_size,
            duration,
            window_width,
            window_height,
            seed,
            replay,
            instrumentation,
        } => run(
            site,
            spawn_speed,
            target_size,
            duration,
            window_width,
            window_height,
            seed,
            replay,
            instrumentation,
        ),
        Command::Replay {
            log,
            summary,
            show_clicks,
            render_summary,
        } => replay(log, summary, show_clicks, render_summary),
        Command::ValidateRun {
            run_dir,
            require_click_map,
        } => validate_run(run_dir, require_click_map),
        Command::ReconReal { url } => recon_real(url),
    }
}

fn selftest_boot() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("failed to resolve workspace root")?
        .join("test_pages");
    let base_url = start_test_server(root)?;
    let calibration_url = base_url
        .join("calibration.html")
        .context("failed to build calibration URL")?;

    let title = saccade_browser::selftest_boot(calibration_url)?;
    if title != "Calibration" {
        bail!("expected page title \"Calibration\", got {title:?}");
    }

    println!("BOOT OK title=\"{title}\"");
    Ok(())
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
                "calibration.html"
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

fn calibrate() -> Result<()> {
    let workspace = workspace_root()?;
    let root = workspace.join("test_pages");
    let base_url = start_test_server(root)?;
    let calibration_url = base_url
        .join("calibration.html")
        .context("failed to build calibration URL")?;

    let report = saccade_browser::calibrate_input(calibration_url)?;
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs();
    let output_dir = workspace
        .join("runs")
        .join("calibration")
        .join(run_id.to_string());
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let config = serde_json::json!({
        "browser_config_hash": "servo-0.2.0-window-1280x800-hidpi-1",
        "input_space": report.input_space,
        "max_err_css_px": report.max_err_css_px,
        "device_pixel_ratio": report.device_pixel_ratio,
        "attempts": report.attempts,
    });
    let pretty = serde_json::to_string_pretty(&config)?;
    let run_config_path = output_dir.join("calibration.json");
    std::fs::write(&run_config_path, &pretty)
        .with_context(|| format!("failed to write {}", run_config_path.display()))?;
    let latest_dir = workspace.join("runs").join("calibration");
    std::fs::create_dir_all(&latest_dir)
        .with_context(|| format!("failed to create {}", latest_dir.display()))?;
    let latest_path = latest_dir.join("latest.json");
    std::fs::write(&latest_path, pretty)
        .with_context(|| format!("failed to write {}", latest_path.display()))?;

    println!(
        "CALIBRATION OK max_err_css_px={:.3} input_space={:?} config={}",
        report.max_err_css_px,
        report.input_space,
        run_config_path.display()
    );
    Ok(())
}

fn selftest_pages() -> Result<()> {
    let workspace = workspace_root()?;
    let calibration = latest_calibration(&workspace)?;
    let base_url = start_test_server(workspace.join("test_pages"))?;
    let report = saccade_browser::selftest_pages(base_url, calibration.input_space)?;

    let mut failed = Vec::new();
    for outcome in &report.outcomes {
        let status = if outcome.passed { "PASS" } else { "FAIL" };
        println!(
            "{status} {} truth=\"{}\" clicks_sent={} {}",
            outcome.name, outcome.truth, outcome.clicks_sent, outcome.detail
        );
        if !outcome.passed {
            failed.push(outcome.name.clone());
        }
    }

    if failed.is_empty() {
        println!("SELFTEST PAGES PASS pages={}", report.outcomes.len());
        Ok(())
    } else {
        bail!("SELFTEST PAGES FAIL failed={}", failed.join(","))
    }
}

#[derive(Debug, Clone, Copy)]
struct LatestCalibration {
    input_space: InputSpace,
    max_err_css_px: f32,
}

fn latest_calibration(workspace: &Path) -> Result<LatestCalibration> {
    let path = workspace
        .join("runs")
        .join("calibration")
        .join("latest.json");
    let raw = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "missing calibration config {}; run mousemax calibrate",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let input_space = match value
        .get("input_space")
        .and_then(Value::as_str)
        .context("calibration config missing input_space")?
    {
        "CssLogical" => InputSpace::CssLogical,
        "DevicePhysical" => InputSpace::DevicePhysical,
        other => bail!("unknown input_space {other:?} in {}", path.display()),
    };
    let max_err_css_px = value
        .get("max_err_css_px")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    Ok(LatestCalibration {
        input_space,
        max_err_css_px,
    })
}

fn run(
    site: String,
    spawn_speed: String,
    target_size: String,
    duration: u32,
    window_width: u32,
    window_height: u32,
    seed: u64,
    replay: bool,
    instrumentation: String,
) -> Result<()> {
    if window_width == 0 || window_height == 0 {
        bail!("window size must be non-zero");
    }
    let workspace = workspace_root()?;
    let calibration = latest_calibration(&workspace)?;

    let run_id = format!(
        "run_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before UNIX_EPOCH")?
            .as_secs()
    );
    let site = site.to_lowercase();
    let output_dir = workspace.join("runs").join(&site).join(&run_id);
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let replay_path = replay.then(|| output_dir.join("replay.jsonl"));

    let result = match site.as_str() {
        "arena" => {
            let base_url = start_test_server(workspace.join("test_pages"))?;
            let mut url = base_url
                .join("arena/index.html")
                .context("failed to build arena URL")?;
            url.query_pairs_mut()
                .append_pair("speed", &spawn_speed.to_lowercase())
                .append_pair("size", &target_size.to_lowercase())
                .append_pair("duration", &duration.to_string())
                .append_pair("seed", &seed.to_string());
            saccade_browser::run_arena(ArenaRunConfig {
                url,
                run_id,
                spawn_speed,
                target_size,
                duration_s: duration,
                artifact_dir: output_dir.clone(),
                window_width,
                window_height,
                seed,
                instrumentation,
                input_space: calibration.input_space,
                calibration_max_err_css_px: calibration.max_err_css_px,
                replay_path,
            })?
            .result
        }
        "real" => {
            let url = Url::parse("https://mouseaccuracy.com/classic/")
                .context("failed to parse real site URL")?;
            saccade_browser::run_real(RealRunConfig {
                url,
                run_id,
                spawn_speed,
                target_size,
                duration_s: duration,
                artifact_dir: output_dir.clone(),
                window_width,
                window_height,
                instrumentation,
                input_space: calibration.input_space,
                calibration_max_err_css_px: calibration.max_err_css_px,
                replay_path,
            })?
            .result
        }
        other => bail!("unknown --site {other:?}; expected arena or real"),
    };
    let result_path = output_dir.join("result.json");
    let pretty = serde_json::to_string_pretty(&result)?;
    std::fs::write(&result_path, &pretty)
        .with_context(|| format!("failed to write {}", result_path.display()))?;
    println!("{pretty}");

    if result.verdict == "PASS" {
        Ok(())
    } else {
        bail!("RUN FAIL result={}", result_path.display())
    }
}

fn replay(
    log: PathBuf,
    summary: bool,
    show_clicks: bool,
    render_summary: Option<PathBuf>,
) -> Result<()> {
    let events = read_events(&log)?;
    let mut detect_to_dispatch = Histogram::new();
    let mut first_visible_to_dispatch = Histogram::new();
    let mut run_finished = None;
    let mut click_count = 0_u32;

    for event in &events {
        match event {
            ReplayEvent::ClickDispatched { receipt } => {
                click_count += 1;
                detect_to_dispatch
                    .record_ns(receipt.t_down_sent_ns.saturating_sub(receipt.t_decided_ns));
                first_visible_to_dispatch.record_ns(
                    receipt
                        .t_down_sent_ns
                        .saturating_sub(receipt.t_target_first_seen_ns),
                );
                if show_clicks {
                    println!(
                        "click={} target={} frame={} first_visible_to_down_ms={:.3} decided_to_down_ms={:.3} point=({:.1},{:.1})",
                        receipt.click_id,
                        receipt.target_id.0,
                        receipt.frame_id,
                        receipt
                            .t_down_sent_ns
                            .saturating_sub(receipt.t_target_first_seen_ns)
                            as f32
                            / 1_000_000.0,
                        receipt.t_down_sent_ns.saturating_sub(receipt.t_decided_ns) as f32
                            / 1_000_000.0,
                        receipt.point_css.x,
                        receipt.point_css.y,
                    );
                }
            }
            ReplayEvent::RunFinished { result } => run_finished = Some(result.clone()),
            _ => {}
        }
    }

    let detect_pair = LatencyPair::from(&detect_to_dispatch);
    let first_visible_pair = LatencyPair::from(&first_visible_to_dispatch);
    if summary || !show_clicks {
        if let Some(result) = run_finished {
            println!(
                "REPLAY SUMMARY verdict={} hits={} misses={} targets_seen={} clicks_sent={} detect_to_dispatch_p95_ms={:.3} first_visible_to_dispatch_p95_ms={:.3} replay={}",
                result.verdict,
                result.result.hits,
                result.result.misses,
                result.result.targets_seen,
                result.result.clicks_sent,
                detect_pair.p95,
                first_visible_pair.p95,
                log.display()
            );
        } else {
            println!(
                "REPLAY SUMMARY clicks={} detect_to_dispatch_p95_ms={:.3} first_visible_to_dispatch_p95_ms={:.3} replay={}",
                click_count,
                detect_pair.p95,
                first_visible_pair.p95,
                log.display()
            );
        }
    }
    if let Some(output) = render_summary {
        render_replay_summary(&events, &output)?;
        println!("REPLAY RENDER summary={}", output.display());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ReplayClickPoint {
    click_id: u64,
    x: f32,
    y: f32,
    outcome: ClickOutcome,
}

fn render_replay_summary(events: &[ReplayEvent], output: &Path) -> Result<()> {
    let (width, height) = replay_canvas_size(events);
    let game_area = replay_game_area(events);
    let clicks = replay_click_points(events);

    let mut image = RgbaImage::from_pixel(width, height, Rgba([250, 250, 248, 255]));
    fill_rect(
        &mut image,
        0,
        0,
        width as i32,
        height as i32,
        Rgba([250, 250, 248, 255]),
    );
    draw_grid(&mut image, 120, Rgba([229, 232, 235, 255]));

    if let Some(rect) = game_area {
        fill_css_rect(&mut image, rect, Rgba([244, 247, 248, 255]));
        draw_css_rect(&mut image, rect, Rgba([88, 96, 104, 255]));
        draw_css_rect_inset(&mut image, rect, 1, Rgba([183, 190, 197, 255]));
    }

    for click in &clicks {
        let color = match click.outcome {
            ClickOutcome::Hit => Rgba([19, 142, 81, 255]),
            ClickOutcome::Miss => Rgba([214, 49, 49, 255]),
            ClickOutcome::Unknown => Rgba([221, 154, 31, 255]),
            ClickOutcome::Stale => Rgba([117, 82, 175, 255]),
        };
        let x = click.x.round() as i32;
        let y = click.y.round() as i32;
        draw_circle(&mut image, x, y, 9, Rgba([255, 255, 255, 255]));
        draw_circle(&mut image, x, y, 7, color);
        draw_circle_outline(&mut image, x, y, 9, Rgba([32, 36, 40, 255]));
        if click.click_id == 1 || click.click_id % 10 == 0 {
            draw_crosshair(&mut image, x, y, 13, Rgba([32, 36, 40, 255]));
        }
    }

    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    image
        .save(output)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn replay_canvas_size(events: &[ReplayEvent]) -> (u32, u32) {
    for event in events {
        if let ReplayEvent::RunStarted { config, .. } = event {
            let width = config
                .get("window_width")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(1920)
                .clamp(320, 4096);
            let height = config
                .get("window_height")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(1080)
                .clamp(240, 4096);
            return (width, height);
        }
    }
    (1920, 1080)
}

fn replay_game_area(events: &[ReplayEvent]) -> Option<CssRect> {
    events.iter().rev().find_map(|event| match event {
        ReplayEvent::FrameReport { report }
            if report.game_area_css.w > 0.0 && report.game_area_css.h > 0.0 =>
        {
            Some(report.game_area_css)
        }
        _ => None,
    })
}

fn replay_click_points(events: &[ReplayEvent]) -> Vec<ReplayClickPoint> {
    let mut outcomes = HashMap::new();
    for event in events {
        if let ReplayEvent::ClickVerified { result } = event {
            outcomes.insert(result.click_id, result.outcome);
        }
    }

    let mut clicks = Vec::new();
    for event in events {
        if let ReplayEvent::ClickDispatched { receipt } = event {
            clicks.push(ReplayClickPoint {
                click_id: receipt.click_id,
                x: receipt.point_css.x,
                y: receipt.point_css.y,
                outcome: outcomes
                    .get(&receipt.click_id)
                    .copied()
                    .unwrap_or(ClickOutcome::Unknown),
            });
        }
    }
    clicks
}

fn fill_css_rect(image: &mut RgbaImage, rect: CssRect, color: Rgba<u8>) {
    fill_rect(
        image,
        rect.x.round() as i32,
        rect.y.round() as i32,
        rect.w.round() as i32,
        rect.h.round() as i32,
        color,
    );
}

fn fill_rect(image: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    for yy in y.max(0)..(y + h).min(image.height() as i32) {
        for xx in x.max(0)..(x + w).min(image.width() as i32) {
            image.put_pixel(xx as u32, yy as u32, color);
        }
    }
}

fn draw_css_rect(image: &mut RgbaImage, rect: CssRect, color: Rgba<u8>) {
    let x = rect.x.round() as i32;
    let y = rect.y.round() as i32;
    let w = rect.w.round() as i32;
    let h = rect.h.round() as i32;
    draw_rect_outline(image, x, y, w, h, color);
}

fn draw_css_rect_inset(image: &mut RgbaImage, rect: CssRect, inset: i32, color: Rgba<u8>) {
    let x = rect.x.round() as i32 + inset;
    let y = rect.y.round() as i32 + inset;
    let w = rect.w.round() as i32 - inset * 2;
    let h = rect.h.round() as i32 - inset * 2;
    draw_rect_outline(image, x, y, w, h, color);
}

fn draw_grid(image: &mut RgbaImage, spacing: u32, color: Rgba<u8>) {
    if spacing == 0 {
        return;
    }
    for x in (0..image.width()).step_by(spacing as usize) {
        for y in 0..image.height() {
            image.put_pixel(x, y, color);
        }
    }
    for y in (0..image.height()).step_by(spacing as usize) {
        for x in 0..image.width() {
            image.put_pixel(x, y, color);
        }
    }
}

fn draw_rect_outline(image: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    if w <= 0 || h <= 0 {
        return;
    }
    draw_line(image, x, y, x + w - 1, y, color);
    draw_line(image, x, y + h - 1, x + w - 1, y + h - 1, color);
    draw_line(image, x, y, x, y + h - 1, color);
    draw_line(image, x + w - 1, y, x + w - 1, y + h - 1, color);
}

fn draw_crosshair(image: &mut RgbaImage, x: i32, y: i32, radius: i32, color: Rgba<u8>) {
    draw_line(image, x - radius, y, x - 4, y, color);
    draw_line(image, x + 4, y, x + radius, y, color);
    draw_line(image, x, y - radius, x, y - 4, color);
    draw_line(image, x, y + 4, x, y + radius, color);
}

fn draw_line(image: &mut RgbaImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        put_pixel_checked(image, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(image: &mut RgbaImage, center_x: i32, center_y: i32, radius: i32, color: Rgba<u8>) {
    let r2 = radius * radius;
    for y in (center_y - radius)..=(center_y + radius) {
        for x in (center_x - radius)..=(center_x + radius) {
            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= r2 {
                put_pixel_checked(image, x, y, color);
            }
        }
    }
}

fn draw_circle_outline(
    image: &mut RgbaImage,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: Rgba<u8>,
) {
    let outer = radius * radius;
    let inner = (radius - 2).max(0) * (radius - 2).max(0);
    for y in (center_y - radius)..=(center_y + radius) {
        for x in (center_x - radius)..=(center_x + radius) {
            let dx = x - center_x;
            let dy = y - center_y;
            let d2 = dx * dx + dy * dy;
            if d2 <= outer && d2 >= inner {
                put_pixel_checked(image, x, y, color);
            }
        }
    }
}

fn put_pixel_checked(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn validate_run(run_dir: PathBuf, require_click_map: bool) -> Result<()> {
    let result_path = run_dir.join("result.json");
    let result = read_benchmark_result(&result_path)?;
    let replay_path = resolve_replay_path(&run_dir, &result);
    let events = read_events(&replay_path)?;
    let replay_finished = replay_finished_result(&events);

    let mut failures = Vec::new();
    validate_result_metrics(&result, &mut failures);
    validate_replay_consistency(&result, replay_finished.as_ref(), &events, &mut failures);
    validate_artifacts(&run_dir, &replay_path, require_click_map, &mut failures);

    if !failures.is_empty() {
        for failure in &failures {
            println!("VALIDATE FAIL {failure}");
        }
        bail!(
            "run validation failed run={} failures={}",
            run_dir.display(),
            failures.len()
        );
    }

    println!(
        "VALIDATE PASS run={} verdict={} site={} instrumentation={} hits={} misses={} targets_seen={} clicks_sent={} detect_to_dispatch_p95_ms={:.3} first_visible_to_dispatch_p95_ms={:.3}",
        run_dir.display(),
        result.verdict,
        result.site,
        result.instrumentation,
        result.result.hits,
        result.result.misses,
        result.result.targets_seen,
        result.result.clicks_sent,
        result.latency_ms.detect_to_dispatch.p95,
        result.latency_ms.first_visible_to_dispatch.p95,
    );
    Ok(())
}

fn read_benchmark_result(path: &Path) -> Result<BenchmarkResult> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn resolve_replay_path(run_dir: &Path, result: &BenchmarkResult) -> PathBuf {
    let replay_file = PathBuf::from(&result.replay_file);
    if replay_file.is_absolute() {
        replay_file
    } else {
        run_dir.join(replay_file)
    }
}

fn replay_finished_result(events: &[ReplayEvent]) -> Option<BenchmarkResult> {
    events.iter().rev().find_map(|event| match event {
        ReplayEvent::RunFinished { result } => Some(result.clone()),
        _ => None,
    })
}

fn validate_result_metrics(result: &BenchmarkResult, failures: &mut Vec<String>) {
    if result.verdict != "PASS" {
        failures.push(format!(
            "result.verdict expected PASS got {}",
            result.verdict
        ));
    }
    if result.result.misses != 0 {
        failures.push(format!("misses expected 0 got {}", result.result.misses));
    }
    if result.result.hits != result.result.targets_seen {
        failures.push(format!(
            "hits != targets_seen ({} != {})",
            result.result.hits, result.result.targets_seen
        ));
    }
    if result.result.hits != result.result.clicks_sent {
        failures.push(format!(
            "hits != clicks_sent ({} != {})",
            result.result.hits, result.result.clicks_sent
        ));
    }
    if result.result.false_positive_clicks != 0 {
        failures.push(format!(
            "false_positive_clicks expected 0 got {}",
            result.result.false_positive_clicks
        ));
    }
    if result.result.stale_clicks != 0 {
        failures.push(format!(
            "stale_clicks expected 0 got {}",
            result.result.stale_clicks
        ));
    }
    if result.result.unknown_verifications != 0 {
        failures.push(format!(
            "unknown_verifications expected 0 got {}",
            result.result.unknown_verifications
        ));
    }
    if result.llm_frame_calls != 0 {
        failures.push(format!(
            "llm_frame_calls expected 0 got {}",
            result.llm_frame_calls
        ));
    }
    if result.latency_ms.detect_to_dispatch.p95 > INPUT_DISPATCH_P95_LIMIT_MS {
        failures.push(format!(
            "detect_to_dispatch p95 {:.3} ms > {:.3} ms",
            result.latency_ms.detect_to_dispatch.p95, INPUT_DISPATCH_P95_LIMIT_MS
        ));
    }
    let first_visible_limit = FRAME_BUDGET_MS + INPUT_DISPATCH_P95_LIMIT_MS;
    if result.latency_ms.first_visible_to_dispatch.p95 > first_visible_limit {
        failures.push(format!(
            "first_visible_to_dispatch p95 {:.3} ms > {:.3} ms",
            result.latency_ms.first_visible_to_dispatch.p95, first_visible_limit
        ));
    }
}

fn validate_replay_consistency(
    result: &BenchmarkResult,
    replay_finished: Option<&BenchmarkResult>,
    events: &[ReplayEvent],
    failures: &mut Vec<String>,
) {
    let click_dispatched = events
        .iter()
        .filter(|event| matches!(event, ReplayEvent::ClickDispatched { .. }))
        .count() as u32;
    let click_verified_hit = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ReplayEvent::ClickVerified {
                    result
                } if result.outcome == ClickOutcome::Hit
            )
        })
        .count() as u32;

    if click_dispatched != result.result.clicks_sent {
        failures.push(format!(
            "replay click_dispatched != result clicks_sent ({} != {})",
            click_dispatched, result.result.clicks_sent
        ));
    }
    if click_verified_hit != result.result.hits {
        failures.push(format!(
            "replay verified hits != result hits ({} != {})",
            click_verified_hit, result.result.hits
        ));
    }
    let Some(replay_finished) = replay_finished else {
        failures.push("replay missing run_finished event".into());
        return;
    };
    if replay_finished.run_id != result.run_id {
        failures.push(format!(
            "replay run_id != result run_id ({} != {})",
            replay_finished.run_id, result.run_id
        ));
    }
    if replay_finished.result.hits != result.result.hits
        || replay_finished.result.misses != result.result.misses
        || replay_finished.result.targets_seen != result.result.targets_seen
        || replay_finished.result.clicks_sent != result.result.clicks_sent
    {
        failures.push("replay run_finished counters differ from result.json".into());
    }
}

fn validate_artifacts(
    run_dir: &Path,
    replay_path: &Path,
    require_click_map: bool,
    failures: &mut Vec<String>,
) {
    for path in [
        run_dir.join("result.json"),
        replay_path.to_path_buf(),
        run_dir.join("before.png"),
        run_dir.join("after.png"),
    ] {
        if !path.is_file() {
            failures.push(format!("missing artifact {}", path.display()));
        }
    }

    if require_click_map {
        let click_map = run_dir.join("click_map.png");
        if !click_map.is_file() {
            failures.push(format!("missing artifact {}", click_map.display()));
        }
    }
}

fn recon_real(url: Url) -> Result<()> {
    let workspace = workspace_root()?;
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs();
    let output_dir = workspace
        .join("runs")
        .join("recon")
        .join(run_id.to_string());

    let result = saccade_browser::real_site_recon(url.clone(), output_dir.clone())?;
    write_raw_probe_files(&output_dir, &result)?;
    let profile = site_profile_markdown(&url, &output_dir, &result);
    let profile_path = workspace.join("docs").join("site_profile.md");
    std::fs::write(&profile_path, profile)
        .with_context(|| format!("failed to write {}", profile_path.display()))?;

    println!(
        "RECON OK profile={} screenshots={}",
        profile_path.display(),
        result.screenshots.len()
    );
    Ok(())
}

fn write_raw_probe_files(output_dir: &Path, result: &RealSiteRecon) -> Result<()> {
    let probes = [
        ("initial_probe.json", result.initial_probe_json.as_deref()),
        (
            "after_options_probe.json",
            result.after_options_probe_json.as_deref(),
        ),
        (
            "arm_observation.json",
            result.arm_observation_json.as_deref(),
        ),
        ("final_probe.json", result.final_probe_json.as_deref()),
    ];

    for (filename, raw) in probes {
        if let Some(raw) = raw {
            let path = output_dir.join(filename);
            std::fs::write(&path, raw)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
    }

    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to resolve workspace root")
}

fn site_profile_markdown(url: &Url, output_dir: &Path, result: &RealSiteRecon) -> String {
    let initial = parse_json(result.initial_probe_json.as_deref());
    let after_options = parse_json(result.after_options_probe_json.as_deref());
    let final_probe = parse_json(result.final_probe_json.as_deref());
    let observation = final_probe
        .as_ref()
        .and_then(|value| value.get("observation"))
        .cloned();

    let initial_title = json_str(initial.as_ref(), "/title").unwrap_or("UNKNOWN");
    let dpr = json_value(initial.as_ref(), "/dpr")
        .map(Value::to_string)
        .unwrap_or_else(|| "UNKNOWN".into());
    let pointer_events = json_value(initial.as_ref(), "/pointerEvents")
        .map(Value::to_string)
        .unwrap_or_else(|| "UNKNOWN".into());
    let controls_found = controls_summary(initial.as_ref());
    let option_state = json_pretty(after_options.as_ref().and_then(|v| v.get("checked")));
    let score_text = json_pretty(final_probe.as_ref().and_then(|v| v.get("scoreText")));
    let result_text = json_pretty(final_probe.as_ref().and_then(|v| v.get("resultText")));
    let tech = classify_tech(initial.as_ref(), final_probe.as_ref(), observation.as_ref());
    let run_dir = output_dir.display();
    let canvas_json = format!(
        "{}\n{}",
        json_pretty(initial.as_ref().and_then(|v| v.get("canvases"))),
        json_pretty(final_probe.as_ref().and_then(|v| v.get("canvases")))
    );
    let iframe_json = format!(
        "{}\n{}",
        json_pretty(initial.as_ref().and_then(|v| v.get("iframes"))),
        json_pretty(final_probe.as_ref().and_then(|v| v.get("iframes")))
    );
    let observation_summary = observation_summary(observation.as_ref());
    let observation_evidence = compact_observation_json(observation.as_ref());
    let raw_probe_files = raw_probe_file_list(output_dir, result);
    let compat = if final_probe
        .as_ref()
        .and_then(|v| v.get("bodyTextSample"))
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("Time is up") || text.contains("You clicked"))
        && result.errors.iter().all(|error| {
            error.contains("take_screenshot timed out") || error.contains("readback fallback")
        }) {
        "GO".to_string()
    } else if result.errors.is_empty() {
        "GO".to_string()
    } else {
        format!("NO-GO({})", result.errors.join("; ").replace('\n', " "))
    };

    let screenshots = result
        .screenshots
        .iter()
        .map(|path| format!("- `{}`", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let screenshots = if screenshots.is_empty() {
        "none".into()
    } else {
        screenshots
    };
    let errors = if result.errors.is_empty() {
        "none".into()
    } else {
        result
            .errors
            .iter()
            .map(|error| format!("- {error}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"# Saccade M1 Site Profile

URL: `{url}`
Run directory: `{run_dir}`

## Load and Controls

- Page title: `{initial_title}`
- Device pixel ratio reported by page: `{dpr}`
- Pointer event support: `{pointer_events}`
- Control rect discovery: {controls_found}
- Option state after clicking Epic and Tiny:

```json
{option_state}
```

## Page Technology

- Classified tech: `{tech}`
- Canvas list from initial/final probes:

```json
{canvas_json}
```

- Iframes/ad/consent candidates:

```json
{iframe_json}
```

## No-Click Run

- We clicked Epic, Tiny, then Start through `WebView::notify_input_event`.
- Target clicks were intentionally disabled for this recon run.
- Result text:

```json
{result_text}
```

- Score/timer text:

```json
{score_text}
```

## Run Observations

{observation_summary}

## Unknowns From Section 2.4

- target technology: `{tech}`.
- hit event path: BLOCKED until M4/M5 calibration pages or a controlled M1 click probe; this no-click M1 proves option/start input only.
- target lifetime/animation curve: see run observations; exact per-target lifetime remains BLOCKED without stable target IDs.
- Epic spawn interval: see run observations.
- multiple target coexistence: see run observations.
- consent banner behavior: see iframe/body samples; none acted on automatically.
- ad slot/iframe behavior: see iframe/body samples; click safety still requires game-area exclusion later.

## Observation Sample

```json
{observation_evidence}
```

## Screenshots

{screenshots}

## Errors / Warnings

{errors}

## Raw Probe Files

{raw_probe_files}

SERVO_COMPAT: {compat}
"#,
    )
}

fn parse_json(raw: Option<&str>) -> Option<Value> {
    raw.and_then(|raw| serde_json::from_str(raw).ok())
}

fn json_value<'a>(value: Option<&'a Value>, pointer: &str) -> Option<&'a Value> {
    value.and_then(|value| value.pointer(pointer))
}

fn json_str<'a>(value: Option<&'a Value>, pointer: &str) -> Option<&'a str> {
    json_value(value, pointer).and_then(Value::as_str)
}

fn json_pretty(value: Option<&Value>) -> String {
    match value {
        Some(value) => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        None => "null".into(),
    }
}

fn controls_summary(value: Option<&Value>) -> String {
    let count = |name: &str| {
        value
            .and_then(|value| value.pointer(&format!("/controls/{name}")))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    format!(
        "Epic={} Tiny={} Start={}",
        count("epic"),
        count("tiny"),
        count("start")
    )
}

fn raw_probe_file_list(output_dir: &Path, result: &RealSiteRecon) -> String {
    let files = [
        (
            "initial probe",
            "initial_probe.json",
            result.initial_probe_json.as_ref(),
        ),
        (
            "after-options probe",
            "after_options_probe.json",
            result.after_options_probe_json.as_ref(),
        ),
        (
            "arm observation",
            "arm_observation.json",
            result.arm_observation_json.as_ref(),
        ),
        (
            "final probe",
            "final_probe.json",
            result.final_probe_json.as_ref(),
        ),
    ];
    let lines = files
        .into_iter()
        .filter_map(|(label, filename, raw)| {
            raw.map(|_| format!("- {label}: `{}`", output_dir.join(filename).display()))
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        "none".into()
    } else {
        lines.join("\n")
    }
}

fn observation_summary(observation: Option<&Value>) -> String {
    let Some(observation) = observation else {
        return [
            "- countdown-before-start: BLOCKED; no passive observation JSON was captured.",
            "- target spawn cadence: BLOCKED; no target mutation data was captured.",
            "- Tiny target size range: BLOCKED; no target rects were captured.",
            "- multiple target coexistence: BLOCKED; no target samples were captured.",
            "- target lifetime/animation curve: BLOCKED; no target samples were captured.",
        ]
        .join("\n");
    };

    let mutations = observation
        .get("mutations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let samples = observation
        .get("samples")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut target_add_times = mutations
        .iter()
        .filter(|entry| entry.get("kind").and_then(Value::as_str) == Some("added"))
        .filter(|entry| value_has_target_class(entry))
        .filter_map(|entry| entry.get("t").and_then(Value::as_f64))
        .collect::<Vec<_>>();
    target_add_times.sort_by(f64::total_cmp);

    let target_removals = mutations
        .iter()
        .filter(|entry| entry.get("kind").and_then(Value::as_str) == Some("removed"))
        .filter(|entry| value_has_target_class(entry))
        .count();

    let cadence = deltas(&target_add_times)
        .and_then(|mut values| {
            values.sort_by(f64::total_cmp);
            stats(&values)
        })
        .map(|stats| {
            format!(
                "median {} (min {}, max {}, avg {}) across {} gaps",
                fmt_ms(stats.median),
                fmt_ms(stats.min),
                fmt_ms(stats.max),
                fmt_ms(stats.avg),
                target_add_times.len().saturating_sub(1)
            )
        })
        .unwrap_or_else(|| "BLOCKED; fewer than two target additions were captured".into());

    let mut widths = Vec::new();
    let mut heights = Vec::new();
    for entry in mutations {
        if value_has_target_class(entry) {
            push_rect_size(entry.get("rect"), &mut widths, &mut heights);
        }
    }
    let mut max_concurrent = 0usize;
    let mut target_sample_times = Vec::new();
    let mut first_timer = None;
    let mut saw_time_up = false;
    for sample in samples {
        let targets = sample_targets(sample);
        max_concurrent = max_concurrent.max(targets.len());
        if !targets.is_empty() {
            if let Some(t) = sample.get("t").and_then(Value::as_f64) {
                target_sample_times.push(t);
            }
        }
        for target in targets {
            push_rect_size(target.get("rect"), &mut widths, &mut heights);
        }
        if let Some(lines) = sample.get("scoreText").and_then(Value::as_array) {
            for line in lines.iter().filter_map(Value::as_str) {
                if first_timer.is_none() && line.contains("Time Remaining") {
                    first_timer = Some(line.to_string());
                }
                if line.contains("Time is up") {
                    saw_time_up = true;
                }
            }
        }
    }

    let size_range = match (stats(&widths), stats(&heights)) {
        (Some(w), Some(h)) => format!(
            "width {}-{} CSS px; height {}-{} CSS px",
            fmt_css(w.min),
            fmt_css(w.max),
            fmt_css(h.min),
            fmt_css(h.max)
        ),
        _ => "BLOCKED; no target rect sizes were captured".into(),
    };

    let countdown = match first_timer {
        Some(line) => {
            format!("no separate countdown observed; first captured timer line was `{line}`")
        }
        None => "BLOCKED; no timer line was captured by the passive observer".into(),
    };

    target_sample_times.sort_by(f64::total_cmp);
    let lifetime = match (target_sample_times.first(), target_sample_times.last()) {
        (Some(first), Some(last)) if last > first => format!(
            "targets persisted concurrently for at least {}; exact per-target lifetime BLOCKED by anonymous DOM nodes in the no-click run",
            fmt_ms(last - first)
        ),
        _ if target_removals > 0 => format!(
            "{target_removals} target removal events observed; exact per-target lifetime BLOCKED by missing stable IDs"
        ),
        _ => "BLOCKED; no target lifetime span could be inferred".into(),
    };

    let result_screen = if saw_time_up {
        "observed `Time is up!` in passive samples"
    } else {
        "not seen in passive samples; final body text is the fallback evidence"
    };

    [
        format!("- countdown-before-start: {countdown}."),
        format!(
            "- target spawn cadence: {cadence}; captured {} target additions.",
            target_add_times.len()
        ),
        format!("- Tiny target size range: {size_range}."),
        format!("- multiple target coexistence: max {max_concurrent} visible target DOM nodes in passive samples."),
        format!("- target lifetime/animation curve: {lifetime}."),
        format!("- no-click result screen: {result_screen}."),
    ]
    .join("\n")
}

fn compact_observation_json(observation: Option<&Value>) -> String {
    let Some(observation) = observation else {
        return "null".into();
    };

    let mutations = observation
        .get("mutations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| value_has_target_class(entry))
        .take(8)
        .cloned()
        .collect::<Vec<_>>();
    let samples = observation
        .get("samples")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|sample| !sample_targets(sample).is_empty())
        .take(5)
        .cloned()
        .collect::<Vec<_>>();

    let compact = serde_json::json!({
        "armedAt": observation.get("armedAt").cloned().unwrap_or(Value::Null),
        "mutationCount": observation.get("mutations").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "sampleCount": observation.get("samples").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "droppedMutations": observation.get("droppedMutations").cloned().unwrap_or(Value::Null),
        "targetMutationSample": mutations,
        "targetSample": samples,
    });

    json_pretty(Some(&compact))
}

#[derive(Clone, Copy)]
struct Stat {
    min: f64,
    median: f64,
    max: f64,
    avg: f64,
}

fn stats(values: &[f64]) -> Option<Stat> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let sum = sorted.iter().sum::<f64>();
    Some(Stat {
        min: sorted[0],
        median: sorted[sorted.len() / 2],
        max: sorted[sorted.len() - 1],
        avg: sum / sorted.len() as f64,
    })
}

fn deltas(values: &[f64]) -> Option<Vec<f64>> {
    let deltas = values
        .windows(2)
        .filter_map(|pair| {
            let delta = pair[1] - pair[0];
            (delta > 0.0).then_some(delta)
        })
        .collect::<Vec<_>>();
    (!deltas.is_empty()).then_some(deltas)
}

fn value_has_target_class(value: &Value) -> bool {
    value
        .get("cls")
        .and_then(Value::as_str)
        .is_some_and(|class| class.split_whitespace().any(|part| part == "target"))
}

fn sample_targets(sample: &Value) -> Vec<&Value> {
    if let Some(targets) = sample.get("targets").and_then(Value::as_array) {
        return targets.iter().collect();
    }

    sample
        .get("smallElements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| value_has_target_class(entry))
        .collect()
}

fn push_rect_size(rect: Option<&Value>, widths: &mut Vec<f64>, heights: &mut Vec<f64>) {
    let Some(rect) = rect else {
        return;
    };
    if let Some(width) = rect.get("w").and_then(Value::as_f64) {
        widths.push(width);
    }
    if let Some(height) = rect.get("h").and_then(Value::as_f64) {
        heights.push(height);
    }
}

fn fmt_ms(value: f64) -> String {
    format!("{value:.0} ms")
}

fn fmt_css(value: f64) -> String {
    if value.fract().abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn classify_tech(
    initial: Option<&Value>,
    final_probe: Option<&Value>,
    observation: Option<&Value>,
) -> &'static str {
    let canvas_count = |value: Option<&Value>| {
        value
            .and_then(|value| value.get("canvases"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    let mutation_count = observation
        .and_then(|value| value.get("mutations"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    match (
        canvas_count(initial) + canvas_count(final_probe) > 0,
        mutation_count > 0,
    ) {
        (true, true) => "mixed",
        (true, false) => "canvas",
        (false, true) => "dom/svg",
        (false, false) => "unknown",
    }
}
