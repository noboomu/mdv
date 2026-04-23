//! Dev helper: `cargo run --example dump_html -- INPUT.md [OUTPUT.html]`.
//!
//! Renders a file through the real mdv pipeline (classify + render + build_page)
//! and writes the resulting HTML to disk so we can visually sanity-check it in
//! a browser before wiring the full webview loop.
//!
//! This is NOT a runtime dependency - it's here to keep the dev cycle short
//! during bring-up.

use std::path::PathBuf;

// We only need the public API of our crate, but `mdv` is a bin crate. The
// simplest path is to re-include the modules from here.
#[path = "../src/assets.rs"]
mod assets;
#[path = "../src/render.rs"]
mod render;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: dump_html INPUT.md [OUTPUT.html]");
    let output = args.next().unwrap_or_else(|| "/tmp/mdv-dump.html".into());

    let input_path = PathBuf::from(&input);
    let cls = render::classify(&input_path)?;
    let result = render::render_file(&input_path, &cls)?;
    let page = render::build_page(&result, false);

    // Rewrite mdv-asset:// URLs to point at the local filesystem so an ordinary
    // browser can open the dumped file for review. This is dev-only: the real
    // app serves these via the custom protocol handler.
    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let page = page.replace(
        "mdv-asset://local/",
        &format!("file://{}/", assets_dir.display()),
    );

    std::fs::write(&output, page)?;
    eprintln!("wrote {}", output);
    eprintln!("classification: {:?} ({} bytes)", cls.kind, cls.size_bytes);
    Ok(())
}
