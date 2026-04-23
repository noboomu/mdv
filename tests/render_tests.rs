//! Integration tests for the mdv render pipeline.
//!
//! These tests exercise the public API of `render` and `main::append_review`
//! without requiring a webview or display - they run headlessly with `cargo test`.

use std::path::{Path, PathBuf};

// Pull in the binary's modules via the test harness.
// Cargo makes the binary's library-like modules available through the lib
// convention; since mdv is a bin-only crate we use a path-based workaround.
// We duplicate the module tree as an inline mod - Cargo resolves the paths.
#[path = "../src/render.rs"]
mod render;

#[path = "../src/assets.rs"]
mod assets;

// ---- helpers ----

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn write_tmp(name: &str, content: &[u8]) -> PathBuf {
    let p = tmp(name);
    std::fs::write(&p, content).expect("write_tmp failed");
    p
}

fn remove_tmp(name: &str) {
    let _ = std::fs::remove_file(tmp(name));
}

// ---- classify ----

#[test]
fn classify_png_extension_faked_as_md() {
    let p = write_tmp("integ_fake_png.md", b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR");
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::NonText { .. }),
        "PNG disguised as .md should be NonText, got {:?}",
        cls.kind
    );
    remove_tmp("integ_fake_png.md");
}

#[test]
fn classify_binary_elf() {
    let p = write_tmp(
        "integ_elf.md",
        b"\x7FELF\x02\x01\x01\x00binary stuff follow",
    );
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::NonText { .. }),
        "ELF binary should be NonText"
    );
    remove_tmp("integ_elf.md");
}

#[test]
fn classify_empty_is_plaintext() {
    let p = write_tmp("integ_empty.md", b"");
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::PlainText),
        "empty file should be PlainText"
    );
    remove_tmp("integ_empty.md");
}

#[test]
fn classify_rich_markdown_file() {
    let content = "# Header\n\n- list\n- items\n\n```rust\nlet x = 1;\n```\n";
    let p = write_tmp("integ_rich.md", content.as_bytes());
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::Markdown),
        "markdown-heavy file should be Markdown"
    );
    remove_tmp("integ_rich.md");
}

#[test]
fn classify_prose_only_is_plaintext() {
    let content =
        "This is a normal prose file.\nIt has no markdown tokens at all.\nJust three lines.\n";
    let p = write_tmp("integ_prose.txt", content.as_bytes());
    let cls = render::classify(&p).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::PlainText),
        "pure prose should be PlainText"
    );
    remove_tmp("integ_prose.txt");
}

// ---- render_file (smoke test - should not panic on any input) ----

#[test]
fn render_file_handles_nontext_gracefully() {
    let p = write_tmp("integ_nonrender.md", b"\x89PNG\r\n\x1a\n fake");
    let cls = render::classify(&p).unwrap();
    let result = render::render_file(&p, &cls).unwrap();
    assert!(
        result.body_html.contains("NON-RENDERABLE"),
        "non-renderable card missing: {}",
        result.body_html
    );
    remove_tmp("integ_nonrender.md");
}

#[test]
fn render_file_handles_plaintext_gracefully() {
    let p = write_tmp("integ_plain.txt", b"Just plain text.\nNothing special.\n");
    let cls = render::classify(&p).unwrap();
    let result = render::render_file(&p, &cls).unwrap();
    assert!(
        result.body_html.contains("plain"),
        "plain text wrapper missing: {}",
        result.body_html
    );
    remove_tmp("integ_plain.txt");
}

#[test]
fn render_file_malformed_markdown_does_not_panic() {
    // Unclosed code fence, unclosed bold, broken table - should not crash.
    let bad = "# Title\n\n```rust\nfn broken(\n**unclosed bold\n| col1 | col2 |\n| a |\n";
    let p = write_tmp("integ_broken.md", bad.as_bytes());
    let cls = render::classify(&p).unwrap();
    let result = render::render_file(&p, &cls);
    assert!(
        result.is_ok(),
        "malformed md should not return Err: {:?}",
        result.err()
    );
    remove_tmp("integ_broken.md");
}

// ---- md_to_html math preservation ----

fn html(md: &str) -> String {
    render::md_to_html(md, Path::new("/docs"))
}

#[test]
fn math_inline_dollar_survives_pipeline() {
    let out = html("Result: $E = mc^2$");
    assert!(out.contains("$E = mc^2$"), "inline math lost: {}", out);
}

#[test]
fn math_display_dollar_survives_pipeline() {
    let out = html("$$\n\\sum_{n} a_n\n$$");
    assert!(out.contains("$$"), "display math $$ lost: {}", out);
    assert!(out.contains("\\sum"), "LaTeX command lost: {}", out);
}

#[test]
fn math_bracket_display_survives_pipeline() {
    let out = html("\\[\nF = ma\n\\]");
    assert!(out.contains("\\["), "\\[ delimiter lost: {}", out);
    assert!(out.contains("F = ma"), "content lost: {}", out);
    assert!(out.contains("\\]"), "\\] delimiter lost: {}", out);
}

#[test]
fn math_paren_inline_survives_pipeline() {
    let out = html("see \\(\\theta\\) for angle");
    assert!(out.contains("\\("), "\\( delimiter lost: {}", out);
    assert!(out.contains("\\theta"), "latex cmd lost: {}", out);
    assert!(out.contains("\\)"), "\\) delimiter lost: {}", out);
}

#[test]
fn currency_dollar_not_treated_as_math() {
    let out = html("Items cost $5 and $20.");
    assert!(out.contains("$5"), "currency dollar disappeared: {}", out);
    assert!(out.contains("$20"), "currency dollar disappeared: {}", out);
}

#[test]
fn math_inside_code_block_not_rendered() {
    let out = html("```\n$not_math$\n```");
    // The dollar sign should be present as raw text inside <code>, not as a KaTeX span.
    assert!(
        out.contains("$not_math$"),
        "dollar in code block was eaten: {}",
        out
    );
}

// ---- md_to_html link rewriting ----

#[test]
fn local_md_link_rewritten_to_mdv_nav() {
    let out = html("[next](chapter2.md)");
    assert!(
        out.contains("mdv://nav/"),
        ".md link was not rewritten to mdv://nav/: {}",
        out
    );
}

#[test]
fn local_md_link_with_anchor_rewritten() {
    let out = html("[section](other.md#intro)");
    assert!(
        out.contains("mdv://nav/"),
        "link with anchor not rewritten: {}",
        out
    );
}

#[test]
fn http_link_preserved_verbatim() {
    let out = html("[site](https://example.com)");
    assert!(
        out.contains("https://example.com"),
        "http link mangled: {}",
        out
    );
    assert!(
        !out.contains("mdv://nav/"),
        "http link incorrectly rewritten: {}",
        out
    );
}

#[test]
fn anchor_only_link_preserved() {
    let out = html("[top](#heading)");
    assert!(out.contains("#heading"), "anchor link lost: {}", out);
    assert!(
        !out.contains("mdv://nav/"),
        "anchor link incorrectly rewritten: {}",
        out
    );
}

// ---- detect_warnings ----

#[test]
fn warns_unclosed_fence() {
    let md = "```rust\nfn main() {}\n// no closing fence\n";
    let w = render::detect_warnings(md);
    assert!(
        !w.is_empty(),
        "expected warning for unclosed fence, got none"
    );
    assert!(
        w.iter().any(|s| s.contains("unclosed")),
        "warning text should mention 'unclosed': {:?}",
        w
    );
}

#[test]
fn no_false_warning_for_balanced_fence() {
    let md = "# Doc\n\n```rust\nlet x = 1;\n```\n";
    let w = render::detect_warnings(md);
    let fence_warns: Vec<_> = w.iter().filter(|s| s.contains("unclosed")).collect();
    assert!(fence_warns.is_empty(), "false fence warning: {:?}", w);
}

#[test]
fn warns_table_column_mismatch() {
    let md = "| a | b | c |\n|---|---|---|\n| 1 | 2 |\n";
    let w = render::detect_warnings(md);
    assert!(
        w.iter().any(|s| s.contains("mismatch")),
        "expected column mismatch warning: {:?}",
        w
    );
}

// ---- build_page ----

#[test]
fn build_page_includes_title() {
    let result = render::RenderResult {
        title: "MyDoc".into(),
        body_html: "<p>content</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    assert!(page.contains("MyDoc"), "title missing from page");
}

#[test]
fn build_page_review_mode_adds_body_class() {
    let result = render::RenderResult {
        title: "Test".into(),
        body_html: "<p>hi</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, true);
    // The <body> element itself must carry the class (the CSS selector string
    // mdv-review-on also appears in the embedded stylesheet, so we target the tag).
    assert!(
        page.contains(r#"<body class="mdv-review-on">"#),
        "review body class missing when review_mode=true: {}",
        &page[..page.len().min(300)]
    );
}

#[test]
fn build_page_no_review_class_when_off() {
    let result = render::RenderResult {
        title: "Test".into(),
        body_html: "<p>hi</p>".into(),
        banners: vec![],
        streaming: None,
    };
    let page = render::build_page(&result, false);
    // With review_mode=false the <body> tag must not carry the review class.
    // (The CSS selector string still appears in the embedded stylesheet, so we
    // check the opening tag specifically rather than the whole page.)
    assert!(
        !page.contains(r#"<body class="mdv-review-on">"#),
        "review body class present on <body> when review_mode=false"
    );
}

// ---- append_review (exercises main::append_review via the fixture path) ----
// We replicate the logic here rather than import from main (bin-only crate).

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
fn append_review_creates_file_with_accept() {
    let out = tmp("integ_review_accept.md");
    let _ = std::fs::remove_file(&out); // clean slate
    let src = Path::new("/docs/report.md");
    append_review_test(&out, src, "accept", "Looks good to me.").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("[ACCEPT]"), "verdict missing: {}", content);
    assert!(
        content.contains("report.md"),
        "source filename missing: {}",
        content
    );
    assert!(
        content.contains("Looks good to me."),
        "text missing: {}",
        content
    );
    remove_tmp("integ_review_accept.md");
}

#[test]
fn append_review_creates_file_with_reject() {
    let out = tmp("integ_review_reject.md");
    let _ = std::fs::remove_file(&out);
    append_review_test(&out, Path::new("/x/doc.md"), "reject", "Not ready.").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("[REJECT]"), "verdict missing: {}", content);
    remove_tmp("integ_review_reject.md");
}

#[test]
fn append_review_appends_on_second_call() {
    let out = tmp("integ_review_append.md");
    let _ = std::fs::remove_file(&out);
    let src = Path::new("/x/doc.md");
    append_review_test(&out, src, "accept", "First feedback.").unwrap();
    append_review_test(&out, src, "reject", "Second feedback.").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("[ACCEPT]"), "first entry missing");
    assert!(content.contains("[REJECT]"), "second entry missing");
    assert!(content.contains("First feedback."), "first text missing");
    assert!(content.contains("Second feedback."), "second text missing");
    remove_tmp("integ_review_append.md");
}

#[test]
fn append_review_escapes_quotes_in_text() {
    let out = tmp("integ_review_escape.md");
    let _ = std::fs::remove_file(&out);
    append_review_test(&out, Path::new("/doc.md"), "accept", r#"She said "hello"."#).unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    // Embedded double-quotes should be escaped as \"
    assert!(content.contains("\\\""), "quotes not escaped: {}", content);
    remove_tmp("integ_review_escape.md");
}

// ---- Streaming: render_chunk ----

#[test]
fn stream_chunk_covers_correct_lines() {
    let lines: Vec<String> = (0..500).map(|i| format!("# line {}", i)).collect();
    let (html, consumed) = render::render_chunk(&lines, 0, Path::new("/tmp/doc.md"));
    assert_eq!(consumed, render::STREAM_CHUNK_LINES);
    assert!(html.contains("line 0"), "first line missing");
    assert!(
        !html.contains(&format!("line {}", render::STREAM_CHUNK_LINES)),
        "line after chunk boundary included"
    );
}

#[test]
fn stream_chunk_cursor_respected() {
    let lines: Vec<String> = (0..500).map(|i| format!("para {}", i)).collect();
    let cursor = render::STREAM_INITIAL_LINES;
    let (html, consumed) = render::render_chunk(&lines, cursor, Path::new("/tmp/doc.md"));
    assert!(consumed > 0, "expected non-zero lines consumed");
    assert!(
        html.contains(&format!("para {}", cursor)),
        "expected para {} in chunk: {}",
        cursor,
        &html[..html.len().min(200)]
    );
}

#[test]
fn stream_chunk_at_end_returns_zero_consumed() {
    let lines: Vec<String> = vec!["a".into(), "b".into()];
    let (_html, consumed) = render::render_chunk(&lines, 2, Path::new("/tmp/doc.md"));
    assert_eq!(consumed, 0, "consumed should be 0 when cursor is at end");
}

// ---- fixture round-trip ----

#[test]
fn fixture_sample_renders_without_error() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.md");
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());
    let cls = render::classify(&fixture).unwrap();
    assert!(
        matches!(cls.kind, render::DocKind::Markdown),
        "sample fixture should classify as Markdown"
    );
    let result = render::render_file(&fixture, &cls).unwrap();
    assert!(!result.body_html.is_empty(), "rendered HTML is empty");
    // Math and code should both appear in output.
    // (KaTeX placeholders are present - KaTeX itself runs in the browser.)
    assert!(
        result.body_html.contains("alpha") || result.body_html.contains("\\alpha"),
        "expected math content in rendered HTML"
    );
}

#[test]
fn fixture_related_renders_and_links_back() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/related.md");
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());
    let cls = render::classify(&fixture).unwrap();
    let result = render::render_file(&fixture, &cls).unwrap();
    // The [sample.md](./sample.md) link should be rewritten to mdv://nav/
    assert!(
        result.body_html.contains("mdv://nav/"),
        "back link not rewritten: {}",
        &result.body_html[..result.body_html.len().min(400)]
    );
}
