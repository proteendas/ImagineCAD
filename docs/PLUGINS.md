# Scripts and plugins

ImagineCAD can be automated in two ways: **command scripts** replay what you
would type on the command line, and **plugins** add new commands written in any
programming language.

## Command scripts

A script (`.scr`) is a text file of commands and their input, exactly as typed:

```text
; A 120 x 60 plate with four corner holes and a title
RECTANG 0,0 120,60
CIRCLE 10,10 4
CIRCLE 110,10 4
DIMLINEAR 0,0 120,0 60,-15
TEXT 0,70 5 0
COVER PLATE
```

- A space and a line end both act as Enter, so `CIRCLE 10,10 4` is the same as
  `CIRCLE`, `10,10` and `4` on three lines.
- An empty line is Enter on its own, for example to end a selection or a line
  sequence. `<ENTER>` does the same inside a line.
- Where text is asked for (the contents of `TEXT`, an attribute value…), the
  whole line is taken, spaces included.
- Where objects are selected, type a point on an object to pick it, `L` for
  the last object drawn, or `ALL`; an empty line ends the selection.
- Lines starting with `;` are comments.

Run a script in the editor with `SCRIPT` (*Tools → Run Script…*), or without a
window:

```bash
imaginecad script plate.scr -o plate.dxf              # a new drawing
imaginecad script revise.scr part.dwg -o part-b.dwg   # changes to a drawing
```

Without a window, drawing, editing, dimensioning and annotation commands run
as usual; commands that open dialogs are skipped and reported. A new drawing
gets a sheet and scale that fit what the script drew. The command exits with
status 1 if the script named a command that does not exist.

## Plugins

A plugin is a folder with a `plugin.json` that names its commands. Install it
by copying the folder into the plugins folder (*Tools → Plugins… → Open plugins
folder*):

| System | Plugins folder |
|---|---|
| macOS | `~/Library/Application Support/ImagineCAD/plugins` |
| Windows | `%APPDATA%\ImagineCAD\plugins` |
| Linux | `~/.config/ImagineCAD/plugins` |

`IMAGINECAD_PLUGINS` may list more folders (separated like `PATH`). Plugin
commands are typed like built-in ones, appear in *Tools → Plugins…*, and work
in scripts and `imaginecad script`. After changing a plugin, press *Reload* or
type `PLUGINS RELOAD`.

**Only install plugins you trust.** A plugin command runs a program on your
computer with your permissions.

### plugin.json

```json
{
  "name": "Bolt circle",
  "version": "1.0",
  "description": "Holes equally spaced on a pitch circle.",
  "commands": [
    {
      "name": "BOLTCIRCLE",
      "description": "holes on a pitch circle",
      "run": ["python3", "boltcircle.py"],
      "inputs": [
        { "name": "pcd", "prompt": "Pitch circle diameter", "kind": "number", "default": 100 },
        { "name": "count", "prompt": "Number of holes", "kind": "number", "default": 6 },
        { "name": "center", "prompt": "Centre of the bolt circle", "kind": "point" }
      ],
      "preview": true,
      "timeout": 60
    },
    { "name": "ORIGINMARK", "script": "originmark.scr" }
  ]
}
```

| Field | Meaning |
|---|---|
| `name` | The command name (one word; case does not matter) |
| `run` | The program and its arguments, started in the plugin's folder. A program file inside the folder is used if there is one, otherwise it is looked up on `PATH` |
| `script` | Instead of `run`: a command script in the plugin's folder |
| `inputs` | What the command asks for, in order. `kind` is `point` (picked or typed), `number`, `text` or `select` (objects); `default` is used when Enter is pressed |
| `timeout` | Seconds the program may run (default 60) |
| `preview` | When the last input is a point: show what the command would add while that point is being picked. The program is asked again as the cursor moves, with `"preview": true` in the request, the cursor as that point, and no `document` or `description`; the `add` operations of its answer are drawn as the preview |

### The request

The program receives one JSON object on standard input:

```json
{
  "protocol": 1,
  "command": "BOLTCIRCLE",
  "inputs": { "pcd": 30, "count": 4, "center": [60, 30] },
  "selection": [12, 15],
  "units": "mm",
  "description": "text description of the drawing, as AI Chat sees it",
  "document": { "...": "the whole drawing in ImagineCAD's .icad JSON form" }
}
```

Points are `[x, y]` in drawing units with y up; `select` inputs are lists of
entity ids, which `document.entities[].id` identifies.

### The answer

The program prints one JSON object on standard output and exits with status
0:

```json
{
  "message": "4 holes Ø5 on Ø30.",
  "actions": [
    { "op": "add", "layer": "OUTLINE", "shape": { "type": "circle", "c": [75, 30], "r": 2.5 } },
    { "op": "move", "ids": [12], "dx": 10, "dy": 0 }
  ],
  "script": "optional command script to run afterwards"
}
```

`actions` are the edit operations AI Chat uses: `add` (lines, arcs, circles,
polylines, text, dimensions, leaders, hatches, tables, symbols…), `update`,
`delete`, `move`, `copy`, `rotate`, `scale`, `mirror`, `set` (layer, colour,
linetype, text), `layer`, `title`, `sheet`, `dimstyle`, `select` and `zoom`.
They are applied as one undo step. The full list, with every shape's fields,
is the operation reference in `crates/icad-vision/src/assistant.rs`
(`system_prompt`).

A non-zero exit status, an answer that is not JSON, or running past the
timeout cancels the command with a message; the drawing is not changed. A
line written to standard error is shown with the failure.

### Example

[`examples/plugins/bolt-circle`](../examples/plugins/bolt-circle) has a
program command in Python (`BOLTCIRCLE`) and a script command
(`ORIGINMARK`). Try it without installing:

```bash
IMAGINECAD_PLUGINS=examples/plugins imaginecad script examples/scripts/plate.scr -o plate.pdf
```
