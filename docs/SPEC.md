# Background Customizer — Specification

**Version:** 0.3
**Date:** October 29, 2026
**Status:** Draft
**Goal:** Build a high-performance, open, cross-platform live-wallpaper engine in Rust that beats Wallpaper Engine on performance, resource use, and control.

## 1. Overview

Background Customizer is a desktop wallpaper engine with a Rust core and a modern web-based settings UI. It runs in the background with a tray icon, supports multiple monitors with independent wallpapers, and plays images, videos, GIFs, and later web/shader wallpapers.

The primary performance goal is not feature parity. It is to do strictly less work than Wallpaper Engine at every stage: no GPU work when paused, no CPU copies during video playback, and per-monitor pausing instead of global pausing.

## 2. Goals and Non-Goals

### Goals

- Run as a background process with a tray icon and no taskbar button.
- Support multiple monitors, each with its own wallpaper and settings.
- Support images, GIFs, and video wallpapers at user-selected resolutions and frame caps.
- Provide a modern, accessible settings UI built with web tools.
- Achieve lower CPU, GPU, RAM, and power use than Wallpaper Engine on the same hardware and content.
- Pause per monitor based on occlusion, not globally.
- Run each wallpaper in an isolated, crash-safe context.
- Be open source and cross-platform over time.

### Non-Goals (v1)

- Beating Wallpaper Engine's content library.
- Shipping a full wallpaper editor.
- Supporting every Workshop scene type on day one.
- Mobile support.

## 3. Architecture

| Component | Responsibility | Technology |
|---|---|---|
| `core` | Config, profiles, monitor model, scheduling, state machine | Rust, `serde`, `tokio` |
| `renderer` | One native GPU surface per monitor | `wgpu`, platform APIs |
| `decoder` | Image/video decode, frame pacing, hardware decode | `image`, libmpv or `ffmpeg-next` |
| `shell` | Tray, settings window, autostart, IPC | Tauri v2 |
| `ui` | Library, monitor picker, playlists, settings | React or Svelte, Vite, TypeScript, Tailwind, shadcn/ui |
| `importer` | Reads third-party wallpaper formats | Isolated crate |
| `bench` | Benchmark harness and regression checks | Rust + PresentMon/PDH |

### Process Model

- The Tauri shell is the resident process and owns the tray, settings window, and IPC.
- Each wallpaper renderer runs in its own supervised process, so one bad wallpaper cannot crash the shell.
- Web wallpapers run in a separate sandboxed webview process with network access off by default.

### Desktop Attachment (Windows)

The Windows backend uses the standard WorkerW technique: send Progman message `0x052C`, find the WorkerW sibling window, and reparent the wallpaper surface behind the desktop icons. This is wrapped behind a `DesktopHost` trait so macOS and Linux backends can be added later.

## 4. Functional Requirements

1. **Background operation.** Autostart on login, tray icon with Pause/Resume, Next, Open Settings, per-monitor submenu, and Quit. Closing settings hides the window rather than exiting the app.
2. **Multi-monitor.** Independent wallpaper, fit mode (fill, fit, stretch, tile, center), and playback state per monitor. Handle hotplug, DPI change, and resolution change without crashing.
3. **Formats.**
   - Images: PNG, JPG, WebP, AVIF, GIF.
   - Video: MP4/H.264, H.265, VP9, AV1, WebM.
   - Later: HTML, WebGL/shader, and Wallpaper Engine scene import.
4. **Resolution and frame rate.** User picks a frame cap (15, 24, 30, 60, or match display). Decode resolution matches the monitor resolution. Hardware decode is used when available.
5. **Library.** Drag-and-drop import, thumbnail generation, tags, favorites, playlists, timed or random rotation.
6. **Resource governor.** Per-wallpaper CPU/GPU/RAM budgets, live usage display, and automatic downgrade or pause for offenders.
7. **Per-monitor occlusion.** A covering window on monitor A pauses only monitor A; monitor B continues.
8. **Audio.** Optional per-wallpaper audio, processed in a separate hot-swappable subsystem so device changes do not stall video.
9. **Sandboxed web wallpapers.** Each runs in its own process, network off by default, with per-wallpaper permission prompts.

## 5. Performance Requirements

These are the core differentiators. Targets are hypotheses to validate with the benchmark harness.

| Area | Requirement |
|---|---|
| Paused wallpaper | GPU at idle floor, CPU effectively zero, decoder thread asleep |
| Static image | No frames submitted after the first draw |
| Video playback | Zero CPU readback; decoded frames stay on GPU |
| Decode resolution | Matches monitor resolution, never upscaled decode |
| Frame pacing | Present only on new frames; cap to content or display rate |
| Per-monitor pause | Only the occluded monitor pauses |
| Pause latency | Under 100 ms from occlusion event to GPU idle |
| Memory | Bounded texture pool and decoder queues; no unbounded growth |
| Web wallpapers | Suspended completely when paused |

### Performance Techniques

- Hardware decode (D3D11VA/DXVA) with decoded frames kept as GPU textures.
- Share decoded textures directly with the wgpu surface via DXGI shared handles to avoid copies.
- On-demand render loop: static wallpapers draw once and sleep.
- One shared wgpu device, one swapchain per monitor.
- Vsync- or timestamp-driven pacing, never a busy loop.
- Full GPU teardown on pause, with keyframe-aligned resume.
- Hysteresis on occlusion detection to avoid pause/resume thrash.
- Lazy thumbnail loading with disk-cached WebP thumbnails.
- Destroy the settings webview when closed instead of hiding it, if measurements confirm the memory win.
- Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` for the renderer, `opt-level = 3`. Add PGO only after profiling.

## 6. Benchmark Harness

Build this at Milestone 0.5, before the features it will judge.

| Metric | Tool |
|---|---|
| CPU % and RAM | `sysinfo` or Windows performance counters |
| GPU % and VRAM | DXGI/PDH counters, PresentMon |
| Frame pacing, dropped frames | PresentMon or custom timestamps |
| Power draw | Windows energy estimation or external meter |
| Pause/resume latency | Scripted window-event timestamps |
| Startup time | Cold-launch timer |

### Benchmark Scenarios

Run each scenario against Wallpaper Engine and Lively on identical hardware and content.

1. Idle 10 minutes with a static image.
2. 1080p30 H.264 loop, 10 minutes.
3. 4K60 HEVC or AV1 loop.
4. Two monitors with different content.
5. Fullscreen game on monitor 1 while monitor 2 stays visible.
6. Hot-unplug a monitor and an audio device.

Store results in the repository and add a CI regression check that fails on idle-CPU increases.

## 7. UI/UX Design Guide

- **Layout:** Left sidebar (Library, Monitors, Playlists, Settings) with a thumbnail card grid. Monitor view shows clickable rectangles for assignment.
- **Design system:** shadcn/ui or Radix primitives, 8px spacing grid, design tokens in one `tokens.css`.
- **Typography and color:** Inter or Geist, 4-step type scale, dark mode default, WCAG AA contrast.
- **Motion:** 150–250 ms ease-out transitions, respecting `prefers-reduced-motion`.
- **Feedback:** Skeleton loaders for thumbnails, toasts for errors, live preview on hover.
- **Tray popover:** Quick per-monitor on/off and volume control.
- **Accessibility:** Full keyboard navigation, screen-reader labels, reduced-motion support.

## 8. Incremental Roadmap

| # | Milestone | Done when |
|---|---|---|
| 0 | Repo setup: Cargo workspace, Tauri v2 + Vite, CI (`clippy`, `fmt`, tests), `.cursorrules` | `cargo tauri dev` opens a blank window |
| 0.5 | Benchmark harness with the six scenarios, baselines recorded | Baseline numbers exist for Wallpaper Engine and Lively |
| 1 | Tray and lifecycle: tray icon, hide-on-close, single instance, autostart | App lives in tray, no taskbar button |
| 2 | Desktop attach: `DesktopHost` trait, WorkerW reparenting | Colored layer sits behind icons, survives Show Desktop |
| 3 | Static image on one monitor, on-demand rendering | Image displays; no frames submitted while idle |
| 4 | Multi-monitor with hotplug handling | Different image per monitor |
| 4.5 | Per-monitor occlusion detector | Covering window on A pauses only A |
| 5 | Config and IPC: typed commands, persisted state, events | Settings survive restart |
| 6 | Settings UI v1: library grid, monitor picker, apply | Full image workflow via UI |
| 7 | Video playback: hardware decode, zero-copy, frame cap, mute | MP4 plays at target FPS with no CPU readback |
| 8 | Resource governor, idle-floor tests, audio hot-swap | CPU/RAM/GPU targets met |
| 9 | Playlists, scheduling, GIF/WebP animation | Timed rotation works |
| 9b | Sandboxed web wallpaper runtime | Web wallpapers run isolated with network off by default |
| 10 | Polish: installer, updater, crash logging, a11y pass | Beta release |
| 10b | Wallpaper Engine importer (video and web types first) | Imported wallpapers render correctly |
| 11 | macOS and Linux backends | Same trait, new platform implementations |

## 9. Image Upscaler (Future)

- Run Real-ESRGAN or similar via `ort` (ONNX Runtime) with DirectML or CUDA, in a background job queue.
- Upscale once on import and cache the result; never upscale at render time.
- Provide a before/after slider, estimated time, and cancel support.
- Keep it behind a `Processor` trait so other filters can plug in later.

## 10. Risks

| Risk | Mitigation |
|---|---|
| WorkerW behavior varies across Windows versions and multi-monitor setups | Test on Windows 10 and 11 at Milestone 2 |
| Video codec licensing and hardware support | Prototype libmpv and ffmpeg at Milestone 7 |
| Zero-copy texture sharing is platform-specific | Isolate behind a `TextureSource` trait |
| Web wallpaper sandbox escape | Default-deny network, per-wallpaper permissions, process isolation |
| Performance targets may be unrealistic on some hardware | Treat targets as hypotheses; benchmark before committing |
| Wallpaper Engine scene format is complex | Defer scene import to Milestone 10b, start with video/web types |

## 11. Success Criteria

Background Customizer 1.0 is "better on performance" when, on the same machine and same content, it achieves:

- Lower or equal idle CPU, GPU, RAM, and power than Wallpaper Engine and Lively in every benchmark scenario.
- Per-monitor pausing that Wallpaper Engine does not do.
- True GPU idle when a wallpaper is paused.
- No CPU readback during video playback.
- Crash isolation: one bad wallpaper never takes down the shell.

## 12. Cursor AI Workflow

- Store this spec as `docs/SPEC.md` and reference it with `@SPEC.md` in prompts.
- Add `.cursorrules` with the stack, crate layout, and rules: no `unwrap()` outside tests, all OS calls behind traits, one milestone per commit.
- Prompt one milestone at a time: ask for a plan, then implementation, then tests.
- Review all `unsafe` Win32 code manually and keep it in one isolated module.
