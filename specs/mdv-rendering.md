# mdv - rendering

**Goals**
- Correct commonmark output.
- Tolerate broken markdown (unclosed fences, dangling refs, stray HTML): render
  what parses, surface a small warning banner above the content.
- Math: `$..$`, `$$..$$`, `\(..\)`, `\[..\]`. Via KaTeX auto-render,
  `throwOnError: false`.
- Code: syntect highlight, embedded `base16-ocean.dark` theme for dark mode (and
  a light variant swap when the user toggles theme).
- Relocate assets via `mdv-asset://` scheme custom protocol so fonts load from
  memory without touching disk.

**File sniff** (`src/render.rs::classify`)
1. Read first 8 KB.
2. Magic-byte check: PNG 89 50 4E 47, JPEG FF D8 FF, GIF 47 49 46 38, PDF 25 50 44 46,
   ZIP 50 4B 03 04, ELF 7F 45 4C 46, UTF-16 BOM FF FE / FE FF -> NonText.
3. `std::str::from_utf8` on the sample -> on error: NonText{kind: "binary (non-UTF8)"}.
4. Count structural markdown tokens (#, ```, |, ], [, -,  *, _) on sample; if
   density < 0.5% treat as PlainText.
5. Else Markdown.

**NonText card**
Brutalist panel, single color block with uppercase label:
```
[ NON-RENDERABLE ]
kind: image/png
size: 48.3 KB
path: /path/to/cat.md
mdv can display markdown and text files. this file is neither.
```

**PlainText**
Renders inside a `<pre class="plain">` with word-wrap off, tabular numerals.
No markdown parsing attempted.

**Malformed markdown**
`pulldown-cmark` never throws; we still surface a warnings banner when we detect:
- Unbalanced fence count (odd number of lines matching `^```+`).
- Table rows with column-count mismatch.
- Image/link refs without definitions.
Banner format:
```
[ PARSE NOTICES ]
- 1 unclosed code fence
- 2 table rows with column mismatch
```
Content still renders.

**Large file streaming**
- If `size_bytes > 512 * 1024`:
  - Render first 400 lines synchronously and mark the document as `streaming`.
  - After `ready` IPC, Rust sends subsequent 200-line chunks on a loop until EOF.
  - JS appends each chunk into `#streaming-tail`, then re-runs KaTeX auto-render
    on the new subtree only.
  - Top banner: `streaming... X of Y KB`.

**Link rewriting**
- `http(s)://...` left as-is, captured in JS click handler, sent via IPC as
  `{kind: "nav", target: url}`. Rust spawns via `open::that(url)`.
- Relative `.md` path: rewritten at emit-time to `mdv://nav/<path>`. JS click
  handler catches, IPC sends same `nav` kind. Rust resolves and swaps document.
- Other relative links (images, anchors) pass through unchanged.

**History**
- Each window keeps `back: Vec<PathBuf>`, `fwd: Vec<PathBuf>`.
- Navigating a new md link pushes current path onto `back`, clears `fwd`.
- `< BACK` / `FORWARD >` buttons sit at the top of the document, only enabled
  when the corresponding stack is non-empty.
