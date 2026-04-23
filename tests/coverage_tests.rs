//! Extended test coverage for mdv.
//!
//! Targets gaps identified during publish prep: magic byte variants, asset
//! resolution, streaming code paths, math edge cases, link rewriting edge
//! cases, banner rendering, syntax highlighting structure, and review escaping.

use std::path::{Path, PathBuf};

#[path = "../src/render.rs"]
mod render;

#[path = "../src/assets.rs"]
mod assets;

// ---- helpers ----

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mdv_cov_{}", name))
}

fn write_tmp(name: &str, content: &[u8]) -> PathBuf {
    let p = tmp(name);
    std::fs::write(&p, content).expect("write_tmp failed");
    p
}

fn cleanup(name: &str) {
    let _ = std::fs::remove_file(tmp(name));
}

fn html(md: &str) -> String {
    render::md_to_html(md, Path::new("/docs"))
}

// ========================================================================
// Magic byte classification - each format the sniffer claims to handle
// ========================================================================

#[test]
fn classify_jpeg_magic() {
    let p = write_tmp("jpeg.bin", b"\xFF\xD8\xFF\xE0 fake jpeg");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("jpeg")));
    cleanup("jpeg.bin");
}

#[test]
fn classify_gif87a_magic() {
    let p = write_tmp("gif87.bin", b"GIF87a fake gif data here");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("gif")));
    cleanup("gif87.bin");
}

#[test]
fn classify_gif89a_magic() {
    let p = write_tmp("gif89.bin", b"GIF89a fake gif data here");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("gif")));
    cleanup("gif89.bin");
}

#[test]
fn classify_pdf_magic() {
    let p = write_tmp("pdf.bin", b"%PDF-1.7 fake pdf");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("pdf")));
    cleanup("pdf.bin");
}

#[test]
fn classify_zip_magic() {
    let p = write_tmp("zip.bin", b"PK\x03\x04 fake zip");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("zip")));
    cleanup("zip.bin");
}

#[test]
fn classify_utf16_bom_le() {
    let p = write_tmp("utf16le.bin", b"\xFF\xFE some utf16 le text");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("utf-16")));
    cleanup("utf16le.bin");
}

#[test]
fn classify_utf16_bom_be() {
    let p = write_tmp("utf16be.bin", b"\xFE\xFF some utf16 be text");
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("utf-16")));
    cleanup("utf16be.bin");
}

#[test]
fn classify_webp_magic() {
    let mut data = b"RIFF".to_vec();
    data.extend_from_slice(&[0x00; 4]); // size placeholder
    data.extend_from_slice(b"WEBP");
    data.extend_from_slice(b"fake webp data");
    let p = write_tmp("webp.bin", &data);
    let cls = render::classify(&p).unwrap();
    assert!(matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("webp")));
    cleanup("webp.bin");
}

#[test]
fn classify_non_utf8_no_magic_is_nontext() {
    // Bytes that don't match any magic signature but are invalid UTF-8.
    let p = write_tmp(
        "badbytes.bin",
        &[0x80, 0x81, 0x82, 0xFE, 0xFD, 0x90, 0x91, 0x92],
    );
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::NonText { ref label } if label.contains("non-UTF8")),
        "invalid UTF-8 without magic should be NonText, got {:?}",
        cls.kind
    );
    cleanup("badbytes.bin");
}

// ========================================================================
// Asset resolution
// ========================================================================

#[test]
fn asset_resolve_katex_css() {
    let result = assets::resolve("/katex/katex.min.css");
    assert!(result.is_some(), "KaTeX CSS should resolve");
    let (mime, bytes) = result.unwrap();
    assert!(mime.contains("text/css"), "wrong mime: {}", mime);
    assert!(!bytes.is_empty(), "KaTeX CSS bytes empty");
}

#[test]
fn asset_resolve_katex_js() {
    let result = assets::resolve("/katex/katex.min.js");
    assert!(result.is_some(), "KaTeX JS should resolve");
    let (mime, _) = result.unwrap();
    assert!(mime.contains("javascript"), "wrong mime: {}", mime);
}

#[test]
fn asset_resolve_auto_render_js() {
    let result = assets::resolve("/katex/auto-render.min.js");
    assert!(result.is_some(), "auto-render JS should resolve");
}

#[test]
fn asset_resolve_inter_font() {
    let result = assets::resolve("/fonts/InterVariable.woff2");
    assert!(result.is_some(), "Inter font should resolve");
    let (mime, _) = result.unwrap();
    assert!(mime.contains("woff2"), "wrong mime: {}", mime);
}

#[test]
fn asset_resolve_inter_italic_font() {
    let result = assets::resolve("/fonts/InterVariable-Italic.woff2");
    assert!(result.is_some(), "Inter italic font should resolve");
}

#[test]
fn asset_resolve_katex_font_main_regular() {
    let result = assets::resolve("/katex/fonts/KaTeX_Main-Regular.woff2");
    assert!(result.is_some(), "KaTeX Main-Regular font should resolve");
}

#[test]
fn asset_resolve_katex_font_math_italic() {
    let result = assets::resolve("/katex/fonts/KaTeX_Math-Italic.woff2");
    assert!(result.is_some(), "KaTeX Math-Italic font should resolve");
}

#[test]
fn asset_resolve_unknown_returns_none() {
    assert!(assets::resolve("/nonexistent/file.xyz").is_none());
    assert!(assets::resolve("/katex/fonts/NotAFont.woff2").is_none());
    assert!(assets::resolve("").is_none());
}

#[test]
fn asset_resolve_strips_leading_slashes() {
    let with_slash = assets::resolve("/katex/katex.min.css");
    let without_slash = assets::resolve("katex/katex.min.css");
    assert!(
        with_slash.is_some() && without_slash.is_some(),
        "both forms should resolve"
    );
}

// ========================================================================
// Large-file streaming code path in render_file
// ========================================================================

#[test]
fn render_file_large_markdown_triggers_streaming() {
    let mut content = String::from("# Streaming Test Document\n\n");
    for i in 0..15000 {
        content.push_str(&format!("- item {} with some content for bulk\n", i));
    }
    assert!(content.len() as u64 > render::STREAM_THRESHOLD);
    let p = write_tmp("large_stream.md", content.as_bytes());
    let cls = render::classify(&p).unwrap();
    let result = render::render_file(&p, &cls).unwrap();
    assert!(
        result.streaming.is_some(),
        "file over STREAM_THRESHOLD should trigger streaming"
    );
    let ss = result.streaming.unwrap();
    assert_eq!(ss.emitted_lines, render::STREAM_INITIAL_LINES);
    assert!(ss.total_lines > render::STREAM_INITIAL_LINES);
    assert!(ss.total_bytes > render::STREAM_THRESHOLD);
    // Should have a streaming banner
    assert!(
        result
            .banners
            .iter()
            .any(|b| matches!(b.level, render::BannerLevel::Stream)),
        "streaming banner missing"
    );
    cleanup("large_stream.md");
}

#[test]
fn render_file_small_markdown_no_streaming() {
    let p = write_tmp("small_no_stream.md", b"# Small\n\nJust a few lines.\n");
    let cls = render::classify(&p).unwrap();
    let result = render::render_file(&p, &cls).unwrap();
    assert!(result.streaming.is_none(), "small file should not stream");
    cleanup("small_no_stream.md");
}

// ========================================================================
// Math edge cases
// ========================================================================

#[test]
fn math_at_document_start() {
    let out = html("$x^2$ is the first thing");
    assert!(out.contains("$x^2$"), "math at doc start lost: {}", out);
}

#[test]
fn math_at_document_end() {
    let out = html("ends with math $y = mx + b$");
    assert!(
        out.contains("$y = mx + b$"),
        "math at doc end lost: {}",
        out
    );
}

#[test]
fn display_math_adjacent_to_text() {
    let out = html("Before\n$$\na + b = c\n$$\nAfter");
    assert!(out.contains("$$"), "display math lost: {}", out);
    assert!(out.contains("a + b = c"), "math content lost: {}", out);
}

#[test]
fn multiple_math_spans_in_one_paragraph() {
    let out = html("We have $a$, $b$, and $c$ in one line.");
    assert!(out.contains("$a$"), "first math lost: {}", out);
    assert!(out.contains("$b$"), "second math lost: {}", out);
    assert!(out.contains("$c$"), "third math lost: {}", out);
}

#[test]
fn math_with_braces_preserved() {
    let out = html("$\\frac{1}{2}$");
    assert!(out.contains("\\frac"), "frac command lost: {}", out);
    assert!(out.contains("{1}"), "braces lost: {}", out);
}

#[test]
fn math_with_subscripts_and_superscripts() {
    let out = html("$x_{i}^{2}$");
    assert!(
        out.contains("x_{i}^{2}"),
        "subscript/superscript lost: {}",
        out
    );
}

#[test]
fn tilde_fence_protects_math_content() {
    let out = html("~~~\n$not_math$\n~~~");
    assert!(
        out.contains("$not_math$"),
        "dollar inside tilde fence was eaten: {}",
        out
    );
}

#[test]
fn inline_backtick_protects_dollar() {
    let out = html("Use `$HOME` to reference home dir.");
    assert!(
        out.contains("$HOME"),
        "dollar inside inline code was eaten: {}",
        out
    );
}

#[test]
fn multi_backtick_inline_protects_content() {
    let out = html("Use `` `$x` `` for literal backtick-dollar.");
    assert!(
        out.contains("$x"),
        "dollar inside double backtick code was eaten: {}",
        out
    );
}

#[test]
fn mixed_math_delimiters_in_one_doc() {
    let md = "Inline: $a$ and \\(b\\)\n\nDisplay:\n$$c$$\n\n\\[\nd\n\\]";
    let out = html(md);
    assert!(out.contains("$a$"), "inline dollar lost: {}", out);
    assert!(out.contains("\\("), "inline paren lost: {}", out);
    assert!(out.contains("$$"), "display dollar lost: {}", out);
    assert!(out.contains("\\["), "display bracket lost: {}", out);
}

#[test]
fn unclosed_dollar_not_stashed() {
    // A lone $ with no closing $ should pass through as literal text.
    let out = html("Cost is $5 per item");
    assert!(out.contains("$5"), "lone dollar was eaten: {}", out);
}

// ========================================================================
// Link rewriting edge cases
// ========================================================================

#[test]
fn mailto_link_not_rewritten() {
    let out = html("[email](mailto:user@example.com)");
    assert!(
        out.contains("mailto:user@example.com"),
        "mailto mangled: {}",
        out
    );
    assert!(
        !out.contains("mdv://nav/"),
        "mailto incorrectly rewritten: {}",
        out
    );
}

#[test]
fn ftp_link_not_rewritten() {
    let out = html("[files](ftp://ftp.example.com/pub)");
    assert!(
        out.contains("ftp://ftp.example.com"),
        "ftp mangled: {}",
        out
    );
    assert!(
        !out.contains("mdv://nav/"),
        "ftp incorrectly rewritten: {}",
        out
    );
}

#[test]
fn image_link_not_rewritten() {
    let out = html("[photo](picture.png)");
    assert!(
        !out.contains("mdv://nav/"),
        "non-md local link should not be rewritten: {}",
        out
    );
}

#[test]
fn absolute_md_link_rewritten() {
    let out = html("[abs](/docs/other.md)");
    assert!(
        out.contains("mdv://nav/"),
        "absolute .md link not rewritten: {}",
        out
    );
}

#[test]
fn md_link_with_fragment_preserves_anchor() {
    let out = html("[section](doc.md#section-2)");
    assert!(out.contains("mdv://nav/"), "link not rewritten: {}", out);
    assert!(out.contains("#section-2"), "fragment lost: {}", out);
}

#[test]
fn http_link_case_insensitive() {
    let out = html("[site](HTTPS://EXAMPLE.COM)");
    assert!(
        out.contains("HTTPS://EXAMPLE.COM"),
        "http link mangled: {}",
        out
    );
    assert!(
        !out.contains("mdv://nav/"),
        "http link incorrectly rewritten: {}",
        out
    );
}

// ========================================================================
// build_page - banners
// ========================================================================

#[test]
fn build_page_renders_warn_banner() {
    let result = render::RenderResult {
        title: "Test".into(),
        body_html: "<p>content</p>".into(),
        banners: vec![render::Banner {
            level: render::BannerLevel::Warn,
            text: "1 unclosed code fence".into(),
        }],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(
        page.contains("mdv-banner-warn"),
        "warn banner class missing"
    );
    assert!(page.contains("PARSE NOTICES"), "warn prefix missing");
    assert!(page.contains("1 unclosed code fence"), "warn text missing");
}

#[test]
fn build_page_renders_stream_banner() {
    let result = render::RenderResult {
        title: "Big".into(),
        body_html: "<p>content</p>".into(),
        banners: vec![render::Banner {
            level: render::BannerLevel::Stream,
            text: "STREAMING - 400 of 10000 lines (600.0 KB)".into(),
        }],
        streaming: Some(render::StreamState {
            total_bytes: 614400,
            emitted_lines: 400,
            total_lines: 10000,
        }),
    };
    let page = render::build_page(&result, false);
    assert!(
        page.contains("mdv-banner-stream"),
        "stream banner class missing"
    );
    assert!(page.contains("STREAMING"), "stream text missing");
}

#[test]
fn build_page_renders_info_banner() {
    let result = render::RenderResult {
        title: "Plain".into(),
        body_html: "<pre>text</pre>".into(),
        banners: vec![render::Banner {
            level: render::BannerLevel::Info,
            text: "plain text - no markdown structure detected".into(),
        }],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(
        page.contains("mdv-banner-info"),
        "info banner class missing"
    );
    assert!(page.contains("NOTE"), "info prefix missing");
}

#[test]
fn build_page_escapes_title_html() {
    let result = render::RenderResult {
        title: "<script>alert('xss')</script>".into(),
        body_html: "<p>safe</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(
        !page.contains("<script>alert"),
        "title not escaped - XSS possible"
    );
    assert!(
        page.contains("&lt;script&gt;"),
        "title should be HTML-escaped"
    );
}

#[test]
fn build_page_contains_katex_script_tags() {
    let result = render::RenderResult {
        title: "Doc".into(),
        body_html: "<p>math here</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(page.contains("katex.min.js"), "KaTeX JS script tag missing");
    assert!(
        page.contains("auto-render.min.js"),
        "auto-render script tag missing"
    );
    assert!(page.contains("katex.min.css"), "KaTeX CSS link tag missing");
}

#[test]
fn build_page_contains_ipc_bridge() {
    let result = render::RenderResult {
        title: "Doc".into(),
        body_html: "<p>hi</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(page.contains("window.mdv"), "IPC bridge object missing");
    assert!(
        page.contains("window.ipc"),
        "IPC postMessage bridge missing"
    );
    assert!(
        page.contains("DOMContentLoaded"),
        "ready event handler missing"
    );
}

// ========================================================================
// Syntax highlighting structure
// ========================================================================

#[test]
fn code_block_produces_highlighted_wrapper() {
    let out = html("```rust\nfn main() {}\n```");
    assert!(
        out.contains("mdv-code"),
        "syntax highlight wrapper div missing: {}",
        out
    );
    assert!(
        out.contains("data-lang=\"rust\""),
        "language tag missing: {}",
        out
    );
}

#[test]
fn code_block_unknown_lang_still_renders() {
    let out = html("```zortlang\nfoo bar baz\n```");
    assert!(
        out.contains("mdv-code"),
        "wrapper div missing for unknown lang: {}",
        out
    );
    assert!(out.contains("foo bar baz"), "code content missing: {}", out);
}

#[test]
fn indented_code_block_renders() {
    let out = html("Paragraph.\n\n    indented code line\n    another line\n\nParagraph.");
    assert!(
        out.contains("indented code line"),
        "indented code missing: {}",
        out
    );
}

// ========================================================================
// Review escaping edge case
// ========================================================================

fn append_review_test(out: &Path, source: &Path, verdict: &str, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let verdict_up = verdict.to_ascii_uppercase();
    let name = source.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let block = format!(
        "=======\n{} user feedback\n=======\n[{}] \"{}\"\n\n",
        name, verdict_up, escaped,
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)?;
    f.write_all(block.as_bytes())
}

#[test]
fn review_escapes_backslashes() {
    let out = tmp("review_backslash.md");
    let _ = std::fs::remove_file(&out);
    append_review_test(
        &out,
        Path::new("/doc.md"),
        "accept",
        r"path is C:\Users\name",
    )
    .unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("C:\\\\Users\\\\name"),
        "backslashes not double-escaped: {}",
        content
    );
    cleanup("review_backslash.md");
}

#[test]
fn review_handles_empty_text() {
    let out = tmp("review_empty.md");
    let _ = std::fs::remove_file(&out);
    append_review_test(&out, Path::new("/doc.md"), "reject", "").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("[REJECT]"), "verdict missing");
    assert!(
        content.contains("\"\""),
        "empty text should produce empty quotes"
    );
    cleanup("review_empty.md");
}

// ========================================================================
// Streaming chunk edge cases
// ========================================================================

#[test]
fn stream_chunk_smaller_than_chunk_size() {
    let lines: Vec<String> = (0..10).map(|i| format!("short line {}", i)).collect();
    let (html, consumed) = render::render_chunk(&lines, 0, Path::new("/tmp/doc.md"));
    assert_eq!(consumed, 10, "should consume all 10 lines");
    assert!(html.contains("short line 0"), "first line missing");
    assert!(html.contains("short line 9"), "last line missing");
}

#[test]
fn stream_chunk_exact_boundary() {
    let lines: Vec<String> = (0..render::STREAM_CHUNK_LINES)
        .map(|i| format!("boundary {}", i))
        .collect();
    let (html, consumed) = render::render_chunk(&lines, 0, Path::new("/tmp/doc.md"));
    assert_eq!(consumed, render::STREAM_CHUNK_LINES);
    assert!(html.contains("boundary 0"), "first line missing");
}

#[test]
fn stream_multiple_chunks_cover_full_document() {
    let total = 500;
    let lines: Vec<String> = (0..total).map(|i| format!("line {}", i)).collect();
    let mut cursor = 0;
    let mut chunks = 0;
    while cursor < lines.len() {
        let (_html, consumed) = render::render_chunk(&lines, cursor, Path::new("/tmp/doc.md"));
        if consumed == 0 {
            break;
        }
        cursor += consumed;
        chunks += 1;
    }
    assert_eq!(cursor, total, "all lines should be consumed");
    assert!(
        chunks > 1,
        "should take multiple chunks for {} lines",
        total
    );
}

// ========================================================================
// detect_warnings edge cases
// ========================================================================

#[test]
fn no_warning_for_clean_table() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
    let w = render::detect_warnings(md);
    let table_warns: Vec<_> = w.iter().filter(|s| s.contains("mismatch")).collect();
    assert!(
        table_warns.is_empty(),
        "false positive table warning: {:?}",
        w
    );
}

#[test]
fn no_warning_for_empty_document() {
    let w = render::detect_warnings("");
    assert!(
        w.is_empty(),
        "empty doc should produce no warnings: {:?}",
        w
    );
}

#[test]
fn warning_for_double_unclosed_fence() {
    let md = "```\ncode\n```\n\n```\nmore code without close\n";
    let w = render::detect_warnings(md);
    assert!(
        w.iter().any(|s| s.contains("unclosed")),
        "should warn about unclosed fence: {:?}",
        w
    );
}

// ========================================================================
// html_escape completeness
// ========================================================================

#[test]
fn html_escape_empty_string() {
    assert_eq!(render::html_escape(""), "");
}

#[test]
fn html_escape_no_special_chars() {
    let input = "Hello World 123";
    assert_eq!(render::html_escape(input), input);
}

#[test]
fn html_escape_all_special_chars_together() {
    let input = "<div class=\"x\" data-val='a&b'>";
    let escaped = render::html_escape(input);
    assert!(!escaped.contains('<'), "raw < present");
    assert!(!escaped.contains('>'), "raw > present");
    assert!(!escaped.contains('"'), "raw \" present");
    assert!(escaped.contains("&amp;"), "& not escaped");
    assert!(escaped.contains("&#39;"), "' not escaped");
}

// ========================================================================
// Template and CSS are non-empty (catch broken embeds)
// ========================================================================

#[test]
fn html_template_is_valid_html() {
    assert!(
        assets::HTML_TEMPLATE.contains("<!DOCTYPE html>"),
        "template missing doctype"
    );
    assert!(
        assets::HTML_TEMPLATE.contains("</html>"),
        "template missing closing html tag"
    );
    assert!(
        assets::HTML_TEMPLATE.contains("__CONTENT__"),
        "template missing content placeholder"
    );
    assert!(
        assets::HTML_TEMPLATE.contains("__TITLE__"),
        "template missing title placeholder"
    );
}

#[test]
fn style_css_is_non_empty_and_has_theme() {
    assert!(!assets::STYLE_CSS.is_empty(), "embedded CSS is empty");
    assert!(
        assets::STYLE_CSS.contains("--bg"),
        "CSS missing theme variables"
    );
    assert!(
        assets::STYLE_CSS.contains("mdv-content"),
        "CSS missing content class"
    );
}

#[test]
fn all_twenty_katex_fonts_embedded() {
    assert_eq!(
        assets::KATEX_FONTS.len(),
        20,
        "expected 20 KaTeX fonts, got {}",
        assets::KATEX_FONTS.len()
    );
    for (name, bytes) in assets::KATEX_FONTS {
        assert!(!bytes.is_empty(), "KaTeX font {} is empty", name);
        assert!(name.ends_with(".woff2"), "KaTeX font {} not woff2", name);
    }
}
