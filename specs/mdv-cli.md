# mdv - CLI

```
mdv [OPTIONS] [FILE]
mdv -b FILE [FILE ...]
mdv -d DIR
mdv -r OUTPUT_MD FILE
```

**Positional**
- `FILE` - single markdown (or text, or mis-extensioned) file.

**Options**
- `-b, --batch FILE...` - open each of the given files in a separate window.
  Caps at 8. If more are supplied the extras are dropped and a warning printed
  to stderr: `warn: too many documents (N given, 8 rendered)`.
- `-d, --directory DIR` - glob `DIR/*.md` (case insensitive), sort by name, apply
  the same 8-window cap and warning.
- `-r, --review OUTPUT_MD` - enable review mode. Requires exactly one positional
  `FILE`. Mutually exclusive with `-b` and `-d`.
- `-V, --version`, `-h, --help` - standard clap output.

**Exit codes**
- 0: clean exit (all windows closed).
- 2: invalid CLI combination (e.g. `-r` with `-b`).
- 3: no readable inputs.

**Session id**
- First line printed to stderr:
  `mdv <uuid> pid=<pid> args=<n>`

**Conflict resolution**
- `-b` and `-d` together: `-b` wins, `-d` ignored with a stderr warning.
- `-r` with zero or >1 positional: exit 2 with a concrete error.
