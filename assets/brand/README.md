# Assemblywright brand assets

Identity rules — marks, palette, size reductions, typography, and prohibitions —
live in [docs/brand.md](../../docs/brand.md). This file is the file map.

## Sources

Hand-edit these. Everything else is generated.

| File | Purpose |
| --- | --- |
| `svg/mark.svg` | Proofmark, primary. Inherits `currentColor`. Use at 32 px and up |
| `svg/mark-bold.svg` | Proofmark for 20 to 32 px. Heavier strokes, tick intact |
| `svg/mark-micro.svg` | Proofmark for 18 px and below. Tick reduces to a crossbar |
| `svg/seal.svg` | Proof seal. Inherits `currentColor`. Verification surfaces only |
| `svg/lockup-horizontal.svg` | Mark plus wordmark, outlined. No font dependency |
| `svg/wordmark.svg` | Wordmark alone, outlined. No font dependency |
| `svg/app-icon.svg` | macOS icon, 1024 grid, for the 128 px slice and up |
| `svg/app-icon-small.svg` | macOS icon for the 32 and 64 px slices |
| `svg/app-icon-micro.svg` | macOS icon for the 16 px slice |
| `svg/menubar-template.svg` | Menu bar template, black on transparent |
| `svg/favicon.svg` | Plated favicon for 32 and 48 px |
| `svg/favicon-micro.svg` | Plated favicon for 16 px |
| `svg/web-tile.svg` | Full-bleed square tile for touch icons and web manifests |

The `app-icon*` sources share one 824 px plate inset in a 1024 px canvas, so the
icon does not appear to change size between slices. Only the glyph weight and
scale differ.

## Generated

Do not edit `generated/` by hand. Regenerate it:

```bash
./scripts/generate-brand-assets.sh
```

| File | Consumer |
| --- | --- |
| `generated/Assemblywright.icns` | `Assemblywright.app` bundle icon |
| `generated/menubar-template.png` and `@2x`, `@3x` | Menu bar item |
| `generated/favicon-16.png`, `-32`, `-48`, `favicon.ico` | Browser tabs |
| `generated/apple-touch-icon.png` | iOS home screen |
| `generated/web-tile-192.png`, `-512` | Web app manifest |
| `generated/seal-ink-512.png`, `seal-chalk-512.png` | Release evidence, badges |

To prove the committed binaries still match their sources:

```bash
./scripts/generate-brand-assets.sh --check
```

The generator needs `rsvg-convert` (`brew install librsvg`) plus `iconutil` and
`python3` from the Xcode command line tools.

## Notes for consumers

The menu bar PNGs are template art, but AppKit will not treat them as such from
the filename alone. `AssemblywrightBrandAssets` in
[apps/mac/Sources/AssemblywrightMacApp/BrandAssets.swift](../../apps/mac/Sources/AssemblywrightMacApp/BrandAssets.swift)
sets `isTemplate = true` after loading; any new consumer must do the same. The
`@2x` and `@3x` suffixes resolve automatically through `NSImage(named:)`.

`scripts/package-distribution.sh` is what puts these into the app bundle. If you
rename a file here, update `APP_ICON_FILE` or `MENU_BAR_TEMPLATE_FILES` in that
script — the packaging gates assert the bundled paths by name.

`mark.svg`, `mark-bold.svg`, `mark-micro.svg`, `seal.svg`, `wordmark.svg`, and
`lockup-horizontal.svg` all draw with `currentColor`, so inlining them in HTML or
SwiftUI picks up the surrounding text color and adapts to light and dark without
a second copy. Rendering them standalone yields black.

The wordmark is outlined Instrument Sans Bold. `InstrumentSans-OFL.txt` is its
license, kept here for provenance. No font file is vendored and none is needed —
see the typography table in [docs/brand.md](../../docs/brand.md) for the exact
settings if the wordmark ever has to be regenerated.
