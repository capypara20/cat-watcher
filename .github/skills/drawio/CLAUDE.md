# Skill + CLI

Claude Code skill that generates native `.drawio` files, with optional export to PNG/SVG/PDF (with embedded XML) using the draw.io desktop CLI. No MCP server required.

## Key Files

| File | Purpose |
|------|---------|
| `drawio/SKILL.md` | Claude Code skill file (users copy this to their skills directory) |
| `README.md` | Installation and usage documentation |

## How It Works

1. User invokes `/drawio` or Claude detects a diagram request
2. Claude generates mxGraphModel XML for the requested diagram
3. The XML is written to a `.drawio` file in the working directory via the Write tool
4. If the user requested an export format (png, svg, pdf), the draw.io CLI exports to `.drawio.png` / `.drawio.svg` / `.drawio.pdf` with `--embed-diagram`, then deletes the source `.drawio` file
5. The result is opened for viewing (`open` / `xdg-open` / `start`)

Default output is `.drawio` (no export). The user requests export by mentioning a format: `/drawio png ...`, `/drawio svg: ...`, etc.

## draw.io CLI Locations

- **macOS**: `/Applications/draw.io.app/Contents/MacOS/draw.io`
- **Linux**: `drawio` (on PATH via snap/apt/flatpak)
- **Windows**: `"C:\Program Files\draw.io\draw.io.exe"`
- **WSL2**: `` `/mnt/c/Program Files/draw.io/draw.io.exe` `` (detect via `grep -qi microsoft /proc/version`)

The skill tries `drawio` first, then falls back to the platform-specific path. On WSL2, use `wslpath -w` to convert paths when opening files with `cmd.exe /c start`.

## Why XML Only?

A `.drawio` file is native mxGraphModel XML. Mermaid and CSV formats require draw.io's server-side conversion and cannot be saved as native files. The skill generates XML directly for all diagram types.

## Dark Mode Colors

draw.io supports automatic dark mode rendering. How colors behave depends on the property:

- **`strokeColor`, `fillColor`, `fontColor`** default to `"default"`, which renders as black in light theme and white in dark theme. When no explicit color is set, colors adapt automatically.
- **Explicit colors** (e.g. `fillColor=#DAE8FC`) specify the light-mode color. The dark-mode color is computed automatically by inverting the RGB values (blending toward the inverse at 93%) and rotating the hue by 180° (via `mxUtils.getInverseColor`).
- **`light-dark()` function** — To specify both colors explicitly, use `light-dark(lightColor,darkColor)` in the style string, e.g. `fontColor=light-dark(#7EA6E0,#FF0000)`. The first argument is used in light mode, the second in dark mode.

To enable dark mode color adaptation, the `mxGraphModel` element must include `adaptiveColors="auto"`.

When generating diagrams, you generally do not need to specify dark-mode colors — the automatic inversion handles most cases. Use `light-dark()` only when the automatic inverse color is unsatisfactory.

## Coding Conventions

- **Allman brace style**: Opening braces go on their own line for all control structures, functions, objects, and callbacks.
- Prefer `function()` expressions over arrow functions for callbacks.
- See the root `CLAUDE.md` for examples.
