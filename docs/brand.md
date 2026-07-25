# Assemblywright Brand

Assemblywright is the product name. It combines an assembly of specialized
models with the craft implied by a wright.

## Positioning

**Assemblywright**

Orchestrated intelligence. Verified software.

Supporting lines:

- Plan together. Build locally. Advance by proof.
- From approved blueprint to verified `main`.

The product is local-first. Frontier cloud models provide selective review,
not an always-on cloud control plane.

## Product Vocabulary

| Term | Meaning |
| --- | --- |
| Assemblywright | The product |
| The Assembly | The coordinated model system |
| The Line | The feature conveyor |
| Blueprint | An owner-approved specification |
| Builders | Local implementation agents |
| Proof Gate | Deterministic validation |
| Review Gate | Independent model review |
| Control Room | The owner interface |

Use **Assemblywright** exactly, with a capital A and lowercase w. Do not use
AssemblyWrite or AssemblyRight.

## Visual Identity

Two marks carry the identity, one for each half of the tagline.

**The proofmark** is the primary mark: an `A` whose crossbar is a check. It is
the app icon, the favicon, the menu bar icon, and the lockup partner. It stands
for orchestrated intelligence made by a wright.

**The proof seal** is the companion mark: a struck octagonal die around a
chevron and rule. It is reserved for verification surfaces — release evidence
bundles, signed provenance reports, and status badges. It stands for verified
software. It is never the app icon, and it never replaces the proofmark as the
product signature.

Both marks are drawn on a 64-unit grid with round caps and joins. Scale the
committed SVG; never redraw the geometry by hand.

### Palette

| Role | Name | Hex | Use |
| --- | --- | --- | --- |
| Ink | Graphite | `#15181B` | The mark on light surfaces, and the icon plate |
| Chalk | Paper | `#F7F5F2` | The mark on dark surfaces |
| Brass | Brass | `#C1873C` | The check arm, and verified states |

Brass is an accent only. It never draws the `A` itself, never fills a
background, and never appears below 20 px rendered size, where it turns to mud.

### Size variants

The tick cannot survive every size, so the mark reduces in defined steps.

| Rendered size | Source | Reduction |
| --- | --- | --- |
| 32 px and up | `mark.svg` | Full proofmark, two-color permitted |
| 20 to 32 px | `mark-bold.svg` | Heavier strokes, tick intact |
| 18 px and below | `mark-micro.svg` | Tick reduces to a straight crossbar, single color |

Minimum sizes: proofmark 16 px, proof seal 24 px. Clear space on all four sides
is one quarter of the mark's height.

### Typography

The wordmark is Instrument Sans Bold, outlined to paths. Instrument Sans is
published under the SIL Open Font License 1.1, which permits using its outlines
in a logo and redistributing them; the license text is kept alongside the assets
as `assets/brand/InstrumentSans-OFL.txt`.

`wordmark.svg` and `lockup-horizontal.svg` carry no live text and no font
reference, so they render identically everywhere and need no font installed.

To reproduce or retrack the wordmark, the settled values on the 64-unit grid are:

| Value | Setting |
| --- | --- |
| Face | Instrument Sans Bold |
| Cap height | 22 units, which is 29.7297 pt |
| Tracking | 1% of point size, which is 0.2973 pt |
| Baseline | `y = 44`, centering the cap height on the mark |
| Wordmark left edge | `x = 70`, a 16-unit gap from the mark's ink |

Bold is not a stylistic preference. The proofmark is a monoline skeleton whose
stroke is about 10% of its height; a Regular wordmark at matched cap height has
roughly half that stem weight and reads visibly thinner beside the mark. Bold
brings the stems to about 4 units against the mark's 5.

Do not set the wordmark in a system face. Helvetica Neue and SF ship with macOS
but neither is licensed for use in a product wordmark.

### Do not

- Recolor the `A` in brass, or fill its counter.
- Add gradients, shadows, glows, or outlines to either mark.
- Rotate or stretch either mark, or alter its stroke weights.
- Use the seal as an app icon, or the proofmark as a verification badge.

### Files

Hand-edited sources live in `assets/brand/svg`. Every binary in
`assets/brand/generated` — the `.icns`, menu bar templates, favicons,
`favicon.ico`, web tiles, and seal PNGs — is produced by
`scripts/generate-brand-assets.sh`, which also has a `--check` mode that fails
when the committed output no longer matches its sources.

### How the app consumes them

`scripts/package-distribution.sh` copies `Assemblywright.icns` and the three
menu bar templates into `Contents/Resources` as it builds the bundle, and sets
`CFBundleIconFile` in the generated `Info.plist`. The zip payload validation,
the unsigned structure check, and the installer payload assertions all require
those files, so the icon cannot silently drop out of a release.

The Swift shell reads the menu bar art through `JarvisBrandAssets`, which marks
it as template art so AppKit tints it for the current menu bar appearance.
`NSImage(named:)` resolves the `@2x` and `@3x` companions automatically. A
development run through `swift run` has no bundle resources, so the lookup
returns nil and the menu bar falls back to an SF Symbol.

The menu bar shows the proofmark alone when the core is available, and adds a
small state badge beside it for every other mode. Brand presence is the calm
state; a badge means something needs attention.

## Compatibility Names

The Cargo crates and their binaries are now `assemblywright-*`, and the legacy
`jarvis*` binary aliases have been removed.

Identifiers that bind installed state, issued credentials, or signed artifacts
stay unchanged, because renaming them would change code-signing identity, void
an already-issued certificate, or orphan a user's existing installation:
`JARVIS_*` environment variables, Keychain services,
`~/Library/Application Support/Jarvis`, protocol labels such as
`EXPORTER-Jarvis-Developer-Mode-v1`, the `urn:jarvis:device:<uuid>` certificate
subject-alternative-name URI, the protocol-version-1 fixture capability
provider and model (`jarvis-fixture` and `jarvis-fixture-v1`), the
`JarvisMaster` Windows service name and its `Jarvis\master` state directory,
signed code identifiers such as `com.nobiletechnology.jarvis`, the
`JarvisMacApp` executable name inside the bundle, and the bundled `jarvis-cli`
filename. These remain stable compatibility contracts until a separately
designed and tested migration exists.

`./scripts/release-naming-contract-smoke.sh --check` enforces both directions:
the product-facing names must be current, the legacy aliases must stay removed,
and every identifier in the list above must stay exactly as it is. Do not "clean
up" a name that gate protects; change the contract and its migration first.

User-facing app bundles and release artifacts use `Assemblywright.app` and
`Assemblywright-<version>.*`. The CLI entry point is `assemblywright`.

## License

The repository is licensed under the Apache License, Version 2.0. The license
does not grant trademark rights to the Assemblywright name.
