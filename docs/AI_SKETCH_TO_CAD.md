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

## Connecting an AI model

The first time you use Image to CAD, a **Connect an AI model** dialog opens
(later: **AI → Connect API Key** or **AI Settings**).

### Any API key (auto-configured)

Paste a key and click **Detect provider & model**. ImagineCAD recognises the
provider from the key, verifies it by listing that provider's models, and picks
the best vision-capable model. Only models that accept images are offered.
You can choose a different model before saving.

If the configured model turns out to be text-only (the provider rejects the
image, for example OpenRouter's `404 No endpoints found that support image
input`), ImagineCAD picks a vision model from the same provider, retries once,
and saves that model in AI Settings.

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

1. **Reading values (AI).** The image (aspect ratio kept, longest side at most 2000 px) is sent with a strict JSON prompt. The model returns each dimension text (`50`, `12.5`, `Ø20`, `2x Ø8`, `R5`, `M6`, `45°`) and its bounding box, ignoring crossed-out values.
   - **Part reconstruction.** When the model recognises a turned **shaft** (stepped diameters, keyways, chamfers) or a rectangular **plate** with holes, it also returns the part's parameters. ImagineCAD then builds the drawing directly from the written dimensions. It is exact by construction, with every length, diameter, keyway and chamfer dimensioned, laid out for the sheet scale, and on a sheet sized for slender parts (for example A3 at 1:10 for a 2.8 m shaft). Steps 2–5 are skipped. Turn this off with **Let AI rebuild shafts and plates exactly from the written dimensions** in the Image to CAD dialog.
2. **Vectorising (computer vision).** Adaptive thresholding, removal of handwriting and specks, Zhang–Suen thinning, skeleton tracing, Douglas–Peucker lines and least-squares circle/arc fits.
3. **Part vs. annotation.** Components that enclose real area are the part. For each label the pipeline finds its dimension line, extension lines (following them through outline crossings) and arrowheads; leader arrows decide which hole a Ø/R value belongs to.
4. **Beautification.** Snap angles, merge strokes, align coordinates, weld endpoints, close T-junctions.
5. **Making dimensions true.** The distinct x and y coordinates become *stations*. Horizontal and vertical dimensions are hard constraints, undimensioned gaps are springs that keep the sketch's proportions, and a least-squares solve per axis repositions everything through a monotone map. Contradictory dimensions are dropped and reported.

## Tips

- Dark pen on plain paper; photograph straight-on with even light, or scan at 200–300 dpi.
- Closed outlines; a small gap between extension lines and the part.
- Clear values next to each dimension line; leader arrows to holes for Ø/R values.
- Proportions don't matter, because the written dimensions win.

## Limitations

- Horizontal/vertical dimensions are enforced exactly; slanted and angular values are drawn but not yet enforced.
- One view per conversion works best.
- Part reconstruction covers shafts and rectangular plates. Keyway widths that are not written on the sketch are drawn at a nominal width; check them.
- Hatching, threads and surface-finish symbols are not recognised.

## Command line

```bash
imaginecad convert sketch.jpg -o part.dwg                       # stored key / settings
imaginecad convert sketch.jpg -o part.pdf --api-key sk-ant-…    # any provider
imaginecad convert sketch.jpg -o part.dxf --model qwen2.5vl:7b  # local Ollama
imaginecad convert sketch.jpg -o part.svg --no-ai --size 120
```
