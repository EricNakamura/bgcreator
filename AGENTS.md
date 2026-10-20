# AGENTS.md

Procedural backgrounds for Holyrics. Rust desktop app (eframe/egui + wgpu), output 1920×1200 (16:10) PNG and H.264 MP4. UI strings in Portuguese; code identifiers in English.

## Commands

From the repo root (the app resolves `presets/` and `exports/` relative to cwd):

```bash
cargo run                 # GUI with live preview
cargo test                # fast unit tests
cargo test -- --ignored   # GPU + ffmpeg smoke tests; writes target/*.png, target/palettes/*.png, target/*-test.mp4
cargo fmt --all
cargo clippy
```

Requires `ffmpeg` (`brew install ffmpeg`) for MP4 export/tests. Verify output:

```bash
ffprobe -v error -show_entries stream=codec_name,profile,level,width,height,pix_fmt,r_frame_rate,color_space,color_transfer,color_primaries -show_entries format=duration,size -of default=noprint_wrappers=1 target/shine-test.mp4
```

Expected: H.264 High, level 50, 1920×1200, yuv420p, 30 fps, 12 s, bt709.

## Layout

- `src/model.rs` — pure data (`Preset`, palette, bars, text, warp, frame/time constants). No egui/wgpu imports.
- `src/render/` — wgpu scene pipeline, post-processing pass, text atlas texture, offscreen targets, readback, PNG export.
- `src/encode/` — PNG writer and ffmpeg MP4 worker.
- `src/app.rs` — egui UI, preview texture, export progress.
- `src/text.rs` — rasterizes up to 4 text lines into a 2048×256×4 RGBA texture array with `fontdue`. Font: `assets/Inter.ttf` (variable, OFL in `assets/Inter-OFL.txt`).
- `shaders/common.wgsl` + one file per scene (`shine.wgsl`, `gradient.wgsl`) — concatenated at runtime by `render/mod.rs`. Shared uniforms/helpers live in `common.wgsl`.
- `shaders/post.wgsl` — warp maps, text compositing, global color grade, sharpness and final dither. Applied to every scene before preview/export.
- `ColorGrade`, `BarConfig`, `TextConfig` and `WarpConfig` are saved per preset; `#[serde(default)]` keeps old presets valid.
- Gradient has six styles in one shader: `horizon`, `aurora`, `mesh`, `waves`, `haze`, `prism`, selected by `gradient_style`.
- Bars: 4 fixed bars in Gradient only. Each has its own style, palette, phase, enabled and mirror flag; `common.wgsl` uses the private `g_bar` to select the palette.
- Text: 4 lines max, each with direction/motion/speed/size/color/opacity/Y. Scroll speed is an integer number of tile widths per loop so the 12 s loop closes.
- Warp: procedural maps (FBM, turbulence, Voronoi, waves, swirl, ripple) sample both scene and text at a displaced UV, so it distorts the whole composition.
- `presets/{scene}.toml` — user-editable presets; the app writes the default on first run/switch if absent.
- `exports/` — output folder, git-ignored.

## Framework gotchas

- eframe 0.37 uses `App::ui(&mut self, &mut egui::Ui, &mut Frame)`, not `update(&Context, ...)`. Panels are `egui::Panel::left/right/...`, not `SidePanel`. `CentralPanel::show` takes `&mut Ui`.
- Shader files are intentionally commented in Portuguese as teaching material. When changing shader math, update the explanation next to it; do not strip the didactic comments.
- WGSL `smoothstep(e0, e1, x)` requires `e0 < e1`; to invert a transition use `1.0 - smoothstep(...)`, never reverse the edges.
- Always import wgpu through `eframe::wgpu` (wgpu 30 re-export). Adding a second `wgpu` version creates incompatible device/texture types.
- egui native textures must be `Rgba8Unorm`. Do not switch the render target to `Rgba8UnormSrgb`; the scene shader already encodes sRGB before the post pass.
- `Uniforms` is exactly 576 bytes and must stay in sync with the WGSL struct: `resolution`, `time`, `seed`, scene params, bars, warp, text params/colors, then the two grade blocks. `params2.x` is center dim, `params2.y` is the gradient style index. There is a size assertion test.
- Text texture is a `texture_2d_array` with 4 layers; layer V=0 is top. The post shader must invert V (`0.5 - (p.y - line_y)/height`) or the text renders upside down. Tile width in p units is `line_height * 8` (atlas 2048/256).
- Text is baked into PNG/MP4, not live. Holyrics should still own lyrics and live countdown text.
- Warp is applied in `post.wgsl` by displacing the UV used for both `scene_texture` and `text_texture`. Do not warp only one of them.
- Readback rows must be 256-byte aligned. 1920 px works naturally; keep the padding loop for any other width.
- Loop is exactly 12 s × 30 fps = 360 frames. Export uses `t = frame / 360`; every animated term must be periodic (sin/cos/TAU, closed drift, integer line phases). Never use raw elapsed time in a shader.
- 1920×1200 needs H.264 **level 5.0** (9000 macroblocks, above level 4.x). 1920×1080 would be level 4.0. Do not lower it.
- MP4 export runs ffmpeg with `libx264`, CRF 20, maxrate 5M, yuv420p, bt709, faststart, no audio. Keep it under Holyrics' comfort zone.
- The GUI-launched app may not see Homebrew on PATH; `encode/mp4.rs` resolves `/opt/homebrew/bin/ffmpeg` and `/usr/local/bin/ffmpeg` explicitly (`shortcut:` comment). Keep that in mind before changing the command.
- MP4 export runs on a worker thread with a snapshot of the preset; the preview stays interactive. Do not share mutable UI state with the worker.
- Each frame is rendered twice: scene → `scene_target`, then `post.wgsl` (color grade) → final `target`. Preview and exports use that final texture; do not read `scene_target` for output.
- Sharpness uses a fixed 1920×1200 texel offset so preview and export match. Keep it in the post shader, not in individual scenes.

## Holyrics constraints

- Bundled VLC: H.264 is safe; H.265 is not supported.
- Recommended target: 1920×1200, 30 fps, ≤5 Mbps, no audio, yuv420p, bt709.
- Video backgrounds play only on the public screen by default.
- Import is manual: Tema → Plano de Fundo. The Holyrics API does not import new background files; `.thf` is undocumented. Do not build automatic theme import without verifying it.
- Keep the central text zone dark and low-detail; the preview has a lyrics overlay and safe-zone guide for this.

## Current scope

Scenes: `Shine` (upward rays) and `Gradient` (Horizonte, Aurora, Mesh, Ondas, Névoa, Prisma), with 18 palettes, 4 configurable bars, 4 text lines and procedural global warp. Scene switch loads/saves `presets/<scene>.toml`. Next planned: `Arc Lines`, pack export (seed/palette variations), then a cross/light scene. Out of scope for now: 4K, 21:9, live output/Holyrics control, imported noise PNG, keyframe editor, `.thf`, photo/3D-based scenes.
