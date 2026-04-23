# Contributing to mdv

Contributions are welcome. mdv is a focused tool - the goal is a fast, correct,
self-contained markdown viewer with real math support, not a kitchen-sink editor.

## Getting started

```bash
# Dependencies (Debian/Ubuntu)
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev

# Dependencies (Arch)
sudo pacman -S webkit2gtk-4.1 gtk3

# Build
cargo build --release

# Test
cargo test
```

The release binary lands in `target/release/mdv`. All assets (KaTeX, fonts) are
embedded at compile time.

## Architecture

Specs in `specs/` are authoritative. If the code diverges from spec, update the
spec first - then the code.

- `src/main.rs` - CLI, window management, IPC routing (tao + wry)
- `src/render.rs` - Classification, markdown-to-HTML pipeline, math preservation, syntect highlighting
- `src/assets.rs` - Compile-time embedded assets (KaTeX, Inter font, template, CSS)
- `assets/` - HTML template, stylesheet, KaTeX distribution, fonts

## Before submitting a PR

1. `cargo test` must pass (53 tests)
2. `cargo clippy` should be clean
3. If you changed rendering behavior, update the relevant spec in `specs/`
4. If you added a dependency, justify it - mdv aims for a small dependency tree

## Code style

- No comments unless the *why* is non-obvious
- Match the existing patterns in `render.rs` for new rendering features
- Math preservation (the stash/restore pipeline) is load-bearing and subtle - test thoroughly if touched

## Scope

In scope: rendering correctness, math support, performance, Linux desktop integration, accessibility.

Out of scope (for now): editing, syntax-aware autocomplete, plugin systems, non-Linux GUI backends.
