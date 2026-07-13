"""Human-visible page and agent action-map agreement analysis.

The analyzer consumes redacted truth/action maps. Screenshots are optional and
are reduced to geometry/pixel metrics; page pixels are never embedded in the
JSON report.
"""

from __future__ import annotations

from collections import Counter, defaultdict
from math import hypot
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "saccade.human_agent_agreement/1"


def analyze_agreement(
    reference_truth: dict[str, Any],
    observed_truth: dict[str, Any],
    *,
    hit_test: dict[str, Any] | None = None,
    visual_metrics: dict[str, Any] | None = None,
    strict_visual: bool = False,
    max_click_escape_px: float = 8.0,
    max_center_delta_px: float = 24.0,
) -> dict[str, Any]:
    reference_actions = extract_actions(reference_truth)
    observed_actions = extract_actions(observed_truth)
    reference_visible = [action for action in reference_actions if is_visible_fact(action)]
    observed_visible = [action for action in observed_actions if is_visible_fact(action)]
    reference_actionable = [action for action in reference_visible if is_actionable(action)]
    observed_actionable = [action for action in observed_visible if is_actionable(action)]

    fact_pairs, missing_facts, extra_facts = pair_actions(reference_visible, observed_visible)
    action_pairs, missing_actions, extra_actions = pair_actions(
        reference_actionable, observed_actionable
    )

    hidden_contamination = [
        action
        for action in observed_actions
        if action.get("enabled") is True and not is_visible_fact(action)
    ]
    duplicate_count, duplicate_keys = duplicate_contamination(
        reference_visible, observed_visible
    )
    geometry = geometry_metrics(action_pairs)
    hit_metrics = hit_test_metrics(hit_test)
    hit_results = hit_test_results_by_action_id(hit_test)
    observation_base = compare_observation_base(reference_truth, observed_truth)
    visual = normalize_visual_metrics(visual_metrics)

    metrics = {
        "reference_visible_controls": len(reference_visible),
        "observed_visible_facts": len(observed_visible),
        "reference_actionable_controls": len(reference_actionable),
        "observed_actionable_controls": len(observed_actionable),
        "matched_visible_controls": len(fact_pairs),
        "matched_actionable_controls": len(action_pairs),
        "visible_control_recall": ratio(len(fact_pairs), len(reference_visible)),
        "actionable_precision": ratio(len(action_pairs), len(observed_actionable)),
        "missing_visible_controls": len(missing_facts),
        "extra_visible_facts": len(extra_facts),
        "missing_actionable_controls": len(missing_actions),
        "extra_actionable_controls": len(extra_actions),
        "duplicate_contamination": duplicate_count,
        "hidden_contamination": len(hidden_contamination),
        "geometry": geometry,
        "hit_test": hit_metrics,
        "observation_base": observation_base,
        "visual": visual,
    }

    reasons: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    if missing_facts:
        add_reason(
            reasons,
            "AGREEMENT_MISSING_VISIBLE_FACT",
            f"{len(missing_facts)} user-visible control(s) are absent from agent truth",
            len(missing_facts),
        )
    if extra_facts:
        add_reason(
            reasons,
            "AGREEMENT_EXTRA_VISIBLE_FACT",
            f"{len(extra_facts)} visible agent fact(s) have no user-visible reference control",
            len(extra_facts),
        )
    if missing_actions:
        add_reason(
            reasons,
            "AGREEMENT_ACTIONABILITY_MISMATCH",
            f"{len(missing_actions)} user-actionable control(s) are not actionable in agent truth",
            len(missing_actions),
        )
    if extra_actions:
        add_reason(
            reasons,
            "AGREEMENT_EXTRA_ACTION",
            f"{len(extra_actions)} exported action(s) have no visible reference control",
            len(extra_actions),
        )
    if hidden_contamination:
        add_reason(
            reasons,
            "AGREEMENT_HIDDEN_ACTION",
            f"{len(hidden_contamination)} hidden or zero-rect control(s) are enabled",
            len(hidden_contamination),
        )
    if duplicate_count:
        add_reason(
            reasons,
            "AGREEMENT_DUPLICATE_ACTION",
            f"{duplicate_count} duplicate fact/action(s) exceed the visible reference",
            duplicate_count,
        )
    if geometry["max_click_escape_px"] > max_click_escape_px:
        add_reason(
            reasons,
            "AGREEMENT_GEOMETRY_ESCAPE",
            f"agent click center escapes its reference control by {geometry['max_click_escape_px']} px",
            geometry["max_click_escape_px"],
        )
    elif geometry["max_center_delta_px"] > max_center_delta_px:
        add_reason(
            reasons,
            "AGREEMENT_GEOMETRY_DRIFT",
            f"action center differs from the reference by {geometry['max_center_delta_px']} px",
            geometry["max_center_delta_px"],
        )
    if hit_metrics["available"] and hit_metrics["failed"]:
        add_reason(
            reasons,
            "AGREEMENT_HIT_TEST_MISMATCH",
            f"{hit_metrics['failed']} proposed action point(s) hit the wrong target",
            hit_metrics["failed"],
        )
    for mismatch in observation_base["mismatches"]:
        add_reason(
            reasons,
            mismatch["code"],
            mismatch["message"],
            mismatch.get("value"),
        )
    if visual["available"]:
        if visual["dimension_match"] is False:
            add_reason(
                reasons,
                "AGREEMENT_SCREENSHOT_DIMENSION",
                "reference and observed screenshot dimensions differ",
            )
        if visual.get("observed_nonblank") is False:
            add_reason(
                reasons,
                "AGREEMENT_SCREENSHOT_BLANK",
                "observed screenshot is blank or flat",
            )
        diff_ratio = visual.get("diff_ratio")
        if diff_ratio is not None and diff_ratio > 0.08:
            target = reasons if strict_visual else warnings
            add_reason(
                target,
                "AGREEMENT_VISUAL_DIFF",
                f"visual diff ratio is {diff_ratio:.3%}; inspect rendering before trusting appearance",
                diff_ratio,
            )

    reference_inventory_supplied = action_inventory_declared(reference_truth)
    observed_inventory_supplied = action_inventory_declared(observed_truth)
    evidence_complete = reference_inventory_supplied and observed_inventory_supplied
    if not reference_inventory_supplied or not observed_inventory_supplied:
        add_reason(
            reasons,
            "AGREEMENT_ACTION_INVENTORY_MISSING",
            "reference and observed evidence must each declare an actions inventory",
        )
    if not hit_metrics["available"]:
        add_reason(
            warnings,
            "AGREEMENT_HIT_TEST_UNMEASURED",
            "no native hit-test evidence was supplied",
        )
    if not visual["available"]:
        add_reason(
            warnings,
            "AGREEMENT_VISUAL_UNMEASURED",
            "no screenshot metrics were supplied",
        )

    if not evidence_complete:
        verdict = "INCOMPLETE_EVIDENCE"
        recommended_route = "block"
    elif reasons:
        verdict = "ROUTE_COMPATIBILITY"
        recommended_route = "compatibility_or_block"
    elif warnings:
        verdict = "PASS_WITH_WARNINGS"
        recommended_route = "default_with_review"
    else:
        verdict = "PASS_ACTION_GREEN"
        recommended_route = "default"

    return {
        "schema_version": SCHEMA_VERSION,
        "ok": not reasons and evidence_complete,
        "verdict": verdict,
        "recommended_route": recommended_route,
        "evidence_complete": evidence_complete,
        "evidence_coverage": {
            "reference_action_inventory": reference_inventory_supplied,
            "observed_action_inventory": observed_inventory_supplied,
            "native_hit_test": hit_metrics["available"],
            "screenshot_metrics": visual["available"],
        },
        "metrics": metrics,
        "reasons": reasons,
        "warnings": warnings,
        "safe_examples": {
            "missing_visible": summarize_actions(missing_facts),
            "extra_actionable": summarize_actions(extra_actions),
            "duplicate_keys": duplicate_keys[:10],
            "hidden_enabled": summarize_actions(hidden_contamination),
        },
        "matches": [
            match_summary(
                pair,
                hit_results,
                max_click_escape_px=max_click_escape_px,
                max_center_delta_px=max_center_delta_px,
            )
            for pair in fact_pairs
        ],
        "privacy": {
            "field_values_included": False,
            "screenshot_pixels_included": False,
            "labels_limited_to_redacted_truth": True,
        },
    }


def extract_actions(payload: dict[str, Any] | None) -> list[dict[str, Any]]:
    if not isinstance(payload, dict):
        return []
    direct = payload.get("actions")
    if isinstance(direct, list):
        return [item for item in direct if isinstance(item, dict)]
    for key in ("truth", "result", "snapshot"):
        nested = payload.get(key)
        if isinstance(nested, dict):
            actions = extract_actions(nested)
            if actions:
                return actions
    return []


def action_inventory_declared(payload: dict[str, Any] | None) -> bool:
    if not isinstance(payload, dict):
        return False
    if isinstance(payload.get("actions"), list):
        return True
    for key in ("truth", "result", "snapshot"):
        nested = payload.get(key)
        if isinstance(nested, dict) and action_inventory_declared(nested):
            return True
    return False


def extract_viewport(payload: dict[str, Any] | None) -> dict[str, Any] | None:
    if not isinstance(payload, dict):
        return None
    viewport = payload.get("viewport")
    if isinstance(viewport, dict):
        return viewport
    for key in ("truth", "result", "snapshot"):
        nested = payload.get(key)
        viewport = extract_viewport(nested) if isinstance(nested, dict) else None
        if viewport:
            return viewport
    return None


def extract_revision(payload: dict[str, Any] | None) -> int | str | None:
    if not isinstance(payload, dict):
        return None
    for key in ("page_revision", "dom_page_revision", "revision"):
        value = payload.get(key)
        if isinstance(value, (int, str)):
            return value
    for key in ("truth", "result", "snapshot"):
        nested = payload.get(key)
        value = extract_revision(nested) if isinstance(nested, dict) else None
        if value is not None:
            return value
    return None


def compare_observation_base(
    reference_truth: dict[str, Any], observed_truth: dict[str, Any]
) -> dict[str, Any]:
    mismatches: list[dict[str, Any]] = []
    reference_viewport = extract_viewport(reference_truth)
    observed_viewport = extract_viewport(observed_truth)
    if reference_viewport and observed_viewport:
        for key in ("width", "height"):
            reference_value = number(reference_viewport.get(key))
            observed_value = number(observed_viewport.get(key))
            if reference_value is not None and observed_value is not None and abs(reference_value - observed_value) > 1:
                mismatches.append(
                    {
                        "code": "AGREEMENT_VIEWPORT_MISMATCH",
                        "message": f"viewport {key} differs: reference={reference_value}, observed={observed_value}",
                        "value": abs(reference_value - observed_value),
                    }
                )
        reference_dpr = number(
            reference_viewport.get("devicePixelRatio", reference_viewport.get("device_scale_factor"))
        )
        observed_dpr = number(
            observed_viewport.get("devicePixelRatio", observed_viewport.get("device_scale_factor"))
        )
        if reference_dpr is not None and observed_dpr is not None and abs(reference_dpr - observed_dpr) > 0.01:
            mismatches.append(
                {
                    "code": "AGREEMENT_DPR_MISMATCH",
                    "message": f"device pixel ratio differs: reference={reference_dpr}, observed={observed_dpr}",
                    "value": abs(reference_dpr - observed_dpr),
                }
            )

    reference_url = extract_string(reference_truth, "url")
    observed_url = extract_string(observed_truth, "url")
    if reference_url and observed_url and reference_url != observed_url:
        mismatches.append(
            {
                "code": "AGREEMENT_URL_MISMATCH",
                "message": "reference and observed truth describe different URLs",
            }
        )

    reference_revision = extract_revision(reference_truth)
    observed_revision = extract_revision(observed_truth)
    if (
        reference_revision is not None
        and observed_revision is not None
        and reference_revision != observed_revision
    ):
        mismatches.append(
            {
                "code": "AGREEMENT_REVISION_MISMATCH",
                "message": f"revision differs: reference={reference_revision}, observed={observed_revision}",
            }
        )
    return {
        "reference_viewport": safe_viewport(reference_viewport),
        "observed_viewport": safe_viewport(observed_viewport),
        "reference_revision": reference_revision,
        "observed_revision": observed_revision,
        "same_url": None if not reference_url or not observed_url else reference_url == observed_url,
        "mismatches": mismatches,
    }


def pair_actions(
    reference_actions: list[dict[str, Any]], observed_actions: list[dict[str, Any]]
) -> tuple[
    list[tuple[tuple[int, dict[str, Any]], tuple[int, dict[str, Any]]]],
    list[tuple[int, dict[str, Any]]],
    list[tuple[int, dict[str, Any]]],
]:
    reference_groups: dict[tuple[str, str], list[tuple[int, dict[str, Any]]]] = defaultdict(list)
    observed_groups: dict[tuple[str, str], list[tuple[int, dict[str, Any]]]] = defaultdict(list)
    for index, action in enumerate(reference_actions):
        reference_groups[action_key(action)].append((index, action))
    for index, action in enumerate(observed_actions):
        observed_groups[action_key(action)].append((index, action))

    pairs = []
    missing = []
    extra = []
    for key in sorted(set(reference_groups) | set(observed_groups)):
        references = list(reference_groups.get(key, []))
        observed = list(observed_groups.get(key, []))
        while references and observed:
            best = min(
                (
                    center_distance(reference[1], candidate[1]),
                    reference_index,
                    observed_index,
                )
                for reference_index, reference in enumerate(references)
                for observed_index, candidate in enumerate(observed)
            )
            _, reference_index, observed_index = best
            pairs.append((references.pop(reference_index), observed.pop(observed_index)))
        missing.extend(references)
        extra.extend(observed)
    return pairs, missing, extra


def duplicate_contamination(
    reference_actions: list[dict[str, Any]], observed_actions: list[dict[str, Any]]
) -> tuple[int, list[dict[str, Any]]]:
    reference_counts = Counter(action_key(action) for action in reference_actions)
    observed_counts = Counter(action_key(action) for action in observed_actions)
    duplicates = []
    total = 0
    for key, count in observed_counts.items():
        extra = max(0, count - reference_counts.get(key, 0))
        if extra:
            total += extra
            duplicates.append({"key": list(key), "extra": extra})
    return total, sorted(duplicates, key=lambda item: item["key"])


def geometry_metrics(
    pairs: list[tuple[tuple[int, dict[str, Any]], tuple[int, dict[str, Any]]]]
) -> dict[str, Any]:
    max_center_delta = 0.0
    max_rect_delta = 0.0
    max_click_escape = 0.0
    for reference, observed in pairs:
        reference_rect = rect_of(reference[1])
        observed_rect = rect_of(observed[1])
        max_center_delta = max(
            max_center_delta, center_distance(reference[1], observed[1])
        )
        max_rect_delta = max(max_rect_delta, rect_max_delta(reference_rect, observed_rect))
        max_click_escape = max(
            max_click_escape,
            point_escape_distance(rect_center(observed_rect), reference_rect),
        )
    return {
        "paired": len(pairs),
        "max_center_delta_px": round(max_center_delta, 3),
        "max_rect_delta_px": round(max_rect_delta, 3),
        "max_click_escape_px": round(max_click_escape, 3),
    }


def hit_test_metrics(hit_test: dict[str, Any] | None) -> dict[str, Any]:
    if not isinstance(hit_test, dict):
        return {"available": False, "total": 0, "passed": 0, "failed": 0, "accuracy": None}
    results = hit_test.get("results")
    if isinstance(results, list):
        passed = sum(1 for item in results if isinstance(item, dict) and item.get("ok") is True)
        total = len(results)
    else:
        passed = int(hit_test.get("passed", hit_test.get("verified", 0)) or 0)
        failed = int(hit_test.get("failed", 0) or 0)
        total = passed + failed
    failed = total - passed
    return {
        "available": True,
        "total": total,
        "passed": passed,
        "failed": failed,
        "accuracy": ratio(passed, total),
    }


def hit_test_results_by_action_id(
    hit_test: dict[str, Any] | None,
) -> dict[str, bool]:
    if not isinstance(hit_test, dict) or not isinstance(hit_test.get("results"), list):
        return {}
    result = {}
    for item in hit_test["results"]:
        if not isinstance(item, dict):
            continue
        action_id = item.get("action_id")
        if isinstance(action_id, str) and isinstance(item.get("ok"), bool):
            result[action_id] = item["ok"]
    return result


def match_summary(
    pair: tuple[tuple[int, dict[str, Any]], tuple[int, dict[str, Any]]],
    hit_results: dict[str, bool],
    *,
    max_click_escape_px: float,
    max_center_delta_px: float,
) -> dict[str, Any]:
    reference, observed = pair
    reference_rect = rect_of(reference[1])
    observed_rect = rect_of(observed[1])
    center_delta = center_distance(reference[1], observed[1])
    click_escape = point_escape_distance(rect_center(observed_rect), reference_rect)
    action_id = observed[1].get("action_id")
    return {
        "reference_index": reference[0],
        "observed_index": observed[0],
        "observed_action_id": str(action_id or "")[:80],
        "key": list(action_key(reference[1])),
        "center_delta_px": round(center_delta, 3),
        "click_escape_px": round(click_escape, 3),
        "geometry_ok": click_escape <= max_click_escape_px
        and center_delta <= max_center_delta_px,
        "hit_test_ok": hit_results.get(action_id) if isinstance(action_id, str) else None,
    }


def normalize_visual_metrics(metrics: dict[str, Any] | None) -> dict[str, Any]:
    if not isinstance(metrics, dict):
        return {
            "available": False,
            "dimension_match": None,
            "observed_nonblank": None,
            "diff_ratio": None,
        }
    return {
        "available": True,
        "dimension_match": metrics.get("dimension_match"),
        "observed_nonblank": metrics.get("observed_nonblank", True),
        "diff_ratio": number(metrics.get("diff_ratio")),
        "mean_abs_channel_delta": number(metrics.get("mean_abs_channel_delta")),
        "reference_size": dimensions(metrics, "chrome", "reference"),
        "observed_size": dimensions(metrics, "saccade", "observed"),
    }


def write_fact_overlay(
    screenshot_path: str | Path,
    observed_truth: dict[str, Any],
    agreement: dict[str, Any],
    output_path: str | Path,
) -> Path:
    try:
        from PIL import Image, ImageDraw
    except ImportError as error:  # pragma: no cover - optional dependency problem.
        raise RuntimeError("Pillow is required to write an agreement overlay") from error

    screenshot_path = Path(screenshot_path)
    output_path = Path(output_path)
    image = Image.open(screenshot_path).convert("RGBA")
    viewport = extract_viewport(observed_truth) or {}
    viewport_width = number(viewport.get("width")) or image.width
    viewport_height = number(viewport.get("height")) or image.height
    scale_x = image.width / max(1.0, viewport_width)
    scale_y = image.height / max(1.0, viewport_height)
    matched_items = {
        int(item["observed_index"]): item
        for item in agreement.get("matches", [])
        if isinstance(item, dict) and isinstance(item.get("observed_index"), int)
    }
    draw = ImageDraw.Draw(image)
    visible_actions = [
        action for action in extract_actions(observed_truth) if is_visible_fact(action)
    ]
    for index, action in enumerate(visible_actions):
        rect = rect_of(action)
        if rect["width"] <= 0 or rect["height"] <= 0:
            continue
        match = matched_items.get(index)
        if match is None:
            color = (220, 58, 58, 255)
        elif match.get("geometry_ok") is False or match.get("hit_test_ok") is False:
            color = (224, 132, 32, 255)
        else:
            color = (42, 173, 91, 255)
        xy = (
            rect["left"] * scale_x,
            rect["top"] * scale_y,
            rect["right"] * scale_x,
            rect["bottom"] * scale_y,
        )
        draw.rectangle(xy, outline=color, width=max(2, round(min(scale_x, scale_y) * 2)))
        draw.text((xy[0] + 3, xy[1] + 2), str(index), fill=color)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    image.convert("RGB").save(output_path)
    return output_path


def compare_screenshots(
    reference_path: str | Path,
    observed_path: str | Path,
    *,
    threshold: int = 24,
) -> dict[str, Any]:
    try:
        from PIL import Image, ImageChops, ImageStat
    except ImportError as error:  # pragma: no cover - optional dependency problem.
        raise RuntimeError("Pillow is required to compare screenshots") from error

    reference = Image.open(reference_path).convert("RGB")
    observed = Image.open(observed_path).convert("RGB")
    dimension_match = reference.size == observed.size
    width = min(reference.width, observed.width)
    height = min(reference.height, observed.height)
    reference_crop = reference.crop((0, 0, width, height))
    observed_crop = observed.crop((0, 0, width, height))
    difference = ImageChops.difference(reference_crop, observed_crop)
    channel_masks = [
        channel.point(lambda value: 255 if value > threshold else 0)
        for channel in difference.split()
    ]
    diff_mask = ImageChops.lighter(
        ImageChops.lighter(channel_masks[0], channel_masks[1]), channel_masks[2]
    )
    diff_pixels = diff_mask.histogram()[255]
    total_pixels = max(1, width * height)
    observed_extrema = observed_crop.getextrema()
    observed_nonblank = any(high - low > 4 for low, high in observed_extrema)
    mean = ImageStat.Stat(difference).mean
    return {
        "dimension_match": dimension_match,
        "chrome_width": reference.width,
        "chrome_height": reference.height,
        "saccade_width": observed.width,
        "saccade_height": observed.height,
        "diff_ratio": round(min(1.0, diff_pixels / total_pixels), 6),
        "mean_abs_channel_delta": round(sum(mean) / len(mean), 3),
        "observed_nonblank": observed_nonblank,
    }


def action_key(action: dict[str, Any]) -> tuple[str, str]:
    label = normalize_text(
        action.get("label") or action.get("name") or action.get("action_id") or action.get("tag")
    )
    kind = normalize_text(action.get("kind") or action.get("tag") or "control")
    return label, kind


def is_visible_fact(action: dict[str, Any]) -> bool:
    if action.get("visible") is False or action.get("offscreen") is True:
        return False
    rect = rect_of(action)
    return rect["width"] > 0 and rect["height"] > 0


def is_actionable(action: dict[str, Any]) -> bool:
    if not is_visible_fact(action):
        return False
    if action.get("enabled") is False or action.get("disabled") is True:
        return False
    return not bool(action.get("blocked_by"))


def rect_of(action: dict[str, Any]) -> dict[str, float]:
    rect = action.get("rect") if isinstance(action.get("rect"), dict) else {}
    left = number(rect.get("left")) or 0.0
    top = number(rect.get("top")) or 0.0
    width = number(rect.get("width"))
    height = number(rect.get("height"))
    right = number(rect.get("right"))
    bottom = number(rect.get("bottom"))
    if width is None:
        width = max(0.0, (right or left) - left)
    if height is None:
        height = max(0.0, (bottom or top) - top)
    if right is None:
        right = left + width
    if bottom is None:
        bottom = top + height
    return {
        "left": left,
        "top": top,
        "right": right,
        "bottom": bottom,
        "width": width,
        "height": height,
    }


def center_distance(a: dict[str, Any], b: dict[str, Any]) -> float:
    ax, ay = rect_center(rect_of(a))
    bx, by = rect_center(rect_of(b))
    return hypot(ax - bx, ay - by)


def rect_center(rect: dict[str, float]) -> tuple[float, float]:
    return rect["left"] + rect["width"] / 2, rect["top"] + rect["height"] / 2


def rect_max_delta(a: dict[str, float], b: dict[str, float]) -> float:
    return max(abs(a[key] - b[key]) for key in ("left", "top", "width", "height"))


def point_escape_distance(point: tuple[float, float], rect: dict[str, float]) -> float:
    x, y = point
    dx = max(rect["left"] - x, 0.0, x - rect["right"])
    dy = max(rect["top"] - y, 0.0, y - rect["bottom"])
    return hypot(dx, dy)


def summarize_actions(actions: list[tuple[int, dict[str, Any]]] | list[dict[str, Any]]) -> list[dict[str, Any]]:
    result = []
    for item in actions[:10]:
        if isinstance(item, tuple):
            index, action = item
        else:
            index, action = None, item
        result.append(
            {
                "index": index,
                "action_id": str(action.get("action_id") or "")[:80],
                "label": normalize_text(action.get("label"))[:80],
                "kind": normalize_text(action.get("kind"))[:40],
            }
        )
    return result


def extract_string(payload: dict[str, Any], target: str) -> str | None:
    value = payload.get(target)
    if isinstance(value, str) and value:
        return value
    for key in ("truth", "result", "snapshot"):
        nested = payload.get(key)
        if isinstance(nested, dict):
            value = extract_string(nested, target)
            if value:
                return value
    return None


def safe_viewport(viewport: dict[str, Any] | None) -> dict[str, float] | None:
    if not viewport:
        return None
    result = {}
    for key in ("width", "height", "devicePixelRatio", "device_scale_factor"):
        value = number(viewport.get(key))
        if value is not None:
            result[key] = value
    return result


def dimensions(metrics: dict[str, Any], primary: str, fallback: str) -> dict[str, float] | None:
    width = number(metrics.get(f"{primary}_width", metrics.get(f"{fallback}_width")))
    height = number(metrics.get(f"{primary}_height", metrics.get(f"{fallback}_height")))
    if width is None or height is None:
        return None
    return {"width": width, "height": height}


def normalize_text(value: Any) -> str:
    return " ".join(str(value or "").strip().lower().split())


def number(value: Any) -> float | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    return None


def ratio(numerator: int, denominator: int) -> float:
    if denominator == 0:
        return 1.0
    return round(numerator / denominator, 6)


def add_reason(
    target: list[dict[str, Any]], code: str, message: str, value: Any = None
) -> None:
    item = {"code": code, "message": message}
    if value is not None:
        item["value"] = value
    target.append(item)
