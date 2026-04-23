//! mdv - portable markdown viewer.
//!
//! See `specs/mdv-architecture.md` for the contract. This file wires the CLI
//! to `tao` windows and `wry` webviews, and routes IPC messages.

mod assets;
mod render;

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use clap::Parser as ClapParser;
use serde::Deserialize;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use uuid::Uuid;
use wry::{
    http::{header::CONTENT_TYPE, Response},
    WebView, WebViewBuilder,
};

/// Hard cap on windows (per read.me contract).
const MAX_WINDOWS: usize = 8;

// --------------- CLI ---------------

#[derive(ClapParser, Debug)]
#[command(name = "mdv", version, about = "Portable markdown viewer")]
struct Cli {
    /// A single markdown (or text) file to render.
    file: Option<PathBuf>,

    /// Open each file in a separate window (capped at 8).
    #[arg(short = 'b', long = "batch", num_args = 1..)]
    batch: Vec<PathBuf>,

    /// Open every *.md in a directory (capped at 8).
    #[arg(short = 'd', long = "directory")]
    directory: Option<PathBuf>,

    /// Review mode: render FILE and append verdict to OUTPUT_MD.
    #[arg(short = 'r', long = "review", value_name = "OUTPUT_MD")]
    review: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct WindowConfig {
    path: PathBuf,
    review_output: Option<PathBuf>,
}

fn main() -> Result<()> {
    configure_linux_webkit_env();
    let cli = Cli::parse();
    let _session = Uuid::new_v4();
    eprintln!("mdv {} pid={}", _session, std::process::id());

    let configs = resolve_inputs(&cli)?;
    if configs.is_empty() {
        eprintln!("error: no readable inputs");
        std::process::exit(3);
    }

    run(configs)
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinuxWebkitEnvPlan {
    set_gdk_backend_x11: bool,
    set_disable_compositing: bool,
    warn_missing_display: bool,
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn linux_webkit_env_plan(
    on_wayland: bool,
    has_display: bool,
    has_gdk_backend: bool,
    has_disable_compositing: bool,
) -> Option<LinuxWebkitEnvPlan> {
    if !on_wayland {
        return None;
    }

    if !has_display {
        return Some(LinuxWebkitEnvPlan {
            set_gdk_backend_x11: false,
            set_disable_compositing: false,
            warn_missing_display: true,
        });
    }

    Some(LinuxWebkitEnvPlan {
        set_gdk_backend_x11: !has_gdk_backend,
        set_disable_compositing: !has_disable_compositing,
        warn_missing_display: false,
    })
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn configure_linux_webkit_env() {
    let plan = linux_webkit_env_plan(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
        std::env::var_os("GDK_BACKEND").is_some(),
        std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_some(),
    );

    let Some(plan) = plan else {
        return;
    };

    if plan.warn_missing_display {
        eprintln!(
            "warn: WAYLAND_DISPLAY is set but DISPLAY is missing; \
             mdv will not guess an X11 display automatically"
        );
        return;
    }

    // WebKitGTK on Linux is more reliable under XWayland for this app.
    if plan.set_gdk_backend_x11 {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    if plan.set_disable_compositing {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn configure_linux_webkit_env() {}

fn resolve_inputs(cli: &Cli) -> Result<Vec<WindowConfig>> {
    // Review mode: exactly one file, no batch/dir.
    if let Some(out) = &cli.review {
        if !cli.batch.is_empty() || cli.directory.is_some() {
            eprintln!("error: -r is mutually exclusive with -b and -d");
            std::process::exit(2);
        }
        let Some(file) = cli.file.clone() else {
            eprintln!("error: -r requires exactly one input FILE");
            std::process::exit(2);
        };
        if !file.exists() {
            bail!("input not found: {}", file.display());
        }
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).ok();
            }
        }
        return Ok(vec![WindowConfig {
            path: canonicalize_best(&file),
            review_output: Some(out.clone()),
        }]);
    }

    // -b wins if both -b and -d are present.
    if !cli.batch.is_empty() {
        if cli.directory.is_some() {
            eprintln!("warn: -d ignored (superseded by -b)");
        }
        return Ok(cap_and_warn(
            cli.batch
                .iter()
                .filter(|p| p.exists())
                .map(|p| WindowConfig {
                    path: canonicalize_best(p),
                    review_output: None,
                })
                .collect(),
        ));
    }

    if let Some(dir) = &cli.directory {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .with_context(|| format!("reading directory {}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("md"))
                        .unwrap_or(false)
            })
            .collect();
        files.sort();
        return Ok(cap_and_warn(
            files
                .into_iter()
                .map(|p| WindowConfig {
                    path: canonicalize_best(&p),
                    review_output: None,
                })
                .collect(),
        ));
    }

    if let Some(file) = &cli.file {
        if !file.exists() {
            bail!("input not found: {}", file.display());
        }
        return Ok(vec![WindowConfig {
            path: canonicalize_best(file),
            review_output: None,
        }]);
    }

    Ok(Vec::new())
}

fn cap_and_warn(mut v: Vec<WindowConfig>) -> Vec<WindowConfig> {
    if v.len() > MAX_WINDOWS {
        eprintln!(
            "warn: too many documents ({} given, {} rendered)",
            v.len(),
            MAX_WINDOWS
        );
        v.truncate(MAX_WINDOWS);
    }
    v
}

fn canonicalize_best(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn linux_webkit_env_plan_is_none_off_wayland() {
        assert_eq!(linux_webkit_env_plan(false, true, false, false), None);
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn linux_webkit_env_plan_sets_missing_vars_on_wayland() {
        assert_eq!(
            linux_webkit_env_plan(true, true, false, false),
            Some(LinuxWebkitEnvPlan {
                set_gdk_backend_x11: true,
                set_disable_compositing: true,
                warn_missing_display: false,
            })
        );
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn linux_webkit_env_plan_respects_existing_overrides() {
        assert_eq!(
            linux_webkit_env_plan(true, true, true, true),
            Some(LinuxWebkitEnvPlan {
                set_gdk_backend_x11: false,
                set_disable_compositing: false,
                warn_missing_display: false,
            })
        );
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn linux_webkit_env_plan_warns_when_wayland_has_no_x_display() {
        assert_eq!(
            linux_webkit_env_plan(true, false, false, false),
            Some(LinuxWebkitEnvPlan {
                set_gdk_backend_x11: false,
                set_disable_compositing: false,
                warn_missing_display: true,
            })
        );
    }
}

// --------------- Event / State types ---------------

/// Events the IPC handler sends back into the event loop.
/// All variants are `Send` so `EventLoopProxy<UserEvent>` is legal.
#[derive(Debug)]
#[allow(dead_code)]
enum UserEvent {
    /// Close a window by id (raised when the webview wants self-close).
    CloseWindow(tao::window::WindowId),
    /// Navigate to a local .md path in the given window.
    Navigate {
        window_id: tao::window::WindowId,
        path: String,
    },
    /// Go back or forward in the given window's history.
    GoHistory {
        window_id: tao::window::WindowId,
        dir: String,
    },
    /// Evaluate an arbitrary JS snippet in the given window's webview.
    EvalScript {
        window_id: tao::window::WindowId,
        script: String,
    },
}

struct WindowState {
    current: PathBuf,
    back: Vec<PathBuf>,
    fwd: Vec<PathBuf>,
    review_output: Option<PathBuf>,
    /// True once the user has submitted a review verdict.
    review_submitted: bool,
    /// All source lines for the current document - populated only for large
    /// files being streamed; empty otherwise.
    stream_all_lines: Vec<String>,
    /// Index of the next line in `stream_all_lines` to emit.
    stream_cursor: usize,
}

struct ManagedWindow {
    window: Window,
    webview: WebView,
    state: Rc<RefCell<WindowState>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum IpcMsg {
    Nav {
        target: String,
    },
    History {
        dir: String,
    },
    Review {
        verdict: String,
        text: String,
    },
    Ready,
    #[serde(rename = "chunk_ack")]
    ChunkAck {
        #[allow(dead_code)]
        next: usize,
    },
}

// --------------- Event loop / window builder ---------------

fn run(configs: Vec<WindowConfig>) -> Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let mut managed: std::collections::HashMap<tao::window::WindowId, ManagedWindow> =
        std::collections::HashMap::new();

    for cfg in configs {
        let proxy = event_loop.create_proxy();
        let mw = build_window(&event_loop, &cfg, proxy)?;
        managed.insert(mw.window.id(), mw);
    }

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            // --- Window close ---
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                managed.remove(&window_id);
                if managed.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }

            // --- User events from IPC ---
            Event::UserEvent(UserEvent::CloseWindow(wid)) => {
                managed.remove(&wid);
                if managed.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(UserEvent::Navigate { window_id, path }) => {
                if let Some(mw) = managed.get_mut(&window_id) {
                    let dest = PathBuf::from(&path);
                    let dest = if dest.is_absolute() {
                        dest
                    } else {
                        // Resolve relative to the current document's directory.
                        let cur_dir = mw
                            .state
                            .borrow()
                            .current
                            .parent()
                            .unwrap_or(Path::new("."))
                            .to_path_buf();
                        canonicalize_best(&cur_dir.join(&dest))
                    };

                    match navigate_to_path(&mut mw.state.borrow_mut(), dest) {
                        Some(html) => {
                            if let Err(e) = mw.webview.load_html(&html) {
                                eprintln!("load_html error: {}", e);
                            }
                        }
                        None => eprintln!("nav: failed to render {}", path),
                    }
                }
            }

            Event::UserEvent(UserEvent::GoHistory { window_id, dir }) => {
                if let Some(mw) = managed.get_mut(&window_id) {
                    match navigate_history(&mut mw.state.borrow_mut(), &dir) {
                        Some(html) => {
                            if let Err(e) = mw.webview.load_html(&html) {
                                eprintln!("load_html error: {}", e);
                            }
                        }
                        None => eprintln!("history {}: nothing to pop", dir),
                    }
                }
            }

            Event::UserEvent(UserEvent::EvalScript { window_id, script }) => {
                if let Some(mw) = managed.get(&window_id) {
                    if let Err(e) = mw.webview.evaluate_script(&script) {
                        eprintln!("evaluate_script error: {}", e);
                    }
                }
            }

            _ => {}
        }
    });
}

fn build_window(
    event_loop: &EventLoop<UserEvent>,
    cfg: &WindowConfig,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<ManagedWindow> {
    let title = format!(
        "mdv - {}",
        cfg.path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
    );
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 760.0))
        .build(event_loop)
        .context("creating window")?;

    let (page, state) = render_initial(cfg)?;
    let state = Rc::new(RefCell::new(state));

    let window_id = window.id();
    let state_ipc = Rc::clone(&state);

    let builder = WebViewBuilder::new()
        .with_html(page)
        .with_ipc_handler(move |req| {
            let body = req.into_body();
            handle_ipc(body, &state_ipc, window_id, &proxy);
        })
        .with_custom_protocol("mdv-asset".into(), |_wv_id, request| {
            let path = request.uri().path().to_string();
            match assets::resolve(&path) {
                Some((mime, bytes)) => {
                    let body: Vec<u8> = bytes.into_owned();
                    Response::builder()
                        .status(200)
                        .header(CONTENT_TYPE, mime)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(body.into())
                        .unwrap_or_default()
                }
                None => Response::builder()
                    .status(404)
                    .body(Vec::<u8>::new().into())
                    .unwrap_or_default(),
            }
        });

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().context("no gtk vbox on window")?;
        builder.build_gtk(vbox)?
    };

    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    let webview = builder.build(&window)?;

    Ok(ManagedWindow {
        window,
        webview,
        state,
    })
}

// --------------- Initial render ---------------

fn render_initial(cfg: &WindowConfig) -> Result<(String, WindowState)> {
    let path = &cfg.path;
    let cls = render::classify(path).with_context(|| format!("classifying {}", path.display()))?;
    let result =
        render::render_file(path, &cls).with_context(|| format!("rendering {}", path.display()))?;
    let review_mode = cfg.review_output.is_some();
    let html = render::build_page(&result, review_mode);

    // Prepare streaming state if the file is large.
    let (stream_all_lines, stream_cursor) = if result.streaming.is_some() {
        let all: Vec<String> = fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect();
        let cursor = render::STREAM_INITIAL_LINES.min(all.len());
        (all, cursor)
    } else {
        (Vec::new(), 0)
    };

    let state = WindowState {
        current: path.clone(),
        back: Vec::new(),
        fwd: Vec::new(),
        review_output: cfg.review_output.clone(),
        review_submitted: false,
        stream_all_lines,
        stream_cursor,
    };
    Ok((html, state))
}

// --------------- Navigation helpers ---------------

/// Navigate to a new path. Updates `state` in place and returns the new page
/// HTML, or `None` if the file cannot be rendered.
fn navigate_to_path(state: &mut WindowState, dest: PathBuf) -> Option<String> {
    if !dest.exists() {
        eprintln!("nav: not found: {}", dest.display());
        return None;
    }
    let cls = render::classify(&dest).ok()?;
    let result = render::render_file(&dest, &cls).ok()?;
    let review_mode = state.review_output.is_some();
    let html = render::build_page(&result, review_mode);

    // Set up streaming for the new document if needed.
    if result.streaming.is_some() {
        let all: Vec<String> = fs::read_to_string(&dest)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect();
        let cursor = render::STREAM_INITIAL_LINES.min(all.len());
        state.stream_all_lines = all;
        state.stream_cursor = cursor;
    } else {
        state.stream_all_lines.clear();
        state.stream_cursor = 0;
    }

    // Update navigation stacks.
    let prev = std::mem::replace(&mut state.current, dest);
    state.back.push(prev);
    state.fwd.clear();

    Some(html)
}

/// Go back or forward in history. Updates `state` and returns new page HTML.
fn navigate_history(state: &mut WindowState, dir: &str) -> Option<String> {
    let dest = if dir == "back" {
        if state.back.is_empty() {
            return None;
        }
        let dest = state.back.pop().unwrap();
        let old = std::mem::replace(&mut state.current, dest.clone());
        state.fwd.push(old);
        dest
    } else {
        // "fwd"
        if state.fwd.is_empty() {
            return None;
        }
        let dest = state.fwd.pop().unwrap();
        let old = std::mem::replace(&mut state.current, dest.clone());
        state.back.push(old);
        dest
    };

    let cls = render::classify(&dest).ok()?;
    let result = render::render_file(&dest, &cls).ok()?;
    let review_mode = state.review_output.is_some();
    let html = render::build_page(&result, review_mode);

    if result.streaming.is_some() {
        let all: Vec<String> = fs::read_to_string(&dest)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect();
        let cursor = render::STREAM_INITIAL_LINES.min(all.len());
        state.stream_all_lines = all;
        state.stream_cursor = cursor;
    } else {
        state.stream_all_lines.clear();
        state.stream_cursor = 0;
    }

    Some(html)
}

// --------------- IPC handler ---------------

fn handle_ipc(
    body: String,
    state: &Rc<RefCell<WindowState>>,
    window_id: tao::window::WindowId,
    proxy: &EventLoopProxy<UserEvent>,
) {
    let msg: IpcMsg = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "ipc: bad json: {} (raw: {:?})",
                e,
                &body[..body.len().min(120)]
            );
            return;
        }
    };

    match msg {
        // ---- Navigation ----
        IpcMsg::Nav { target } => {
            let lower = target.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                // Open external URLs in the system browser.
                if let Err(e) = open::that(&target) {
                    eprintln!("open external url failed: {}", e);
                }
                return;
            }
            // Local .md file: route to Navigate event.
            let _ = proxy.send_event(UserEvent::Navigate {
                window_id,
                path: target,
            });
        }

        // ---- History ----
        IpcMsg::History { dir } => {
            let _ = proxy.send_event(UserEvent::GoHistory { window_id, dir });
        }

        // ---- Review submission ----
        IpcMsg::Review { verdict, text } => {
            let mut s = state.borrow_mut();
            if s.review_submitted {
                eprintln!("review: duplicate submission ignored");
                return;
            }
            let Some(out_path) = s.review_output.clone() else {
                eprintln!("review submit ignored: not in review mode");
                return;
            };
            match append_review(&out_path, &s.current, &verdict, &text) {
                Ok(()) => {
                    s.review_submitted = true;
                    eprintln!(
                        "review: appended [{}] to {}",
                        verdict.to_ascii_uppercase(),
                        out_path.display()
                    );
                }
                Err(e) => eprintln!("review write failed: {}", e),
            }
        }

        // ---- Page ready (fires on every DOMContentLoaded) ----
        IpcMsg::Ready => {
            let s = state.borrow();
            // Build a single script that sets history buttons + review.
            let mut parts: Vec<String> = Vec::new();

            let has_back = !s.back.is_empty();
            let has_fwd = !s.fwd.is_empty();
            parts.push(format!(
                "window.mdv && window.mdv.setHistory({}, {})",
                has_back, has_fwd
            ));

            if s.review_output.is_some() && !s.review_submitted {
                parts.push("window.mdv && window.mdv.enableReview()".into());
            }

            let script = parts.join(";");
            let _ = proxy.send_event(UserEvent::EvalScript { window_id, script });

            // If streaming is active, kick the first chunk.
            let needs_chunk = s.stream_cursor < s.stream_all_lines.len();
            drop(s);
            if needs_chunk {
                send_next_chunk(state, window_id, proxy);
            }
        }

        // ---- Streaming chunk acknowledge ----
        IpcMsg::ChunkAck { .. } => {
            let more = {
                let s = state.borrow();
                s.stream_cursor < s.stream_all_lines.len()
            };
            if more {
                send_next_chunk(state, window_id, proxy);
            } else {
                // All chunks delivered - fade out the streaming banner.
                let _ = proxy.send_event(UserEvent::EvalScript {
                    window_id,
                    script: "window.mdv && window.mdv.streamDone()".into(),
                });
            }
        }
    }
}

/// Build and send the next streaming chunk as an EvalScript event.
fn send_next_chunk(
    state: &Rc<RefCell<WindowState>>,
    window_id: tao::window::WindowId,
    proxy: &EventLoopProxy<UserEvent>,
) {
    let (html, consumed) = {
        let s = state.borrow();
        if s.stream_cursor >= s.stream_all_lines.len() {
            return;
        }
        render::render_chunk(&s.stream_all_lines, s.stream_cursor, &s.current)
    };

    state.borrow_mut().stream_cursor += consumed;

    // Encode HTML as a JSON string literal so it survives the JS eval safely.
    let json_html = serde_json::to_string(&html).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!("window.mdv && window.mdv.appendChunk({})", json_html);
    let _ = proxy.send_event(UserEvent::EvalScript { window_id, script });
}

// --------------- Review output ---------------

pub fn append_review(out: &Path, source: &Path, verdict: &str, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let verdict_up = verdict.to_ascii_uppercase();
    let name = source.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    // Escape backslashes and double-quotes so the block is valid markdown.
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let block = format!(
        "=======\n{} user feedback\n=======\n[{}] \"{}\"\n\n",
        name, verdict_up, escaped,
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)?;
    f.write_all(block.as_bytes())?;
    Ok(())
}
