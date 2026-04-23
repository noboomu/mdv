# Changelog

## v0.1.0

Initial public release.

- Single-file, batch (`-b`), and directory (`-d`) rendering modes
- KaTeX math support for `$..$`, `$$..$$`, `\(..\)`, `\[..\]`
- Syntax highlighting via syntect with embedded theme
- Link navigation with back/forward history
- Large-file streaming (>512 KB)
- Review mode (`-r`) for verdict submission
- Industrial dark theme with embedded Inter typeface
- Wayland workaround applied automatically on supported sessions
- Single-binary distribution, all assets baked in

### Known Issues

- Linux only. macOS and Windows are not yet supported.
- Runtime dependency on system `webkit2gtk-4.1` (same class of dep as any GTK/Qt app).
- No live reload / file watching yet. Re-invoke the binary to see changes.
- Review mode is single-file only; `-r` is mutually exclusive with `-b` and `-d`.
- Unusual Wayland sessions without XWayland may require manual `DISPLAY` setup.
