//! Target detection, fusion, and tracking on core frame types.

use std::collections::VecDeque;
use std::sync::Arc;

use saccade_core::{
    CssPoint, CssRect, DomRectObs, FrameObservation, GameFrameReport, PixelRegion, RenderedTarget,
    TargetCandidate, TargetEvidence, TargetId, TargetSource, TrackerEvent,
};

const CELL: usize = 8;
const ACTIVE_BLOCK: usize = 16;
const ACTIVE_SAMPLE_STEP: usize = 4;

pub trait TargetDetector {
    fn detect(&mut self, obs: &FrameObservation, cfg: &DetectConfig) -> Vec<TargetCandidate>;
    fn name(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct DetectConfig {
    pub enable_pixel: bool,
    pub enable_dom: bool,
    pub fg_threshold: u8,
    pub min_area_px: u32,
    pub max_radius_css: f32,
    pub min_fill_ratio: f32,
    pub min_contrast: f32,
    pub min_dom_size_css: f32,
    pub max_dom_size_css: f32,
}

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            enable_pixel: true,
            enable_dom: true,
            fg_threshold: 28,
            min_area_px: 4,
            max_radius_css: 16.0,
            min_fill_ratio: 0.55,
            min_contrast: 20.0,
            min_dom_size_css: 2.0,
            max_dom_size_css: 24.0,
        }
    }
}

#[derive(Debug, Clone)]
struct BackgroundModel {
    cells_w: usize,
    rgb: Vec<[u8; 3]>,
}

#[derive(Debug, Default)]
pub struct PixelDetector {
    background: Option<BackgroundModel>,
    stable_centers: Vec<(CssPoint, u32)>,
}

impl PixelDetector {
    pub fn reset_background(&mut self) {
        self.background = None;
        self.stable_centers.clear();
    }
}

impl TargetDetector for PixelDetector {
    fn detect(&mut self, obs: &FrameObservation, cfg: &DetectConfig) -> Vec<TargetCandidate> {
        if self.background.is_none() {
            self.background = Some(build_background(&obs.pixels));
            return Vec::new();
        }

        let Some(background) = self.background.as_ref().cloned() else {
            return Vec::new();
        };
        let w = obs.pixels.w as usize;
        let h = obs.pixels.h as usize;
        let rgba = obs.pixels.rgba.as_slice();
        let mut visited = vec![false; w * h];
        let mut candidates = Vec::new();
        let dsf = obs.viewport.device_scale_factor * obs.viewport.page_zoom;
        let max_area =
            (std::f32::consts::PI * (cfg.max_radius_css * dsf).powi(2) * 1.3).ceil() as u32;

        let active = active_blocks(w, h, rgba, &background, cfg.fg_threshold);
        for block_y in 0..active.blocks_h {
            for block_x in 0..active.blocks_w {
                if !active.is_active(block_x, block_y) {
                    continue;
                }

                let y0 = block_y * ACTIVE_BLOCK;
                let y1 = ((block_y + 1) * ACTIVE_BLOCK).min(h);
                let x0 = block_x * ACTIVE_BLOCK;
                let x1 = ((block_x + 1) * ACTIVE_BLOCK).min(w);

                for y in y0..y1 {
                    for x in x0..x1 {
                        let index = y * w + x;
                        if visited[index]
                            || !foreground(index, x, y, rgba, &background, cfg.fg_threshold)
                        {
                            continue;
                        }

                        let component = collect_component(
                            x,
                            y,
                            w,
                            h,
                            rgba,
                            &background,
                            cfg.fg_threshold,
                            &mut visited,
                        );
                        if component.area < cfg.min_area_px || component.area > max_area {
                            continue;
                        }

                        let width = (component.max_x - component.min_x + 1) as f32;
                        let height = (component.max_y - component.min_y + 1) as f32;
                        let aspect = width / height.max(1.0);
                        if !(0.6..=1.6).contains(&aspect) {
                            continue;
                        }

                        let radius_device = width.max(height) / 2.0;
                        let circle_area = std::f32::consts::PI * radius_device.powi(2);
                        let fill_ratio = component.area as f32 / circle_area.max(1.0);
                        if fill_ratio < cfg.min_fill_ratio {
                            continue;
                        }

                        let contrast = component.contrast_sum / component.area as f32;
                        if contrast < cfg.min_contrast {
                            continue;
                        }

                        let center_device = CssPoint {
                            x: component.sum_x / component.area as f32,
                            y: component.sum_y / component.area as f32,
                        };
                        let center_css = CssPoint {
                            x: obs.game_area_css.x + center_device.x / dsf,
                            y: obs.game_area_css.y + center_device.y / dsf,
                        };
                        let bbox_css = CssRect {
                            x: obs.game_area_css.x + component.min_x as f32 / dsf,
                            y: obs.game_area_css.y + component.min_y as f32 / dsf,
                            w: width / dsf,
                            h: height / dsf,
                        };
                        let fill_score = (fill_ratio / 0.85).min(1.0);
                        let contrast_score = (contrast / 80.0).min(1.0);
                        let size_score = (component.area as f32 / cfg.min_area_px as f32).min(1.0);
                        let mut confidence =
                            (0.4 * fill_score + 0.3 * contrast_score + 0.3 * size_score)
                                .clamp(0.0, 1.0);
                        if self.is_static_suspect(center_css) {
                            confidence *= 0.2;
                        }

                        candidates.push(TargetCandidate {
                            center_css,
                            bbox_css,
                            radius_css: radius_device / dsf,
                            source: TargetSource::PixelDetector,
                            confidence,
                            evidence: TargetEvidence::PixelComponent {
                                area_px: component.area,
                                fill_ratio,
                                contrast,
                                temporal_delta: contrast,
                            },
                        });
                    }
                }
            }
        }

        candidates.sort_by(|a, b| {
            a.center_css
                .y
                .total_cmp(&b.center_css.y)
                .then(a.center_css.x.total_cmp(&b.center_css.x))
        });
        candidates
    }

    fn name(&self) -> &'static str {
        "PixelDetector"
    }
}

impl PixelDetector {
    fn is_static_suspect(&mut self, center: CssPoint) -> bool {
        for (known, frames) in &mut self.stable_centers {
            if distance(*known, center) <= 1.0 {
                *frames += 1;
                return *frames >= 60;
            }
        }
        if self.stable_centers.len() < 64 {
            self.stable_centers.push((center, 1));
        }
        false
    }
}

#[derive(Debug, Default)]
pub struct DomRectDetector;

impl TargetDetector for DomRectDetector {
    fn detect(&mut self, obs: &FrameObservation, cfg: &DetectConfig) -> Vec<TargetCandidate> {
        obs.dom_rects
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|rect| dom_rect_candidate(rect, obs, cfg))
            .collect()
    }

    fn name(&self) -> &'static str {
        "DomRectDetector"
    }
}

fn dom_rect_candidate(
    rect: &DomRectObs,
    obs: &FrameObservation,
    cfg: &DetectConfig,
) -> Option<TargetCandidate> {
    if !rect.rect_css.inside(&obs.game_area_css) {
        return None;
    }
    let max_side = rect.rect_css.w.max(rect.rect_css.h);
    if max_side < cfg.min_dom_size_css || max_side > cfg.max_dom_size_css {
        return None;
    }
    Some(TargetCandidate {
        center_css: rect.rect_css.center(),
        bbox_css: rect.rect_css,
        radius_css: max_side / 2.0,
        source: TargetSource::DomRect,
        confidence: 0.95,
        evidence: TargetEvidence::DomBox {
            label: rect.label.clone(),
        },
    })
}

#[derive(Debug, Clone)]
pub struct FusionConfig {
    pub dedupe_distance_css: f32,
    pub min_confidence: f32,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            dedupe_distance_css: 8.0,
            min_confidence: 0.70,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Fusion {
    pub cfg: FusionConfig,
}

impl Fusion {
    pub fn fuse(&self, mut candidates: Vec<TargetCandidate>) -> Vec<TargetCandidate> {
        candidates.retain(|candidate| candidate.confidence >= self.cfg.min_confidence);
        candidates.sort_by(|a, b| {
            a.center_css
                .y
                .total_cmp(&b.center_css.y)
                .then(a.center_css.x.total_cmp(&b.center_css.x))
        });

        let mut groups: Vec<Vec<TargetCandidate>> = Vec::new();
        'candidate: for candidate in candidates {
            for group in &mut groups {
                if group.iter().any(|existing| {
                    distance(existing.center_css, candidate.center_css)
                        <= self
                            .cfg
                            .dedupe_distance_css
                            .max(existing.radius_css)
                            .max(candidate.radius_css)
                }) {
                    group.push(candidate);
                    continue 'candidate;
                }
            }
            groups.push(vec![candidate]);
        }

        let mut fused = groups.into_iter().map(fuse_group).collect::<Vec<_>>();
        fused.sort_by(|a, b| {
            a.center_css
                .y
                .total_cmp(&b.center_css.y)
                .then(a.center_css.x.total_cmp(&b.center_css.x))
        });
        fused
    }
}

fn fuse_group(group: Vec<TargetCandidate>) -> TargetCandidate {
    if group.len() == 1 {
        return group.into_iter().next().unwrap();
    }

    let mut weight = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut confidence_product = 1.0;
    let mut bbox = group[0].bbox_css;
    let mut radius: f32 = 0.0;
    let source = if group.iter().all(|item| item.source == group[0].source) {
        group[0].source
    } else {
        TargetSource::Fused
    };

    for candidate in &group {
        weight += candidate.confidence;
        x += candidate.center_css.x * candidate.confidence;
        y += candidate.center_css.y * candidate.confidence;
        confidence_product *= 1.0 - candidate.confidence;
        bbox = union_rect(bbox, candidate.bbox_css);
        radius = radius.max(candidate.radius_css);
    }

    TargetCandidate {
        center_css: CssPoint {
            x: x / weight.max(f32::EPSILON),
            y: y / weight.max(f32::EPSILON),
        },
        bbox_css: bbox,
        radius_css: radius,
        source,
        confidence: 1.0 - confidence_product,
        evidence: TargetEvidence::DomBox {
            label: "fused".into(),
        },
    }
}

#[derive(Debug, Clone)]
struct Track {
    target: RenderedTarget,
    missed_frames: u32,
}

#[derive(Debug, Clone)]
pub struct Tracker {
    next_id: u64,
    miss_frames: u32,
    tracks: Vec<Track>,
    events: Vec<TrackerEvent>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            next_id: 1,
            miss_frames: 2,
            tracks: Vec::new(),
            events: Vec::new(),
        }
    }
}

impl Tracker {
    pub fn update(
        &mut self,
        obs: &FrameObservation,
        candidates: Vec<TargetCandidate>,
        detector_ms: f32,
    ) -> GameFrameReport {
        self.events.clear();
        let mut matched = vec![false; self.tracks.len()];

        for candidate in candidates {
            let best = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(index, _)| !matched[*index])
                .filter_map(|(index, track)| {
                    let threshold = track.target.radius_css.max(candidate.radius_css);
                    let dist = distance(track.target.center_css, candidate.center_css);
                    (dist <= threshold.max(1.0)).then_some((index, dist))
                })
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(index, _)| index);

            if let Some(index) = best {
                matched[index] = true;
                let track = &mut self.tracks[index];
                track.missed_frames = 0;
                track.target.frame_id = obs.frame_id;
                track.target.last_seen_ns = obs.t_paint_ns;
                track.target.center_css = candidate.center_css;
                track.target.bbox_css = candidate.bbox_css;
                track.target.radius_css = candidate.radius_css;
                track.target.confidence = candidate.confidence;
                track.target.source = candidate.source;
                self.events.push(TrackerEvent::Updated {
                    target: track.target.clone(),
                });
            } else {
                let target = RenderedTarget {
                    id: TargetId(self.next_id),
                    frame_id: obs.frame_id,
                    first_seen_ns: obs.t_paint_ns,
                    last_seen_ns: obs.t_paint_ns,
                    center_css: candidate.center_css,
                    bbox_css: candidate.bbox_css,
                    radius_css: candidate.radius_css,
                    confidence: candidate.confidence,
                    source: candidate.source,
                    clicked: false,
                };
                self.next_id += 1;
                self.events.push(TrackerEvent::Appeared {
                    target: target.clone(),
                });
                self.tracks.push(Track {
                    target,
                    missed_frames: 0,
                });
                matched.push(true);
            }
        }

        for (index, track) in self.tracks.iter_mut().enumerate() {
            if !matched.get(index).copied().unwrap_or(false) {
                track.missed_frames += 1;
            }
        }

        let mut index = 0;
        while index < self.tracks.len() {
            if self.tracks[index].missed_frames > self.miss_frames {
                let target = self.tracks.remove(index).target;
                self.events.push(TrackerEvent::Disappeared {
                    target_id: target.id,
                    t_obs_ns: obs.t_paint_ns,
                });
            } else {
                index += 1;
            }
        }

        let mut targets = self
            .tracks
            .iter()
            .filter(|track| track.missed_frames == 0)
            .map(|track| track.target.clone())
            .collect::<Vec<_>>();
        targets.sort_by_key(|target| target.id.0);

        GameFrameReport {
            frame_id: obs.frame_id,
            t_report_ns: obs.t_readback_ns,
            game_area_css: obs.game_area_css,
            targets,
            detector_ms,
        }
    }

    pub fn mark_clicked(&mut self, target_id: TargetId) {
        if let Some(track) = self
            .tracks
            .iter_mut()
            .find(|track| track.target.id == target_id)
        {
            track.target.clicked = true;
        }
    }

    pub fn events(&self) -> &[TrackerEvent] {
        &self.events
    }
}

#[derive(Debug, Default)]
pub struct DetectionPipeline {
    pixel: PixelDetector,
    dom: DomRectDetector,
    fusion: Fusion,
    tracker: Tracker,
}

impl DetectionPipeline {
    pub fn on_frame(&mut self, obs: &FrameObservation, cfg: &DetectConfig) -> GameFrameReport {
        let start = std::time::Instant::now();
        let mut candidates = if cfg.enable_pixel {
            self.pixel.detect(obs, cfg)
        } else {
            Vec::new()
        };
        if cfg.enable_dom {
            candidates.extend(self.dom.detect(obs, cfg));
        }
        let candidates = self.fusion.fuse(candidates);
        let detector_ms = start.elapsed().as_secs_f32() * 1000.0;
        self.tracker.update(obs, candidates, detector_ms)
    }

    pub fn mark_clicked(&mut self, target_id: TargetId) {
        self.tracker.mark_clicked(target_id);
    }

    pub fn events(&self) -> &[TrackerEvent] {
        self.tracker.events()
    }
}

#[derive(Debug, Clone, Copy)]
struct Component {
    area: u32,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    sum_x: f32,
    sum_y: f32,
    contrast_sum: f32,
}

#[derive(Debug, Clone)]
struct ActiveBlocks {
    blocks_w: usize,
    blocks_h: usize,
    active: Vec<bool>,
}

impl ActiveBlocks {
    fn is_active(&self, x: usize, y: usize) -> bool {
        self.active[y * self.blocks_w + x]
    }

    fn mark_with_neighbors(&mut self, x: usize, y: usize) {
        let x0 = x.saturating_sub(1);
        let y0 = y.saturating_sub(1);
        let x1 = (x + 1).min(self.blocks_w - 1);
        let y1 = (y + 1).min(self.blocks_h - 1);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                self.active[by * self.blocks_w + bx] = true;
            }
        }
    }
}

fn active_blocks(
    w: usize,
    h: usize,
    rgba: &[u8],
    background: &BackgroundModel,
    threshold: u8,
) -> ActiveBlocks {
    let blocks_w = w.div_ceil(ACTIVE_BLOCK);
    let blocks_h = h.div_ceil(ACTIVE_BLOCK);
    let mut blocks = ActiveBlocks {
        blocks_w,
        blocks_h,
        active: vec![false; blocks_w * blocks_h],
    };

    for by in 0..blocks_h {
        for bx in 0..blocks_w {
            let y0 = by * ACTIVE_BLOCK;
            let y1 = ((by + 1) * ACTIVE_BLOCK).min(h);
            let x0 = bx * ACTIVE_BLOCK;
            let x1 = ((bx + 1) * ACTIVE_BLOCK).min(w);
            let mut found = false;
            let mut y = y0;
            while y < y1 && !found {
                let mut x = x0;
                while x < x1 {
                    let index = y * w + x;
                    if foreground(index, x, y, rgba, background, threshold) {
                        found = true;
                        break;
                    }
                    x += ACTIVE_SAMPLE_STEP;
                }
                y += ACTIVE_SAMPLE_STEP;
            }
            if found {
                blocks.mark_with_neighbors(bx, by);
            }
        }
    }

    blocks
}

fn build_background(pixels: &PixelRegion) -> BackgroundModel {
    let w = pixels.w as usize;
    let h = pixels.h as usize;
    let cells_w = w.div_ceil(CELL);
    let cells_h = h.div_ceil(CELL);
    let mut sums = vec![[0u32; 4]; cells_w * cells_h];

    for y in 0..h {
        for x in 0..w {
            let cell = (y / CELL) * cells_w + x / CELL;
            let index = (y * w + x) * 4;
            sums[cell][0] += pixels.rgba[index] as u32;
            sums[cell][1] += pixels.rgba[index + 1] as u32;
            sums[cell][2] += pixels.rgba[index + 2] as u32;
            sums[cell][3] += 1;
        }
    }

    let rgb = sums
        .into_iter()
        .map(|sum| {
            let count = sum[3].max(1);
            [
                (sum[0] / count) as u8,
                (sum[1] / count) as u8,
                (sum[2] / count) as u8,
            ]
        })
        .collect();

    BackgroundModel { cells_w, rgb }
}

fn collect_component(
    start_x: usize,
    start_y: usize,
    w: usize,
    h: usize,
    rgba: &[u8],
    background: &BackgroundModel,
    threshold: u8,
    visited: &mut [bool],
) -> Component {
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited[start_y * w + start_x] = true;

    let mut component = Component {
        area: 0,
        min_x: start_x,
        min_y: start_y,
        max_x: start_x,
        max_y: start_y,
        sum_x: 0.0,
        sum_y: 0.0,
        contrast_sum: 0.0,
    };

    while let Some((x, y)) = queue.pop_front() {
        let index = y * w + x;
        component.area += 1;
        component.min_x = component.min_x.min(x);
        component.min_y = component.min_y.min(y);
        component.max_x = component.max_x.max(x);
        component.max_y = component.max_y.max(y);
        component.sum_x += x as f32 + 0.5;
        component.sum_y += y as f32 + 0.5;
        component.contrast_sum += pixel_delta(index, x, y, rgba, background) as f32;

        for (nx, ny) in neighbors(x, y, w, h) {
            let nindex = ny * w + nx;
            if !visited[nindex] && foreground(nindex, nx, ny, rgba, background, threshold) {
                visited[nindex] = true;
                queue.push_back((nx, ny));
            }
        }
    }

    component
}

fn neighbors(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut out = [(usize::MAX, usize::MAX); 4];
    let mut len = 0;
    if x > 0 {
        out[len] = (x - 1, y);
        len += 1;
    }
    if x + 1 < w {
        out[len] = (x + 1, y);
        len += 1;
    }
    if y > 0 {
        out[len] = (x, y - 1);
        len += 1;
    }
    if y + 1 < h {
        out[len] = (x, y + 1);
        len += 1;
    }
    out.into_iter().take(len)
}

fn foreground(
    index: usize,
    x: usize,
    y: usize,
    rgba: &[u8],
    background: &BackgroundModel,
    threshold: u8,
) -> bool {
    pixel_delta(index, x, y, rgba, background) > threshold
}

fn pixel_delta(index: usize, x: usize, y: usize, rgba: &[u8], background: &BackgroundModel) -> u8 {
    let rgb = background.rgb[(y / CELL) * background.cells_w + x / CELL];
    let offset = index * 4;
    let dr = rgba[offset].abs_diff(rgb[0]);
    let dg = rgba[offset + 1].abs_diff(rgb[1]);
    let db = rgba[offset + 2].abs_diff(rgb[2]);
    dr.max(dg).max(db)
}

fn distance(a: CssPoint, b: CssPoint) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn union_rect(a: CssRect, b: CssRect) -> CssRect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    CssRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

pub fn solid_frame(w: u32, h: u32, rgba: [u8; 4]) -> PixelRegion {
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    for _ in 0..w as usize * h as usize {
        pixels.extend_from_slice(&rgba);
    }
    PixelRegion {
        w,
        h,
        rgba: Arc::new(pixels),
    }
}

#[cfg(test)]
mod synthetic {
    use std::sync::Arc;
    use std::time::Instant;

    use saccade_core::{
        ClickOutcome, ClickReceipt, CssPoint, CssRect, DomRectObs, FrameObservation,
        GameFrameReport, InputBackendKind, MotorAction, RenderedTarget, ScoreState, TargetId,
        TargetSource, ViewportInfo,
    };
    use saccade_motor::MotorController;
    use saccade_verify::Verifier;

    use super::*;

    const W: u32 = 1280;
    const H: u32 = 600;
    const FRAME_NS: u64 = 16_000_000;

    #[test]
    fn detects_single_disc_center_within_half_px() {
        let mut detector = PixelDetector::default();
        let cfg = DetectConfig::default();
        let game = game_area();
        let blank = frame(1, &[], None, game);
        assert!(detector.detect(&blank, &cfg).is_empty());

        let expected = CssPoint { x: 120.0, y: 90.0 };
        let obs = frame(2, &[Disc::new(expected, 7.0)], None, game);
        let candidates = detector.detect(&obs, &cfg);

        assert_eq!(candidates.len(), 1);
        let actual = candidates[0].center_css;
        assert!((actual.x - expected.x).abs() <= 0.5, "{actual:?}");
        assert!((actual.y - expected.y).abs() <= 0.5, "{actual:?}");
    }

    #[test]
    fn tracks_growing_disc_as_same_target() {
        let mut pipeline = DetectionPipeline::default();
        let cfg = DetectConfig::default();
        let game = game_area();
        let center = CssPoint { x: 220.0, y: 180.0 };

        let _ = pipeline.on_frame(&frame(1, &[], None, game), &cfg);
        let first = pipeline.on_frame(&frame(2, &[Disc::new(center, 4.0)], None, game), &cfg);
        let second = pipeline.on_frame(&frame(3, &[Disc::new(center, 9.0)], None, game), &cfg);

        assert_eq!(first.targets.len(), 1);
        assert_eq!(second.targets.len(), 1);
        assert_eq!(first.targets[0].id, second.targets[0].id);
    }

    #[test]
    fn one_click_per_target() {
        let mut motor = MotorController::default();
        let target = target(TargetId(1), 1, 0, CssPoint { x: 80.0, y: 80.0 }, 0.95);
        let report = report(1, 1_000_000, vec![target.clone()], game_area());

        assert!(matches!(
            motor.on_frame(&report, 2_000_000),
            MotorAction::Click {
                target_id: TargetId(1),
                ..
            }
        ));
        assert!(matches!(
            motor.on_frame(&report, 20_000_000),
            MotorAction::Noop { .. }
        ));
    }

    #[test]
    fn multi_target_oldest_first() {
        let mut motor = MotorController::default();
        let young = target(
            TargetId(2),
            10,
            20_000_000,
            CssPoint { x: 200.0, y: 100.0 },
            0.95,
        );
        let old = target(
            TargetId(1),
            10,
            5_000_000,
            CssPoint { x: 100.0, y: 100.0 },
            0.95,
        );
        let report = report(10, 30_000_000, vec![young, old], game_area());

        assert!(matches!(
            motor.on_frame(&report, 31_000_000),
            MotorAction::Click {
                target_id: TargetId(1),
                ..
            }
        ));
    }

    #[test]
    fn stale_frame_rejected() {
        let mut motor = MotorController::default();
        let target = target(TargetId(1), 1, 0, CssPoint { x: 80.0, y: 80.0 }, 0.95);
        let report = report(1, 1_000_000, vec![target], game_area());

        let action = motor.on_frame(&report, 100_000_000);
        assert!(matches!(action, MotorAction::Noop { .. }));
    }

    #[test]
    fn no_click_outside_game_area() {
        let mut motor = MotorController::default();
        let game = CssRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let target = target(TargetId(1), 1, 0, CssPoint { x: 150.0, y: 80.0 }, 0.95);
        let report = report(1, 1_000_000, vec![target], game);

        let action = motor.on_frame(&report, 2_000_000);
        assert!(matches!(action, MotorAction::Noop { .. }));
    }

    #[test]
    fn miss_counter_triggers_conservative_mode() {
        let mut verifier = Verifier::default();
        let mut motor = MotorController::default();
        let receipt = receipt(TargetId(1), CssPoint { x: 40.0, y: 40.0 }, 10_000_000);
        verifier.add_click(receipt);
        let initial = ScoreState {
            hits: 0,
            misses: 0,
            time_remaining_s: Some(14.0),
            finished: false,
            t_obs_ns: 11_000_000,
        };
        assert!(
            verifier
                .on_frame(&[], Some(&initial), 11_000_000)
                .is_empty()
        );
        let missed = ScoreState {
            misses: 1,
            t_obs_ns: 12_000_000,
            ..initial
        };
        let results = verifier.on_frame(&[], Some(&missed), 12_000_000);
        assert_eq!(results[0].outcome, ClickOutcome::Miss);

        motor.note_miss_confirmed(12_000_000);
        assert!(motor.is_conservative(13_000_000));
        let pixel_target = target(
            TargetId(2),
            2,
            12_000_000,
            CssPoint { x: 80.0, y: 80.0 },
            0.95,
        );
        let report = report(2, 13_000_000, vec![pixel_target], game_area());
        assert!(matches!(
            motor.on_frame(&report, 14_000_000),
            MotorAction::Noop { .. }
        ));
    }

    #[test]
    fn verifies_hit_by_disappearance() {
        let mut verifier = Verifier::default();
        let receipt = receipt(TargetId(7), CssPoint { x: 55.0, y: 55.0 }, 10_000_000);
        verifier.add_click(receipt);
        let events = [saccade_verify::target_disappeared(TargetId(7), 20_000_000)];
        let results = verifier.on_frame(&events, None, 20_000_000);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, ClickOutcome::Hit);
        assert_eq!(results[0].target_id, TargetId(7));
    }

    #[test]
    fn pixel_detector_1280x600_detects_under_three_ms() {
        let mut detector = PixelDetector::default();
        let cfg = DetectConfig::default();
        let game = game_area();
        let _ = detector.detect(&frame(1, &[], None, game), &cfg);
        let obs = frame(
            2,
            &[Disc::new(CssPoint { x: 640.0, y: 300.0 }, 7.0)],
            None,
            game,
        );

        let start = Instant::now();
        let candidates = detector.detect(&obs, &cfg);
        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_secs_f32() * 1000.0;
        eprintln!("pixel detect 1280x600 synthetic: {elapsed_ms:.3} ms");

        assert_eq!(candidates.len(), 1);
        assert!(elapsed_ms <= 3.0, "detect took {elapsed_ms:.3} ms");
    }

    #[test]
    fn full_synthetic_detect_track_motor_verify_loop_hits() {
        let mut pipeline = DetectionPipeline::default();
        let cfg = DetectConfig::default();
        let game = game_area();
        let center = CssPoint { x: 500.0, y: 300.0 };
        let _ = pipeline.on_frame(&frame(1, &[], None, game), &cfg);
        let report = pipeline.on_frame(&frame(2, &[Disc::new(center, 7.0)], None, game), &cfg);
        let mut motor = MotorController::default();
        let action = motor.on_frame(&report, report.t_report_ns + 1_000_000);
        let MotorAction::Click {
            target_id,
            point_css,
            frame_id,
        } = action
        else {
            panic!("expected click");
        };
        pipeline.mark_clicked(target_id);

        let mut verifier = Verifier::default();
        verifier.add_click(ClickReceipt {
            click_id: 1,
            target_id,
            point_css,
            frame_id,
            t_target_first_seen_ns: report.targets[0].first_seen_ns,
            t_decided_ns: report.t_report_ns + 1_000_000,
            t_move_sent_ns: report.t_report_ns + 1_100_000,
            t_down_sent_ns: report.t_report_ns + 1_200_000,
            t_up_sent_ns: report.t_report_ns + 1_300_000,
            backend: InputBackendKind::ServoInternal,
        });

        let _ = pipeline.on_frame(&frame(3, &[], None, game), &cfg);
        let _ = pipeline.on_frame(&frame(4, &[], None, game), &cfg);
        let _ = pipeline.on_frame(&frame(5, &[], None, game), &cfg);
        let results = verifier.on_frame(pipeline.events(), None, 80_000_000);
        assert!(
            results
                .iter()
                .any(|result| result.outcome == ClickOutcome::Hit)
        );
    }

    #[derive(Debug, Clone, Copy)]
    struct Disc {
        center: CssPoint,
        radius: f32,
    }

    impl Disc {
        fn new(center: CssPoint, radius: f32) -> Self {
            Self { center, radius }
        }
    }

    fn game_area() -> CssRect {
        CssRect {
            x: 0.0,
            y: 0.0,
            w: W as f32,
            h: H as f32,
        }
    }

    fn frame(
        frame_id: u64,
        discs: &[Disc],
        dom_rects: Option<Vec<DomRectObs>>,
        game_area_css: CssRect,
    ) -> FrameObservation {
        let pixels = render(discs);
        FrameObservation {
            frame_id,
            t_paint_ns: frame_id * FRAME_NS,
            t_readback_ns: frame_id * FRAME_NS + 1_000_000,
            viewport: ViewportInfo {
                width_css: W as f32,
                height_css: H as f32,
                device_scale_factor: 1.0,
                page_zoom: 1.0,
            },
            game_area_css,
            pixels,
            dom_rects,
        }
    }

    fn render(discs: &[Disc]) -> PixelRegion {
        let mut pixels = vec![24u8; W as usize * H as usize * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
        for disc in discs {
            let min_x = (disc.center.x - disc.radius).floor().max(0.0) as i32;
            let max_x = (disc.center.x + disc.radius).ceil().min(W as f32 - 1.0) as i32;
            let min_y = (disc.center.y - disc.radius).floor().max(0.0) as i32;
            let max_y = (disc.center.y + disc.radius).ceil().min(H as f32 - 1.0) as i32;
            let r2 = disc.radius * disc.radius;
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let dx = x as f32 + 0.5 - disc.center.x;
                    let dy = y as f32 + 0.5 - disc.center.y;
                    if dx * dx + dy * dy <= r2 {
                        let index = (y as usize * W as usize + x as usize) * 4;
                        pixels[index] = 250;
                        pixels[index + 1] = 40;
                        pixels[index + 2] = 40;
                        pixels[index + 3] = 255;
                    }
                }
            }
        }
        PixelRegion {
            w: W,
            h: H,
            rgba: Arc::new(pixels),
        }
    }

    fn target(
        id: TargetId,
        frame_id: u64,
        first_seen_ns: u64,
        center: CssPoint,
        confidence: f32,
    ) -> RenderedTarget {
        RenderedTarget {
            id,
            frame_id,
            first_seen_ns,
            last_seen_ns: frame_id * FRAME_NS,
            center_css: center,
            bbox_css: CssRect {
                x: center.x - 7.0,
                y: center.y - 7.0,
                w: 14.0,
                h: 14.0,
            },
            radius_css: 7.0,
            confidence,
            source: TargetSource::PixelDetector,
            clicked: false,
        }
    }

    fn report(
        frame_id: u64,
        t_report_ns: u64,
        targets: Vec<RenderedTarget>,
        game_area_css: CssRect,
    ) -> GameFrameReport {
        GameFrameReport {
            frame_id,
            t_report_ns,
            game_area_css,
            targets,
            detector_ms: 0.5,
        }
    }

    fn receipt(target_id: TargetId, point_css: CssPoint, t_up_sent_ns: u64) -> ClickReceipt {
        ClickReceipt {
            click_id: target_id.0,
            target_id,
            point_css,
            frame_id: 1,
            t_target_first_seen_ns: t_up_sent_ns - 10_000_000,
            t_decided_ns: t_up_sent_ns - 2_000_000,
            t_move_sent_ns: t_up_sent_ns - 1_000_000,
            t_down_sent_ns: t_up_sent_ns - 500_000,
            t_up_sent_ns,
            backend: InputBackendKind::ServoInternal,
        }
    }
}
