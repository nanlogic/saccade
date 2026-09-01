# Saccade visual explainer

This package explains Saccade without pretending to show a recorded browser
session. The five frames are product diagrams, not benchmark footage.

## Outputs

| File | Use |
| --- | --- |
| `01-one-tab.png` | Exact Agent tab ownership |
| `02-only-changes.png` | First read followed by browser-pushed deltas |
| `03-current-target.png` | Local actionability and identity checks |
| `04-action-receipt.png` | Verified receipt and explicit unknown outcome |
| `05-real-browser.png` | Chrome/Edge, Extension, Node Broker, and MCP setup |
| `saccade-explainer-loop.gif` | DEV article and other inline posts |
| `saccade-explainer-14s.mp4` | Medium, social posts, or a hosted video embed |
| `explainer-contact-sheet.png` | Quick review of the complete sequence |

Each store frame is `1280 × 800`. Editable SVG source sits beside each PNG.

## Recommended Chrome Web Store order

Upload `01` through `05` in filename order. The public Chrome Web Store item is:

https://chromewebstore.google.com/detail/saccade/gbjapdcoclbdjpcaogmjdbpmnmfgombn

## Ready-to-paste DEV block

Place this after the article's opening problem, before the architecture details:

```markdown
Most browser-agent demos stop at the click. Saccade keeps track of the tab,
what changed, and whether the action actually worked.

![Saccade gives an AI one authorized tab, sends small page changes, checks the current target locally, and returns a verified action receipt.](https://raw.githubusercontent.com/nanlogic/saccade/main/docs/marketing/assets/explainer/saccade-explainer-loop.gif)

The loop is simple: own one tab, read the current page, receive changes, act on
the current object, and get evidence back.
```

For readers who prefer still images, use `01-one-tab.png`,
`02-only-changes.png`, and `04-action-receipt.png` at the matching sections.

## Accessibility text

- `01`: An AI Agent is assigned one authorized browser tab while other tabs
  remain outside its session.
- `02`: A full semantic read at revision 42 is followed by a small revision 43
  delta showing that the Save button became enabled.
- `03`: Saccade checks that the Save button is visible, enabled, stable,
  focusable, and still the same object immediately before acting.
- `04`: A Save action returns accepted, observed, and verified status. A second
  card shows that an uncertain outcome is not replayed automatically.
- `05`: The Agent connects through six MCP tools and the local Node Broker to a
  Saccade-authorized Chrome or Edge tab.

## Source note

The browser-flow illustration in frame `01` was generated with the built-in
ImageGen tool. The prompt requested a text-free, transparent product
illustration of browser cards converging into one verified result, using navy,
electric blue, warm ivory, and a small green success accent. All copy, diagrams,
branding, and layout were then built as deterministic SVG so the text remains
exact and editable.
