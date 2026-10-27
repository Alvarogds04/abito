# Abito icon sources

`abito-icon.svg` (macOS, keeps Apple's ~80/1024 margin) and `abito-icon-full.svg` (Linux and Windows, full-bleed tile)
wrap Abito's clay character in its `idle` pose, rendered from `frontend/clay-engine.js` (seed 505, no shadow), on a
dark rounded tile. Rasterize each SVG at 1024×1024 and derive:

- `icons/icon.icns`: every macOS size (16–512 plus @2x, i.e. up to 1024) from `abito-icon.svg`; this is what the .app uses.
- `icons/icon-macos.png`: 512×512 from `abito-icon.svg` (kept for previews).
- `icons/icon.png`: 256×256 from `abito-icon-full.svg` (Linux launcher and window icon).
- `icons/icon.ico`: 16, 24, 32, 48, 64, 128 and 256 px from `abito-icon-full.svg` (Windows exe and installer).

The tray keeps its own state glyphs (`src/tray.rs`); these files are the application icon only.
