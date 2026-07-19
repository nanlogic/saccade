# Saccade application icons

These are the canonical cross-platform Saccade icon assets.

- `saccade-icon-macos.png`: opaque midnight-navy rounded-square plate with
  transparent outer corners. `build_icon_macos.sh` converts it to
  `Saccade.icns` on every macOS app build.
- `saccade-icon-windows.png`: transparent Human/Agent interwoven glyph for
  Windows light, dark, and wallpaper backgrounds.
- `Saccade.icns`: generated macOS bundle icon.
- `Saccade.ico`: generated Windows icon containing 16, 24, 32, 48, 64, 128,
  and 256 pixel representations.

The ivory ribbon represents Human attention, the cobalt ribbon represents the
Agent, and their centered rounded-square negative space is Saccade's shared
browser viewport. Packaging must use these platform-specific assets rather
than the legacy SVG exploration or an upstream browser icon.

Rebuild the platform containers with:

```sh
engines/cef/scripts/build_icon_macos.sh
engines/cef/scripts/build_icon_windows.sh
```
