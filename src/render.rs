//! Render pipeline: file classification + markdown-to-HTML.
//!
//! Safe against broken markdown, non-UTF8 input, and large files. See
//! `specs/mdv-rendering.md` for the contract.

use std::fs;
use std::path::{Path, PathBuf};

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

use crate::assets;

/// Bytes sampled from the head of the file for classification.
const SNIFF_BYTES: usize = 8 * 1024;

/// Files at or above this size use chunked streaming.
pub const STREAM_THRESHOLD: u64 = 512 * 1024;

/// Number of lines in the initial synchronous chunk for large files.
pub const STREAM_INITIAL_LINES: usize = 400;

/// Lines per subsequent streamed chunk.
pub const STREAM_CHUNK_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocKind {
    Markdown,
    PlainText,
    NonText { label: String },
}

#[derive(Debug, Clone)]
pub struct Classification {
    pub kind: DocKind,
    pub size_bytes: u64,
}

pub fn classify(path: &Path) -> std::io::Result<Classification> {
    let meta = fs::metadata(path)?;
    let size_bytes = meta.len();

    let mut f = fs::File::open(path)?;
    let mut buf = vec![0u8; SNIFF_BYTES.min(size_bytes as usize + 1)];
    let n = {
        use std::io::Read;
        f.read(&mut buf)?
    };
    buf.truncate(n);

    // Magic-byte classifications.
    if let Some(label) = sniff_magic(&buf) {
        return Ok(Classification {
            kind: DocKind::NonText { label },
            size_bytes,
        });
    }

    // UTF-8 check on the sample. If the sample is valid UTF-8 we accept the
    // rest of the file too; if not, call it binary.
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s,
        Err(_) => {
            return Ok(Classification {
                kind: DocKind::NonText {
                    label: "binary (non-UTF8)".into(),
                },
                size_bytes,
            });
        }
    };

    // Empty file: treat as plain text to avoid zero-division in density.
    if text.trim().is_empty() {
        return Ok(Classification {
            kind: DocKind::PlainText,
            size_bytes,
        });
    }

    // Density-based heuristic: count structural md tokens.
    let tokens = text
        .lines()
        .map(|l| {
            let t = l.trim_start();
            (t.starts_with('#') && t.starts_with("# ")
                || t.starts_with("## ")
                || t.starts_with("### ")
                || t.starts_with("#### ")
                || t.starts_with("##### ")
                || t.starts_with("###### ")) as usize
                + if t.starts_with("```") { 1 } else { 0 }
                + if t.starts_with("- ") { 1 } else { 0 }
                + if t.starts_with("* ") { 1 } else { 0 }
                + if t.starts_with("> ") { 1 } else { 0 }
                + if t.starts_with("| ") { 1 } else { 0 }
                + if t.starts_with("---") { 1 } else { 0 }
        })
        .sum::<usize>();
    let lines = text.lines().count().max(1);
    let density = tokens as f32 / lines as f32;

    let kind = if density < 0.02 && !looks_like_inline_md(text) {
        DocKind::PlainText
    } else {
        DocKind::Markdown
    };

    Ok(Classification { kind, size_bytes })
}

fn looks_like_inline_md(text: &str) -> bool {
    // Cheap check: presence of link, image, emphasis, or math markers.
    text.contains("](")
        || text.contains("**")
        || text.contains("$$")
        || text.contains("$")
        || text.contains("```")
}

fn sniff_magic(buf: &[u8]) -> Option<String> {
    let starts = |sig: &[u8]| buf.len() >= sig.len() && &buf[..sig.len()] == sig;
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png".into());
    }
    if starts(b"\xFF\xD8\xFF") {
        return Some("image/jpeg".into());
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some("image/gif".into());
    }
    if starts(b"%PDF-") {
        return Some("application/pdf".into());
    }
    if starts(b"PK\x03\x04") {
        return Some("application/zip (or office)".into());
    }
    if starts(b"\x7FELF") {
        return Some("application/x-elf".into());
    }
    if starts(b"\xFF\xFE") || starts(b"\xFE\xFF") {
        return Some("text/utf-16 (not supported)".into());
    }
    if starts(b"RIFF") && buf.len() > 8 && &buf[8..12] == b"WEBP" {
        return Some("image/webp".into());
    }
    None
}

// --------------- markdown rendering ---------------

pub struct RenderResult {
    pub title: String,
    pub body_html: String,
    pub banners: Vec<Banner>,
    pub streaming: Option<StreamState>,
}

#[derive(Debug, Clone)]
pub struct Banner {
    pub level: BannerLevel,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub enum BannerLevel {
    Warn,
    Stream,
    Info,
}

#[derive(Debug, Clone)]
pub struct StreamState {
    pub total_bytes: u64,
    pub emitted_lines: usize,
    pub total_lines: usize,
}

/// Render a classified file. For non-markdown kinds, returns a body suitable
/// for injecting into the main content container.
pub fn render_file(path: &Path, cls: &Classification) -> std::io::Result<RenderResult> {
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("mdv")
        .to_string();
    match &cls.kind {
        DocKind::NonText { label } => {
            let html = render_nonrender_card(path, label, cls.size_bytes);
            Ok(RenderResult {
                title,
                body_html: html,
                banners: vec![],
                streaming: None,
            })
        }
        DocKind::PlainText => {
            let raw = fs::read_to_string(path).unwrap_or_else(|e| format!("<error: {}>", e));
            let escaped = html_escape(&raw);
            let body = format!("<pre class=\"plain\">{}</pre>", escaped);
            Ok(RenderResult {
                title,
                body_html: body,
                banners: vec![Banner {
                    level: BannerLevel::Info,
                    text: "plain text - no markdown structure detected".into(),
                }],
                streaming: None,
            })
        }
        DocKind::Markdown => render_markdown_file(path, cls),
    }
}

fn render_markdown_file(path: &Path, cls: &Classification) -> std::io::Result<RenderResult> {
    let raw = fs::read_to_string(path)?;
    let total_lines = raw.lines().count();

    let (initial_src, streaming) = if cls.size_bytes > STREAM_THRESHOLD {
        let head: String = raw
            .lines()
            .take(STREAM_INITIAL_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        (
            head,
            Some(StreamState {
                total_bytes: cls.size_bytes,
                emitted_lines: STREAM_INITIAL_LINES.min(total_lines),
                total_lines,
            }),
        )
    } else {
        (raw.clone(), None)
    };

    let mut banners = Vec::new();
    for w in detect_warnings(&raw) {
        banners.push(Banner {
            level: BannerLevel::Warn,
            text: w,
        });
    }
    if let Some(s) = &streaming {
        banners.push(Banner {
            level: BannerLevel::Stream,
            text: format!(
                "STREAMING - {} of {} lines ({:.1} KB)",
                s.emitted_lines,
                s.total_lines,
                s.total_bytes as f64 / 1024.0
            ),
        });
    }

    let body_html = md_to_html(&initial_src, path);
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("mdv")
        .to_string();
    Ok(RenderResult {
        title,
        body_html,
        banners,
        streaming,
    })
}

/// Convert a markdown string to inline HTML, with syntect code highlighting
/// and mdv:// link rewriting. Math delimiters are protected from CommonMark's
/// backslash-escape handling so KaTeX auto-render sees them verbatim in the
/// browser.
pub fn md_to_html(md: &str, source_dir: &Path) -> String {
    // 1. Replace math spans with placeholders that survive CommonMark intact.
    let (stashed, math_store) = stash_math(md);

    // 2. Disable SMART_PUNCTUATION - it turns ' into ’ inside paragraph text
    //    which can corrupt math-adjacent content; let KaTeX get the raw chars.
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(&stashed, opts);

    // We walk events so we can (a) rewrite links, (b) handle code blocks via
    // syntect. Pure html::push_html would miss both.
    let events = transform_events(parser, source_dir);

    let mut out = String::with_capacity(stashed.len() * 2);
    pulldown_cmark::html::push_html(&mut out, events.into_iter());

    // 3. Restore math spans verbatim.
    restore_math(&out, &math_store)
}

/// Placeholder token shape: `MDV0MATH0<index>0MDV` - all-alphanumeric so
/// CommonMark treats it as plain text (no characters that trigger parsing).
fn math_placeholder(idx: usize) -> String {
    format!("MDV0MATH0{}0MDV", idx)
}

/// Scan the source for math spans, swap each for a placeholder, and return
/// (modified_source, Vec<original_span_with_delimiters>).
///
/// Skips code fences (``` ... ```), tildes (~~~ ... ~~~), and inline code
/// (`...`) so latex-looking tokens inside code blocks are left alone.
fn stash_math(src: &str) -> (String, Vec<String>) {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut store: Vec<String> = Vec::new();
    let mut i = 0usize;

    // Track fenced-code state line-by-line, but still step character by
    // character so in-line math is correctly skipped on non-fence lines.
    let mut in_fence: Option<char> = None; // Some('`') or Some('~')
    let mut at_line_start = true;

    while i < bytes.len() {
        // Check for fence transitions at the start of a line.
        if at_line_start {
            // Consume leading spaces (up to 3 are permitted before a fence).
            let mut j = i;
            let mut spaces = 0;
            while j < bytes.len() && bytes[j] == b' ' && spaces < 3 {
                j += 1;
                spaces += 1;
            }
            // Check for a fence marker.
            let fence_ch = if j + 2 < bytes.len()
                && (bytes[j] == b'`' && bytes[j + 1] == b'`' && bytes[j + 2] == b'`')
            {
                Some('`')
            } else if j + 2 < bytes.len()
                && (bytes[j] == b'~' && bytes[j + 1] == b'~' && bytes[j + 2] == b'~')
            {
                Some('~')
            } else {
                None
            };

            if let Some(ch) = fence_ch {
                // Toggle fence state. Copy this entire line verbatim.
                match in_fence {
                    None => in_fence = Some(ch),
                    Some(cur) if cur == ch => in_fence = None,
                    _ => {}
                }
                // Copy the line.
                while i < bytes.len() && bytes[i] != b'\n' {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() {
                    out.push('\n');
                    i += 1;
                }
                at_line_start = true;
                continue;
            }
        }

        // Inside a fenced code block: copy verbatim, no math extraction.
        if in_fence.is_some() {
            let c = bytes[i] as char;
            out.push(c);
            at_line_start = c == '\n';
            i += 1;
            continue;
        }

        // Inline code `...`: skip math inside.
        if bytes[i] == b'`' {
            // How many backticks open the span.
            let mut open = 0usize;
            while i + open < bytes.len() && bytes[i + open] == b'`' {
                open += 1;
            }
            let span_start = i;
            i += open;
            // Find matching run of the same length.
            let mut closed = false;
            while i + open <= bytes.len() {
                if bytes[i..].starts_with(&vec![b'`'; open][..]) {
                    // Ensure exactly `open` backticks (not longer).
                    let mut run = 0usize;
                    while i + run < bytes.len() && bytes[i + run] == b'`' {
                        run += 1;
                    }
                    if run == open {
                        i += open;
                        closed = true;
                        break;
                    } else {
                        i += run;
                    }
                } else {
                    i += 1;
                }
            }
            // Copy the whole inline code span (or the unclosed tail) verbatim.
            let end = if closed { i } else { bytes.len() };
            out.push_str(&src[span_start..end]);
            at_line_start = false;
            continue;
        }

        // Display math: $$...$$
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'$' {
            if let Some(end) = find_close(src, i + 2, "$$") {
                let math = &src[i..end + 2];
                let idx = store.len();
                store.push(math.to_string());
                out.push_str(&math_placeholder(idx));
                i = end + 2;
                at_line_start = false;
                continue;
            }
        }

        // Display math: \[...\]
        if i + 1 < bytes.len() && bytes[i] == b'\\' && bytes[i + 1] == b'[' {
            if let Some(end) = find_close(src, i + 2, "\\]") {
                let math = &src[i..end + 2];
                let idx = store.len();
                store.push(math.to_string());
                out.push_str(&math_placeholder(idx));
                i = end + 2;
                at_line_start = false;
                continue;
            }
        }

        // Inline math: \(...\)
        if i + 1 < bytes.len() && bytes[i] == b'\\' && bytes[i + 1] == b'(' {
            if let Some(end) = find_close(src, i + 2, "\\)") {
                let math = &src[i..end + 2];
                let idx = store.len();
                store.push(math.to_string());
                out.push_str(&math_placeholder(idx));
                i = end + 2;
                at_line_start = false;
                continue;
            }
        }

        // Inline math: $...$  (single-dollar). Guard against currency:
        //   - must not be followed by whitespace right after opening $
        //   - closing $ must not be preceded by whitespace
        //   - must not be followed by a digit in closing position (avoids "$5 for $10")
        if bytes[i] == b'$' {
            if let Some(end) = find_inline_dollar_close(src, i + 1) {
                let math = &src[i..end + 1];
                let idx = store.len();
                store.push(math.to_string());
                out.push_str(&math_placeholder(idx));
                i = end + 1;
                at_line_start = false;
                continue;
            }
        }

        let c = bytes[i] as char;
        out.push(c);
        at_line_start = c == '\n';
        i += 1;
    }

    (out, store)
}

fn find_close(src: &str, start: usize, delim: &str) -> Option<usize> {
    let bytes = src.as_bytes();
    let d = delim.as_bytes();
    let mut i = start;
    while i + d.len() <= bytes.len() {
        if &bytes[i..i + d.len()] == d {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Inline `$..$` closer: require non-whitespace on the inside of both delimiters
/// so we don't latch onto prose-dollars like "$5 and $10".
fn find_inline_dollar_close(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    // Opening must be followed by a non-space non-newline.
    if start >= bytes.len() {
        return None;
    }
    let first = bytes[start];
    if first == b' ' || first == b'\t' || first == b'\n' {
        return None;
    }

    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => return None, // inline math doesn't cross paragraph breaks
            b'$' => {
                // Previous byte must be non-space.
                let prev = bytes[i - 1];
                if prev == b' ' || prev == b'\t' || prev == b'\n' {
                    i += 1;
                    continue;
                }
                // $$ inside: bail; the outer scanner should have caught it.
                if i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                    return None;
                }
                return Some(i);
            }
            b'\\' => {
                // Skip the escape + next byte (e.g. \$).
                i += 2;
                continue;
            }
            _ => i += 1,
        }
    }
    None
}

fn restore_math(html: &str, store: &[String]) -> String {
    let mut out = html.to_string();
    for (idx, math) in store.iter().enumerate() {
        let placeholder = math_placeholder(idx);
        // Math can appear inside `<p>`, `<li>`, `<td>`, etc. Replace all.
        // The math string itself is opaque to HTML - KaTeX handles it in JS -
        // but we must not HTML-escape the backslashes or braces.
        // It should, however, be safe against any HTML special chars embedded
        // in the math. We escape < > & only.
        let safe = math
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out = out.replace(&placeholder, &safe);
    }
    out
}

fn transform_events<'a>(parser: Parser<'a>, source_dir: &Path) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::new();
    let mut in_code_block: Option<String> = None; // language hint
    let mut code_buf = String::new();

    for ev in parser {
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) => {
                in_code_block = Some(lang.to_string());
                code_buf.clear();
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                in_code_block = Some(String::new());
                code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                let lang = in_code_block.take().unwrap_or_default();
                let html = highlight_code(&code_buf, &lang);
                out.push(Event::Html(html.into()));
            }
            Event::Text(ref t) if in_code_block.is_some() => {
                code_buf.push_str(t);
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let rewritten = rewrite_link(&dest_url, source_dir);
                out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: rewritten.into(),
                    title,
                    id,
                }));
            }
            other => out.push(other),
        }
    }
    out
}

fn rewrite_link(url: &str, source_dir: &Path) -> String {
    // Leave non-local schemes alone.
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("ftp://")
        || lower.starts_with("mdv://")
    {
        return url.to_string();
    }
    // Only rewrite .md local links; leave images / anchors alone.
    if !lower.ends_with(".md") && !lower.contains(".md#") {
        return url.to_string();
    }
    let (path_part, anchor) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let candidate: PathBuf = if Path::new(path_part).is_absolute() {
        PathBuf::from(path_part)
    } else {
        source_dir.parent().unwrap_or(source_dir).join(path_part)
    };
    let resolved = candidate.to_string_lossy().to_string();
    format!("mdv://nav/{}{}", urlencoding::encode(&resolved), anchor)
}

fn highlight_code(code: &str, lang: &str) -> String {
    use once_cell::sync::Lazy;
    static SS: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
    static TS: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

    let syntax = if !lang.is_empty() {
        SS.find_syntax_by_token(lang)
            .unwrap_or(SS.find_syntax_plain_text())
    } else {
        SS.find_syntax_plain_text()
    };
    let theme = &TS.themes["base16-ocean.dark"];

    match highlighted_html_for_string(code, &SS, syntax, theme) {
        Ok(html) => {
            // syntect returns <pre style="...">...</pre>. Wrap language class
            // so future CSS can latch on.
            format!(
                "<div class=\"mdv-code\" data-lang=\"{}\">{}</div>",
                html_escape(lang),
                html
            )
        }
        Err(_) => format!("<pre><code>{}</code></pre>", html_escape(code)),
    }
}

fn render_nonrender_card(path: &Path, label: &str, size: u64) -> String {
    let size_str = human_size(size);
    format!(
        "<div class=\"mdv-nonrender\">\
            <div class=\"mdv-nonrender-title\">[ NON-RENDERABLE ]</div>\
            <dl>\
                <dt>kind</dt><dd>{}</dd>\
                <dt>size</dt><dd>{}</dd>\
                <dt>path</dt><dd>{}</dd>\
            </dl>\
            <p>mdv displays markdown and text files. this file is neither.</p>\
         </div>",
        html_escape(label),
        size_str,
        html_escape(&path.display().to_string()),
    )
}

fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        return format!("{} B", bytes);
    }
    if b < 1024.0 * 1024.0 {
        return format!("{:.1} KB", b / 1024.0);
    }
    if b < 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.1} MB", b / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", b / (1024.0 * 1024.0 * 1024.0))
}

pub fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

/// Produce human-readable parse warnings without failing the render.
#[allow(dead_code)] // used in tests
pub fn detect_warnings(md: &str) -> Vec<String> {
    let mut w = Vec::new();

    // Unbalanced fenced code blocks.
    let fence_count = md
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("```") || t.starts_with("~~~")
        })
        .count();
    if fence_count % 2 != 0 {
        w.push(format!("{} unclosed code fence", 1));
    }

    // Table column-count mismatches.
    let mut in_table = false;
    let mut expected_cols: Option<usize> = None;
    let mut mismatches = 0usize;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with('|') && t.ends_with('|') {
            let cols = t.matches('|').count() - 1;
            if !in_table {
                in_table = true;
                expected_cols = Some(cols);
            } else if let Some(exp) = expected_cols {
                if cols != exp && !t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                    mismatches += 1;
                }
            }
        } else if !t.is_empty() {
            in_table = false;
            expected_cols = None;
        }
    }
    if mismatches > 0 {
        w.push(format!("{} table rows with column mismatch", mismatches));
    }

    w
}

/// Render the next streaming chunk starting at `cursor` in `all_lines`.
///
/// Returns `(rendered_html, lines_consumed)`. The caller should advance its
/// cursor by `lines_consumed` after calling this.
pub fn render_chunk(all_lines: &[String], cursor: usize, source_path: &Path) -> (String, usize) {
    let slice = &all_lines[cursor..];
    let chunk: Vec<&str> = slice
        .iter()
        .take(STREAM_CHUNK_LINES)
        .map(|s| s.as_str())
        .collect();
    let consumed = chunk.len();
    let src = chunk.join("\n");
    let html = md_to_html(&src, source_path);
    (html, consumed)
}

/// Render the complete HTML page ready for the webview.
pub fn build_page(result: &RenderResult, review_mode: bool) -> String {
    let banners_html = if result.banners.is_empty() {
        String::new()
    } else {
        result
            .banners
            .iter()
            .map(|b| {
                let class = match b.level {
                    BannerLevel::Warn => "mdv-banner-warn",
                    BannerLevel::Stream => "mdv-banner-stream",
                    BannerLevel::Info => "mdv-banner-info",
                };
                let prefix = match b.level {
                    BannerLevel::Warn => "[ PARSE NOTICES ] ",
                    BannerLevel::Stream => "",
                    BannerLevel::Info => "[ NOTE ] ",
                };
                format!(
                    "<div class=\"mdv-banner {}\"><b>{}</b>{}</div>",
                    class,
                    prefix,
                    html_escape(&b.text),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let body_class = if review_mode {
        " class=\"mdv-review-on\""
    } else {
        ""
    };

    let page = assets::HTML_TEMPLATE
        .replace("__TITLE__", &html_escape(&result.title))
        .replace("__STYLE__", assets::STYLE_CSS)
        .replace("__BANNERS__", &banners_html)
        .replace("__CONTENT__", &result.body_html);

    // Splice the body class in. Template uses `<body>` literally.
    page.replacen("<body>", &format!("<body{}>", body_class), 1)
}

// --------------- Unit tests ---------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Helper: assert the output of md_to_html contains `needle`.
    fn html_contains(md: &str, needle: &str) -> bool {
        let html = md_to_html(md, Path::new("/tmp"));
        html.contains(needle)
    }

    // ---- Math preservation through pulldown-cmark ----

    #[test]
    fn inline_dollar_math_preserved() {
        // The $...$ span must survive CommonMark parsing verbatim.
        let md = "The constant is $\\alpha \\approx 1/137$";
        let html = md_to_html(md, Path::new("/tmp"));
        assert!(
            html.contains("$\\alpha \\approx 1/137$"),
            "inline dollar math was mangled: {}",
            html
        );
    }

    #[test]
    fn display_dollar_math_preserved() {
        let md = "$$\n\\int_{0}^{1} x\\,dx = \\frac{1}{2}\n$$";
        let html = md_to_html(md, Path::new("/tmp"));
        assert!(html.contains("$$"), "display $$ math was mangled: {}", html);
        assert!(html.contains("\\int"), "LaTeX command lost: {}", html);
    }

    #[test]
    fn bracket_display_math_preserved() {
        let md = "\\[\n E = mc^2\n\\]";
        let html = md_to_html(md, Path::new("/tmp"));
        assert!(html.contains("\\["), "\\[ lost: {}", html);
        assert!(html.contains("E = mc^2"), "content lost: {}", html);
        assert!(html.contains("\\]"), "\\] lost: {}", html);
    }

    #[test]
    fn paren_inline_math_preserved() {
        let md = "divergence \\( \\nabla \\cdot \\mathbf{E} \\)";
        let html = md_to_html(md, Path::new("/tmp"));
        assert!(html.contains("\\("), "\\( lost: {}", html);
        assert!(html.contains("\\nabla"), "latex command lost: {}", html);
        assert!(html.contains("\\)"), "\\) lost: {}", html);
    }

    #[test]
    fn currency_dollars_not_stashed() {
        // "$5 and $10" must not be treated as math.
        let md = "Costs $5 and $10 at most.";
        let html = md_to_html(md, Path::new("/tmp"));
        // The literal dollar signs should appear in the output unchanged.
        assert!(html.contains("$5"), "currency dollar was removed: {}", html);
        assert!(
            html.contains("$10"),
            "currency dollar was removed: {}",
            html
        );
    }

    #[test]
    fn math_inside_code_fence_untouched() {
        // LaTeX-looking content inside a fenced code block must be left alone
        // (not stashed, not rendered by KaTeX).
        let md = "```\nlet x = $alpha + $beta;\n```";
        let html = md_to_html(md, Path::new("/tmp"));
        // It should appear as code, not as a math span.
        assert!(
            html.contains("$alpha"),
            "dollar inside code was removed: {}",
            html
        );
    }

    #[test]
    fn math_and_code_in_same_document() {
        let md = "Math: $x^2$\n\n```rust\nlet y = 1;\n```";
        let html = md_to_html(md, Path::new("/tmp"));
        assert!(html.contains("$x^2$"), "math lost: {}", html);
        // syntect syntax-highlights code, splitting tokens across <span> elements,
        // so "let y = 1" won't appear verbatim. Check for the wrapper div and
        // that the keyword is present somewhere in the highlighted output.
        assert!(html.contains("mdv-code"), "code wrapper missing: {}", html);
        assert!(
            html.contains("let"),
            "let keyword missing from highlighted code: {}",
            html
        );
    }

    // ---- Link rewriting ----

    #[test]
    fn relative_md_link_rewritten_to_mdv_nav() {
        let html = md_to_html("[next](other.md)", Path::new("/docs/index.md"));
        assert!(html.contains("mdv://nav/"), "link not rewritten: {}", html);
    }

    #[test]
    fn http_link_not_rewritten() {
        let html = md_to_html("[site](https://example.com)", Path::new("/tmp"));
        assert!(
            html.contains("https://example.com"),
            "http link mangled: {}",
            html
        );
        assert!(
            !html.contains("mdv://nav/"),
            "http link incorrectly rewritten: {}",
            html
        );
    }

    #[test]
    fn anchor_only_link_not_rewritten() {
        let html = md_to_html("[section](#intro)", Path::new("/tmp"));
        assert!(html.contains("#intro"), "anchor lost: {}", html);
        assert!(
            !html.contains("mdv://nav/"),
            "anchor incorrectly rewritten: {}",
            html
        );
    }

    // ---- detect_warnings ----

    #[test]
    fn warns_on_unclosed_fence() {
        let md = "```rust\nfn main() {}\n// no closing fence";
        let w = detect_warnings(md);
        assert!(!w.is_empty(), "expected warning for unclosed fence");
        assert!(
            w[0].contains("unclosed"),
            "unexpected warning text: {:?}",
            w
        );
    }

    #[test]
    fn no_warning_for_clean_doc() {
        let md = "# Title\n\nNormal paragraph.\n\n```rust\nlet x = 1;\n```\n";
        let w = detect_warnings(md);
        assert!(w.is_empty(), "unexpected warnings: {:?}", w);
    }

    // ---- html_escape ----

    #[test]
    fn html_escape_covers_all_special_chars() {
        let s = "<b>alert('x')</b> & \"quoted\"";
        let e = html_escape(s);
        // Each dangerous char must be replaced with its entity form.
        assert!(e.contains("&lt;"), "&lt; missing: {}", e);
        assert!(e.contains("&gt;"), "&gt; missing: {}", e);
        assert!(e.contains("&amp;"), "&amp; missing: {}", e);
        assert!(e.contains("&quot;"), "&quot; missing: {}", e);
        assert!(e.contains("&#39;"), "&#39; missing: {}", e);
        // Original unescaped tag structures must be gone.
        assert!(!e.contains("<b>"), "raw <b> still present: {}", e);
        assert!(!e.contains("</b>"), "raw </b> still present: {}", e);
    }

    // ---- render_chunk ----

    #[test]
    fn render_chunk_returns_correct_slice() {
        let lines: Vec<String> = (0..50).map(|i| format!("line {}", i)).collect();
        let (html, consumed) = render_chunk(&lines, 0, Path::new("/tmp/doc.md"));
        // consumed should be STREAM_CHUNK_LINES or lines.len(), whichever is smaller
        assert_eq!(consumed, STREAM_CHUNK_LINES.min(lines.len()));
        assert!(html.contains("line 0"), "first line missing from chunk");
    }

    #[test]
    fn render_chunk_respects_cursor() {
        let lines: Vec<String> = (0..50).map(|i| format!("para {}", i)).collect();
        let (html, consumed) = render_chunk(&lines, 10, Path::new("/tmp/doc.md"));
        assert!(html.contains("para 10"), "cursor not respected: {}", html);
        assert!(
            !html.contains("para 9"),
            "lines before cursor included: {}",
            html
        );
        let _ = consumed;
    }

    #[test]
    fn render_chunk_at_end_returns_zero() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let (html, consumed) = render_chunk(&lines, 2, Path::new("/tmp/doc.md"));
        assert_eq!(consumed, 0, "expected 0 lines consumed at end");
        assert!(
            html.is_empty() || html.trim().is_empty() || html.contains('\n'),
            "unexpected html for empty chunk: {:?}",
            html
        );
    }

    // ---- classify ----

    #[test]
    fn classify_empty_file_is_plaintext() {
        use std::io::Write;
        let mut f = tempfile_helper();
        f.flush().unwrap();
        // We need a real Path; use a named tempfile approach via fs.
        let tmp = std::env::temp_dir().join("mdv_test_empty.md");
        std::fs::write(&tmp, b"").unwrap();
        let cls = classify(&tmp).unwrap();
        assert!(
            matches!(cls.kind, DocKind::PlainText),
            "empty file should be PlainText"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn classify_png_magic_bytes_is_nontext() {
        let tmp = std::env::temp_dir().join("mdv_test_fake.md");
        // Write PNG magic bytes.
        std::fs::write(&tmp, b"\x89PNG\r\n\x1a\n fake png content").unwrap();
        let cls = classify(&tmp).unwrap();
        assert!(
            matches!(cls.kind, DocKind::NonText { .. }),
            "PNG magic should be NonText"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn classify_markdown_file_is_markdown() {
        let tmp = std::env::temp_dir().join("mdv_test_md.md");
        let content = "# Title\n\n- item 1\n- item 2\n\n```rust\nlet x = 1;\n```\n";
        std::fs::write(&tmp, content).unwrap();
        let cls = classify(&tmp).unwrap();
        assert!(
            matches!(cls.kind, DocKind::Markdown),
            "markdown file should be Markdown"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn classify_plain_text_file_is_plaintext() {
        let tmp = std::env::temp_dir().join("mdv_test_plain.txt");
        // No markdown tokens at all.
        let content = "This is a normal text file.\nNo special formatting here.\nJust words.\n";
        std::fs::write(&tmp, content).unwrap();
        let cls = classify(&tmp).unwrap();
        assert!(
            matches!(cls.kind, DocKind::PlainText),
            "plain text file should be PlainText"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    // small helper so we can have a writable temp file handle
    fn tempfile_helper() -> std::fs::File {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(std::env::temp_dir().join("mdv_test_scratch"))
            .unwrap()
    }
}
