# mdv

**Fast, self-contained markdown viewer for Linux with real math rendering.**

[![CI](https://github.com/noboomu/mdv/actions/workflows/ci.yml/badge.svg)](https://github.com/noboomu/mdv/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

mdv renders markdown in a native window with full KaTeX math support, syntax
highlighting, inter-document navigation, review annotations, and large-file
streaming - all in a single ~5 MB binary with zero runtime dependencies beyond
system WebKitGTK.

No Electron. No browser tabs. No network calls. Everything is embedded.

---

## Why mdv

Every other Rust markdown viewer punts on math. mdv doesn't.

| Feature | mdv | inlyne | md-preview | ghostwriter |
|---|---|---|---|---|
| KaTeX math (all delimiters) | **yes** | no | no | MathJax (slow) |
| Review / annotation mode | **yes** | no | no | no |
| Batch + directory mode | **yes** | no | no | no |
| Large-file streaming | **yes** | no | no | no |
| Self-contained binary | **yes** | yes | yes | no (Qt) |
| Language | Rust | Rust | Rust | C++ |

mdv handles `$...$`, `$$...$$`, `\(...\)`, and `\[...\]` delimiters correctly -
including the hard cases where CommonMark's backslash escaping would normally
destroy your LaTeX. A pre-parse stash/restore pipeline preserves math verbatim
through pulldown-cmark so KaTeX sees exactly what you wrote.

## Install

### From source

```bash
# Arch
sudo pacman -S webkit2gtk-4.1 gtk3
cargo install --path .

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
cargo install --path .
```

The release binary is fully self-contained - KaTeX 0.16.45, all math fonts, the
Inter typeface, syntax themes, and the HTML template are baked in at compile time.

### System dependency

mdv uses [wry](https://github.com/tauri-apps/wry) + WebKitGTK for rendering. This is
the same class of system dependency that any GTK/Qt app requires. On a typical
Linux desktop, `webkit2gtk-4.1` is already installed or one `pacman -S` /
`apt install` away.

## Usage

```bash
mdv paper.md                     # single document
mdv -b ch1.md ch2.md ch3.md     # batch: one window per file (max 8)
mdv -d ./notes                   # directory: all *.md files (max 8)
mdv -r feedback.md paper.md     # review: render paper, append verdict to feedback.md
```

### Math rendering

All four standard delimiters work out of the box:

```markdown
Inline: $E = mc^2$ or \(E = mc^2\)

Display:
$$
\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

\[
\nabla \times \mathbf{E} = -\frac{\partial \mathbf{B}}{\partial t}
\]
```

Currency (`$5 and $10`) is not misidentified as math. Code fences and inline
code are protected - LaTeX-looking content inside `` ` `` or ```` ``` ```` blocks
is left alone.

### Review mode

The `-r` flag adds a fixed review panel at the bottom of the window. Write
feedback, then submit ACCEPT or REJECT. The verdict is appended to the output
file in a structured format other tools can parse:

```
=======
paper.md user feedback
=======
[ACCEPT] "Clear derivation, minor typo in eq. 14"
```

### Navigation

Click any local `.md` link to navigate. External URLs open in your system
browser. Back/forward buttons track your history within the session.

### Large files

Files over 512 KB stream in chunks - the first 400 lines render immediately,
then subsequent 200-line chunks arrive asynchronously. A banner shows progress
and fades out when complete.

## Design

mdv is built for people who read technical documents. The default theme is a
high-contrast dark interface with an industrial aesthetic - tabular numerals,
monospace code, clean hierarchy, no visual noise.

### Stack

- **tao** - cross-platform window management
- **wry** - system webview (WebKitGTK on Linux)
- **pulldown-cmark** - CommonMark parsing with tables, footnotes, strikethrough, task lists
- **syntect** - syntax highlighting with embedded themes
- **KaTeX 0.16.45** - math rendering (embedded, no network)
- **Inter** - variable-weight sans-serif typeface (embedded)

### Architecture

Single process, event-driven. All IPC between Rust and the webview is JSON over
`window.ipc.postMessage`. No threads, no async runtime, no network calls.

Specs in `specs/` are authoritative:

- `mdv-architecture.md` - process model, stack, IPC protocol
- `mdv-cli.md` - flags, exit codes, session ID
- `mdv-rendering.md` - file classification, math preservation, streaming
- `mdv-review.md` - review panel UI and output format

### Wayland

mdv detects Wayland sessions and applies the required WebKitGTK workaround
(`GDK_BACKEND=x11`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`) automatically before
creating the webview. On a typical XWayland-enabled desktop, `mdv paper.md`
just works.

## Build

```bash
cargo build --release
```

Binary lands in `target/release/mdv` (~5.4 MB, stripped + LTO). Copy anywhere.

```bash
cargo test      # 157 tests
cargo clippy    # lint check
```

## License

MIT - see [LICENSE](LICENSE).
