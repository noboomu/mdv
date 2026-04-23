# mdv sample document

Testing the rendering pipeline end-to-end. This file exercises headings, lists,
code, tables, blockquotes, math, and links.

## Inline features

Paragraphs with **bold**, *italic*, `inline code`, and ~~strikethrough~~.
Inline math sits among text: the fine-structure constant is
$\alpha \approx 1/137.036$, and Euler's identity
$e^{i\pi} + 1 = 0$ still feels like a magic trick.

Display math with double-dollar:

$$
\mathcal{L} = -\tfrac{1}{4} F_{\mu\nu} F^{\mu\nu}
            + i \bar{\psi} \gamma^\mu D_\mu \psi
            + |D_\mu \phi|^2 - V(\phi)
$$

LaTeX-style bracket delimiters too:

\[
\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}
\]

And escaped-paren inline: \( \nabla \cdot \mathbf{E} = \rho / \varepsilon_0 \).

## Lists

- regular bullet
- another bullet
  - nested item
  - another nested
- last bullet

1. numbered
2. numbered
3. and so on

Task list:

- [x] wire KaTeX
- [x] vendor fonts
- [ ] file-watcher reload
- [ ] review mode UX pass

## Code

Rust block with highlighting:

```rust
fn classify(path: &Path) -> std::io::Result<Classification> {
    let meta = fs::metadata(path)?;
    let size_bytes = meta.len();
    // ...
    Ok(Classification { kind: DocKind::Markdown, size_bytes })
}
```

Python block:

```python
def render(md: str) -> str:
    return md2html(md, extras=["fenced-code-blocks", "tables"])
```

Shell block:

```bash
cargo build --release && cp target/release/mdv ~/.local/bin/
```

## Table

| metric       | value      | unit  |
|--------------|-----------:|-------|
| $v_c$        |     20.780 | TeV   |
| $m_0$        |     41.640 | TeV   |
| $M_4$        |     25.300 | TeV   |
| $\alpha$     |      0.007 |  -    |

## Blockquote

> "The purpose of computing is insight, not numbers."
> - Hamming

## Links

- External: [example site](https://example.com/)
- Relative md: [see also](./related.md)
- Anchor: [#lists](#lists)

## Horizontal rule

---

End of fixture.
