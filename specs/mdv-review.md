# mdv - review mode

**Invocation**
```
mdv -r FEEDBACK.md DOC.md
```

**UI**
- Fixed panel docked to the bottom of the window, background = elevated surface
  color (distinct from content), top border = 1px accent line.
- Content area gets bottom-padding equal to panel height so no scroll position
  hides markdown behind the panel.
- Panel contents:
  - `<textarea>` with `rows="2"` minimum, auto-grows up to 8 rows, `word-wrap: break-word`,
    tabular numerals disabled (prose input), monospaced not required.
  - Two buttons side-by-side: `ACCEPT` (primary accent) and `REJECT` (muted).
  - Buttons disabled until textarea has non-whitespace content.

**Submission**
- On click, IPC `{kind: "review", verdict: "accept"|"reject", text: "..."}`.
- Rust appends to `<output>.md` (create if missing):
  ```
  =======
  <input-basename> user feedback
  =======
  [ACCEPT] "<text>"

  ```
  (Two trailing newlines so consecutive entries separate cleanly.)
- On success the panel animates to a "feedback submitted" pill with checkmark
  glyph; inputs removed from DOM.
- On failure (disk error), pill turns red with error message, panel stays open
  so user can retry.

**Constraints**
- Only one review submission per window session (UI does not reopen after
  submit within same invocation).
- Verdict token is always uppercase in the output file.
