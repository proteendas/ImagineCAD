# Image to CAD

ImagineCAD turns a photo or scan of a hand-drawn mechanical sketch into a
clean, editable, dimension-exact CAD drawing with a title block.

## Using it

1. **AI → Image to CAD** (`Ctrl+I`), or drop an image on the window
   (`png`, `jpg`, `bmp`, `tiff`, `webp`).
2. On first use, connect an AI model (see below).
3. Optionally adjust *Straighten lines within* (default 7°), *Sketch
   smoothing*, *Size if no dimensions*, *Processing resolution*, *Add centre
   lines to holes*.
4. **Convert to CAD.** The log reports, for example,
   `4 label(s) → 3 linear and 1 radial dimension(s); 4 dimension(s) satisfied exactly`.
5. **Open as new drawing** (Layout view, sheet fitted) or **Insert into current drawing**.

| Layer | Contents |
|---|---|
| `OUTLINE` | part geometry |
| `CENTER` | centre marks |
| `DIMENSIONS` | real dimension objects measured from the geometry |
| `AI-REVIEW` | uncertain strokes and unmatched labels (magenta, not plotted) |
| `AI-SKETCH-ANNOTATION` | the sketch's own dimension lines and handwriting (hidden) |
| `AI-SKETCH-IMAGE` | the sketch itself under the result, faded, for review (not plotted) |
| `HATCH`, `HIDDEN` | section hatching and hidden edges of rebuilt parts |

**Reviewing the result.** With *Overlay the sketch to review the result* (on by
default, with a *fade* slider), the photo is placed under the drawing so you can
compare them. Change its fade in the Properties palette, or turn the layer off
when you are done.

## Connecting an AI model

The first time you use Image to CAD, a **Connect an AI model** dialog opens
(later: **AI → Connect API Key** or **AI Settings**).

### Any API key (auto-configured)

Paste a key and click **Detect provider & model**. ImagineCAD recognises the
provider from the key, verifies it by listing that provider's models, and picks
the best vision-capable model. Only models that accept images are offered.
You can choose a different model before saving, or later in **AI Settings**:
the model dropdown is searchable, marks each model **FREE** or **PAID** where
the provider publishes prices (OpenRouter does; OpenAI and Anthropic APIs are
paid; local models are free), and has a *Free only* filter.

If the configured model turns out to be text-only (the provider rejects the
image, for example OpenRouter's `404 No endpoints found that support image
input`), ImagineCAD picks a vision model from the same provider, retries once,
and saves that model in AI Settings.

OpenRouter keys without credit are set up with a free vision model (ids ending
in `:free`, for example `google/gemma-4-31b-it:free`). If a paid model answers
`402` (not enough credit), ImagineCAD switches to a free vision model the same
way. Free models are rate-limited by OpenRouter.

### Choosing a model

Reading handwritten dimensions and describing a whole part is demanding.
Strong vision models (current Gemini, Claude, GPT models) read sketches reliably
and get most parts exact. Small free models often misread digits or describe
the part wrongly; ImagineCAD's checks catch that (see *Validation* below) and
fall back to tracing the sketch, so the result is correct but less polished.
A free Google AI Studio key (`AIza…`) gives access to Gemini vision models
within Google's free-tier limits and is the best free choice at the time of
writing.

| Provider | Key looks like | Default model choice |
|---|---|---|
| Anthropic | `sk-ant-…` | `claude-opus-5-5` (or the newest vision model on the account) |
| OpenAI | `sk-proj-…`, `sk-…` | newest `gpt-5` / `gpt-4.1` / `gpt-4o` |
| Google Gemini | `AIza…` | `gemini-2.5-flash` family |
| OpenRouter | `sk-or-…` | a vision model from the catalogue |
| Groq | `gsk_…` | Llama 4 vision models |
| xAI | `xai-…` | Grok vision models |
| Hugging Face | `hf_…` | Qwen2.5-VL |
| NVIDIA | `nvapi-…` | Llama vision models |
| Fireworks | `fw_…` | Llama 4 / Qwen-VL |
| Mistral, Together AI | no prefix | Pixtral / Llama 4 / Qwen-VL |
| Anything else | | **Other OpenAI-compatible endpoint**: enter its base URL |

The key is stored in the **macOS Keychain**, **Windows Credential Manager**,
or `ImagineCAD/credentials` with owner-only permissions on Linux. It is never
written to `settings.json`, and it is only sent to the provider it belongs to.
With a cloud key, the sketch image is sent to that provider for analysis;
review the provider's data policy for confidential drawings.

For scripts and CI, `IMAGINECAD_API_KEY` overrides the stored key, and the CLI
accepts `--api-key` (auto-detected the same way).

### Free and private alternatives

- **Ollama** (local, offline): install from <https://ollama.com>, run `ollama pull qwen2.5vl:7b`, choose *Ollama* in AI Settings.
- **Local OpenAI-compatible servers**: LM Studio (`http://localhost:1234/v1`), llama.cpp `llama-server` (`http://localhost:8080/v1`), vLLM.
- **Tesseract OCR** (optional, printed numbers): `brew install tesseract` · `sudo apt install tesseract-ocr` · UB-Mannheim installer on Windows.
- **Off**: geometry only; the drawing is scaled to the nominal size and you add dimensions yourself.

## How it works

1. **Reading values (AI).** A sharpened, contrast-stretched copy of the image (longest side at most 2400 px) is sent with a strict JSON prompt. The model returns each dimension text (`50`, `12.5`, `Ø20`, `2x Ø8`, `R5`, `M6`, `45°`) and its bounding box, ignoring crossed-out values.
   - **Part reconstruction.** The model also describes the part, using the written values:
     - a turned **shaft** (stepped diameters, keyways, chamfers);
     - a **round part**: ring, washer, bush, flange (hub, bolt circle), wheel or pulley (rim, web, hub, V-grooves, bore keyway), drawn as an end view plus a hatched section;
     - a **spur gear** (module, number of teeth, pressure angle, face width, bore, hub, keyway), drawn with true involute teeth, a hatched section and a gear data table;
     - a helical **compression spring** (wire, outside diameter, free length, active and total coils, ground ends), drawn in section with a spring data table;
     - a rectangular **plate** with holes;
     - any other **profile**: exact lines, arcs, circles and polylines in millimetres, with views placed in projection, plus the dimensions it read.
     ImagineCAD builds the drawing directly from that description, with every value dimensioned and the dimensions laid out clear of the part and of each other on a sheet sized for it. Turn this off with **Let AI rebuild the part exactly from the written dimensions**.
   - **Validation.** Before a description is drawn it is checked:
     - against the values the model read: every written length, diameter and angle must be used, and every diameter in the description must be written;
     - for gaps: a step or groove without a size;
     - for a profile, by measuring every dimension on its own geometry, and checking that its points lie on that geometry.
     A description with problems is sent back once with the list of problems ("the sketch shows Ø140 but no segment has that diameter", "the dimension 50 measures 48 on the geometry"). If it still does not check out, it is not drawn: the sketch is traced with the values instead (steps 2–7) and the reason is reported.
2. **Vectorising (computer vision).** Adaptive thresholding, removal of handwriting, specks and a printed sheet border, Zhang–Suen thinning, skeleton tracing, Douglas–Peucker lines and least-squares circle/arc fits. An arc is accepted only where the stroke curves evenly, so obtuse corners and a straight run meeting a fillet stay separate.
3. **Part vs. annotation.** Components that enclose real area are the part. For each label the pipeline finds its dimension line, extension lines (following them through outline crossings) and arrowheads; leader arrows decide which hole a Ø/R value belongs to; an angle value goes to the corner it sits in. A label next to an outline edge with no dimension line of its own dimensions that edge.
4. **Drafting conventions.** Rows of short collinear strokes with clear gaps become hidden lines (equal dashes) or centre lines (long and short dashes). A regular family of parallel strokes at about 45° running from edge to edge becomes section hatching filling the regions it covers. Handwriting is not taken for hatching.
5. **Beautification and views.** Snap angles, merge strokes, align coordinates, weld endpoints, close T-junctions. Separate views (front above top, side beside front) are recognised and their edges snapped to each other, so hand-drawn misalignment between views does not carry over.
6. **Making dimensions true.** The distinct x and y coordinates become *stations*. Horizontal and vertical dimensions are hard constraints, undimensioned gaps are springs that keep the sketch's proportions, and a least-squares solve per axis repositions everything through a monotone map. Contradictory dimensions are dropped and reported.
7. **Slanted lengths, angles and radii.** A Levenberg–Marquardt solve over the outline vertices then enforces what one axis at a time cannot: slanted lengths, angles between edges, arc radii and fillet tangency, while horizontal and vertical edges and dimensions stay as solved.

## Tips

- Dark pen on plain paper; photograph straight-on with even light, or scan at 200–300 dpi.
- Closed outlines; a small gap between extension lines and the part.
- Clear values next to each dimension line; leader arrows to holes for Ø/R values.
- Proportions don't matter, because the written dimensions win.

## Limitations

- How exact a rebuilt part is depends on the model reading the sketch correctly; the checks above stop wrong descriptions, but cannot recover a value no model could read.
- Dense, finely printed hatching (CAD printouts) is traced as strokes rather than recognised as a hatch; hand-drawn hatching is recognised.
- Very large outlines (more than 160 vertices) skip the slanted/angle refinement; horizontal and vertical dimensions are still exact.
- Keyway widths that are not written on the sketch are drawn at a nominal width; check them.
- Threads and surface-finish symbols are not recognised.

## Command line

```bash
imaginecad convert sketch.jpg -o part.dwg                       # stored key / settings
imaginecad convert sketch.jpg -o part.pdf --api-key sk-ant-…    # any provider
imaginecad convert sketch.jpg -o part.dxf --model qwen2.5vl:7b  # local Ollama
imaginecad convert sketch.jpg -o part.svg --no-ai --size 120
```
