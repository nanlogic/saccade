# Saccade M1 Site Profile

URL: `https://mouseaccuracy.com/classic/`
Run directory: `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153`

## Load and Controls

- Page title: `Mouse Accuracy - Mouse Accuracy and Pointer Click Training`
- Device pixel ratio reported by page: `1`
- Pointer event support: `true`
- Control rect discovery: Epic=1 Tiny=1 Start=1
- Option state after clicking Epic and Tiny:

```json
[
  {
    "tag": "INPUT",
    "type": "radio",
    "value": "300",
    "text": ""
  },
  {
    "tag": "INPUT",
    "type": "radio",
    "value": "tTiny",
    "text": ""
  }
]
```

## Page Technology

- Classified tech: `dom/svg`
- Canvas list from initial/final probes:

```json
[]
[]
```

- Iframes/ad/consent candidates:

```json
[
  {
    "rect": {
      "x": 276,
      "y": 615.6,
      "w": 728,
      "h": 90
    },
    "src": "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-6514849016967014&output=html&h=90&slotname=2660503089&adk=3"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-6514849016967014&output=html&adk=329615837&adf=2762459402&l"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": ""
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://googleads.g.doubleclick.net/pagead/html/r20260610/r20190131/zrt_lookup_fy2021.html"
  }
]
[
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-6514849016967014&output=html&h=90&slotname=2660503089&adk=3"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-6514849016967014&output=html&adk=329615837&adf=2762459402&l"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": ""
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "about:blank"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://www.google.com/recaptcha/api2/aframe"
  },
  {
    "rect": {
      "x": 0,
      "y": 0,
      "w": 0,
      "h": 0
    },
    "src": "https://googleads.g.doubleclick.net/pagead/html/r20260610/r20190131/zrt_lookup_fy2021.html"
  }
]
```

## No-Click Run

- We clicked Epic, Tiny, then Start through `WebView::notify_input_event`.
- Target clicks were intentionally disabled for this recon run.
- Result text:

```json
[
  "You clicked 0 targets.",
  "You misclicked 0 times."
]
```

- Score/timer text:

```json
[
  "You clicked 0 targets.",
  "You misclicked 0 times.",
  "Time Remaining: 0"
]
```

## Run Observations

- countdown-before-start: no separate countdown observed; first captured timer line was `Time Remaining: 15`.
- target spawn cadence: median 306 ms (min 303 ms, max 323 ms, avg 306 ms) across 48 gaps; captured 49 target additions.
- Tiny target size range: width 0.1-16 CSS px; height 0.1-16 CSS px.
- multiple target coexistence: max 10 visible target DOM nodes in passive samples.
- target lifetime/animation curve: targets persisted concurrently for at least 14562 ms; exact per-target lifetime BLOCKED by anonymous DOM nodes in the no-click run.
- no-click result screen: observed `Time is up!` in passive samples.

## Unknowns From Section 2.4

- target technology: `dom/svg`.
- hit event path: BLOCKED until M4/M5 calibration pages or a controlled M1 click probe; this no-click M1 proves option/start input only.
- target lifetime/animation curve: see run observations; exact per-target lifetime remains BLOCKED without stable target IDs.
- Epic spawn interval: see run observations.
- multiple target coexistence: see run observations.
- consent banner behavior: see iframe/body samples; none acted on automatically.
- ad slot/iframe behavior: see iframe/body samples; click safety still requires game-area exclusion later.

## Observation Sample

```json
{
  "armedAt": 2679.44,
  "mutationCount": 49,
  "sampleCount": 183,
  "droppedMutations": 0,
  "targetMutationSample": [
    {
      "t": 2998.09,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 594.4,
        "y": 357.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 3304.78,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 645.6,
        "y": 613.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 3613.38,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 1132,
        "y": 525.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 3916.73,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 197.6,
        "y": 445.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 4221.5,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 69.6,
        "y": 453.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 4529.03,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 799.2,
        "y": 141.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 4833.39,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 543.1833333333333,
        "y": 37.6,
        "w": 4.8,
        "h": 4.8
      }
    },
    {
      "t": 5137.41,
      "kind": "added",
      "tag": "DIV",
      "id": "",
      "cls": "target tTiny",
      "rect": {
        "x": 492,
        "y": 309.6,
        "w": 4.8,
        "h": 4.8
      }
    }
  ],
  "targetSample": [
    {
      "t": 3094.36,
      "scoreText": [
        "Time Remaining: 15"
      ],
      "canvases": [],
      "targets": [
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 593.1666666666666,
            "y": 356.3666666666667,
            "w": 7.266666666666667,
            "h": 7.266666666666667
          }
        }
      ]
    },
    {
      "t": 3198.91,
      "scoreText": [
        "Time Remaining: 15"
      ],
      "canvases": [],
      "targets": [
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 592.6166666666667,
            "y": 355.81666666666666,
            "w": 8.35,
            "h": 8.35
          }
        }
      ]
    },
    {
      "t": 3305.53,
      "scoreText": [
        "Time Remaining: 15"
      ],
      "canvases": [],
      "targets": [
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 592.15,
            "y": 355.35,
            "w": 9.3,
            "h": 9.3
          }
        },
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 645.6,
            "y": 613.6,
            "w": 4.8,
            "h": 4.8
          }
        }
      ]
    },
    {
      "t": 3409.24,
      "scoreText": [
        "Time Remaining: 15"
      ],
      "canvases": [],
      "targets": [
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 591.6833333333333,
            "y": 354.8833333333333,
            "w": 10.216666666666669,
            "h": 10.233333333333333
          }
        },
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 645.1333333333333,
            "y": 613.1333333333333,
            "w": 5.733333333333333,
            "h": 5.733333333333333
          }
        }
      ]
    },
    {
      "t": 3510.93,
      "scoreText": [
        "Time Remaining: 15"
      ],
      "canvases": [],
      "targets": [
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 591.2166666666667,
            "y": 354.4166666666667,
            "w": 11.15,
            "h": 11.15
          }
        },
        {
          "tag": "DIV",
          "id": "",
          "cls": "target tTiny",
          "rect": {
            "x": 644.6666666666666,
            "y": 612.6666666666666,
            "w": 6.666666666666667,
            "h": 6.666666666666667
          }
        }
      ]
    }
  ]
}
```

## Screenshots

- `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/01_loaded.png`
- `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/02_after_options.png`
- `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/03_mid_game_readback.png`
- `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/04_results.png`

## Errors / Warnings

- take_screenshot timed out for 03_mid_game_readback.png; used readback fallback

## Raw Probe Files

- initial probe: `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/initial_probe.json`
- after-options probe: `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/after_options_probe.json`
- arm observation: `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/arm_observation.json`
- final probe: `/Users/waynema/Documents/GitHub/SACCADE/runs/recon/1781157153/final_probe.json`

SERVO_COMPAT: GO
