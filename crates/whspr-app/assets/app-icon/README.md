# whspr app icon

Two directions from the "Typeset"/Modernist brief. The wordmark's `w` (Archivo
ExtraBold) on a baseline rule; a square is the full stop.

- **`whspr-1c.*` — "Poster" (primary/leaning):** accent-red ground, ground-colour
  `w`, ink full stop. Loudest in a Dock; reads as the Recording state.
- **`whspr-1a.*` — "w." (alternate):** paper ground, ink `w`, red-square full stop.

`*.svg` are the editable vector sources (the `w` uses the bundled Archivo
ExtraBold; `*-1024.png` are flat 1024 raster masters with the font baked in).
Corners are flat (full-bleed) — macOS applies its own squircle mask.

To export the macOS icon set once a direction is locked:
`sips` each master down to 512/256/128/64/32/16 into an `.iconset`, then
`iconutil -c icns whspr.iconset -o whspr.icns`.
