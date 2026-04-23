# mdv - architecture

**Binary:** single `mdv` executable. One OS process per CLI invocation; that
process hosts N webview windows (N >= 1, capped at 8).

**Session id:** each invocation generates a UUID v4, logged to stderr at startup
and attached to every IPC message for correlation.

**Stack**
- `wry` + `tao` - webview windows (webkit2gtk on Linux, builtin on macOS/Windows).
- `pulldown-cmark` - CommonMark + tables, strikethrough, task lists, footnotes.
- `syntect` - syntax highlighting, embedded default-newlines pack.
- `clap` v4 derive - CLI parsing.
- `uuid` v1 - invocation id.
- `open` - hand web URLs to system browser.
- `notify` (optional, deferred) - file watcher for reload-on-change.

**Linux startup behavior**
- On Wayland sessions where `DISPLAY` is available, `mdv` forces
  `GDK_BACKEND=x11` and `WEBKIT_DISABLE_COMPOSITING_MODE=1` before WebKit/GTK
  initialization so the normal `mdv ...` entry point works without an external
  launcher script.
- This does not make the Linux build static; the executable still relies on the
  system WebKitGTK/GTK shared-library stack at runtime.

**Embedded assets** (baked via `include_bytes!` so the binary is relocatable):
- KaTeX 0.16.45 CSS + JS + `auto-render` + font files (woff2 preferred).
- InterVariable.woff2.
- `template.html` + `style.css`.

**Per-window state**
- `path` - currently displayed file.
- `history_back: Vec<PathBuf>`, `history_fwd: Vec<PathBuf>`.
- `review: Option<ReviewState>` (only when invoked with `-r`).
- `render_mode: Document | PlainText | NonText { kind, info }`.
- `chunk_state: Option<StreamState>` for large files.

**Render pipeline** (see `mdv-rendering.md`)
1. Read file header bytes (first 8 KB) + metadata.
2. Sniff: binary magic -> NonText. UTF-8 invalid -> NonText("binary / non-UTF8"). UTF-8 valid but no markdown structural tokens -> PlainText. Else -> Markdown.
3. If Markdown and file > 512 KB: render first 400 lines, stream rest in 200-line chunks via IPC.
4. Markdown -> HTML via `pulldown-cmark`, with a walker that:
   - Preserves `$...$`, `$$...$$`, `\[...\]`, `\(...\)` so KaTeX auto-render gets them verbatim.
   - Highlights code blocks with syntect -> inlined HTML (no JS required for code colors).
   - Rewrites `.md` relative links to `mdv://nav/...` so the IPC handler can intercept.
5. Emit into `template.html`, load into webview via `set_html` with a file://-style base URL so relative images resolve.

**IPC protocol** (JSON over `evaluate_script` / `ipc::handler`):
```
{ "kind": "nav",       "target": "path or url" }
{ "kind": "review",    "verdict": "accept"|"reject", "text": "..." }
{ "kind": "chunk_ack", "next": <line_offset> }
{ "kind": "ready" }
```
Rust -> JS uses `window.mdv.__push(json)` via evaluate_script.

**Multi-window spawning**
- `-b f1 f2 ...` or `-d <dir>`: expand to a vec of paths, truncate to 8, warn to stderr if truncated, spawn one `Window` per path on the same event loop.
- `-r` requires exactly one input path.
