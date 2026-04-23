//! Embedded assets. All bundled at compile time so the binary is relocatable.
//!
//! Served to the webview through the custom `mdv-asset://` scheme so fonts and
//! scripts load from memory without touching the filesystem.

use std::borrow::Cow;

pub const HTML_TEMPLATE: &str = include_str!("../assets/template.html");
pub const STYLE_CSS: &str = include_str!("../assets/style.css");

pub const KATEX_CSS: &[u8] = include_bytes!("../assets/katex/katex.min.css");
pub const KATEX_JS: &[u8] = include_bytes!("../assets/katex/katex.min.js");
pub const KATEX_AUTO_RENDER_JS: &[u8] = include_bytes!("../assets/katex/auto-render.min.js");

pub const INTER_VARIABLE: &[u8] = include_bytes!("../assets/fonts/InterVariable.woff2");
pub const INTER_VARIABLE_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/InterVariable-Italic.woff2");

/// KaTeX font files, resolved at runtime by path suffix.
/// (Having a match-table keeps us honest about what's actually embedded.)
macro_rules! katex_fonts {
    ($($name:literal),* $(,)?) => {
        pub const KATEX_FONTS: &[(&str, &[u8])] = &[
            $(
                ($name, include_bytes!(concat!("../assets/katex/fonts/", $name))),
            )*
        ];
    };
}

katex_fonts! {
    "KaTeX_AMS-Regular.woff2",
    "KaTeX_Caligraphic-Bold.woff2",
    "KaTeX_Caligraphic-Regular.woff2",
    "KaTeX_Fraktur-Bold.woff2",
    "KaTeX_Fraktur-Regular.woff2",
    "KaTeX_Main-BoldItalic.woff2",
    "KaTeX_Main-Bold.woff2",
    "KaTeX_Main-Italic.woff2",
    "KaTeX_Main-Regular.woff2",
    "KaTeX_Math-BoldItalic.woff2",
    "KaTeX_Math-Italic.woff2",
    "KaTeX_SansSerif-Bold.woff2",
    "KaTeX_SansSerif-Italic.woff2",
    "KaTeX_SansSerif-Regular.woff2",
    "KaTeX_Script-Regular.woff2",
    "KaTeX_Size1-Regular.woff2",
    "KaTeX_Size2-Regular.woff2",
    "KaTeX_Size3-Regular.woff2",
    "KaTeX_Size4-Regular.woff2",
    "KaTeX_Typewriter-Regular.woff2",
}

/// Resolve an asset path to (mime, bytes). Returns None if unknown.
pub fn resolve(path: &str) -> Option<(&'static str, Cow<'static, [u8]>)> {
    // Normalize: strip leading slashes.
    let p = path.trim_start_matches('/');

    match p {
        "katex/katex.min.css" => Some(("text/css; charset=utf-8", Cow::Borrowed(KATEX_CSS))),
        "katex/katex.min.js" => Some((
            "application/javascript; charset=utf-8",
            Cow::Borrowed(KATEX_JS),
        )),
        "katex/auto-render.min.js" => Some((
            "application/javascript; charset=utf-8",
            Cow::Borrowed(KATEX_AUTO_RENDER_JS),
        )),
        "fonts/InterVariable.woff2" => Some(("font/woff2", Cow::Borrowed(INTER_VARIABLE))),
        "fonts/InterVariable-Italic.woff2" => {
            Some(("font/woff2", Cow::Borrowed(INTER_VARIABLE_ITALIC)))
        }
        _ => {
            // KaTeX fonts live under katex/fonts/<file>
            if let Some(name) = p.strip_prefix("katex/fonts/") {
                for (n, bytes) in KATEX_FONTS {
                    if *n == name {
                        return Some(("font/woff2", Cow::Borrowed(*bytes)));
                    }
                }
            }
            None
        }
    }
}
