# Releasing installers

ImagineCAD's GitHub Actions workflow (`.github/workflows/release.yml`) builds
the macOS, Windows and Linux installers and publishes them on the repository's
**Releases** page automatically.

## What gets built

| Job | Runner | Output |
|---|---|---|
| macOS | `macos-latest` | `ImagineCAD-<ver>-macos-universal.dmg` (Apple silicon + Intel) |
| Windows | `windows-latest` | `ImagineCAD-<ver>-windows-x64-setup.exe`, `ImagineCAD-<ver>-windows-x64-portable.zip` |
| Linux | `ubuntu-22.04` | `imaginecad_<ver>-1_amd64.deb`, `ImagineCAD-<ver>-linux-x86_64.tar.gz` |
| Publish | `ubuntu-latest` | GitHub Release with all files, `SHA256SUMS.txt` and generated release notes |

Every platform job also:

1. builds GNU LibreDWG from source with ImagineCAD's patch (`packaging/libredwg/`) and bundles `dwg2dxf`/`dxf2dwg`, so DWG works out of the box;
2. runs the DWG save → reopen round-trip test against that build;
3. generates `THIRD_PARTY_LICENSES.html` with `cargo-about` and ships it with `LICENSE` and `NOTICE`.

## One-time setup

1. Push the repository to GitHub.
2. **Settings → Actions → General**: allow GitHub Actions, and under *Workflow permissions* select **Read and write permissions**. The workflow also declares `contents: write`.
3. Optional: add the code-signing secrets below.

## Publishing a release

```bash
# 1. bump `version` in the root Cargo.toml and add a CHANGELOG.md entry
git commit -am "Release 0.3.0"
# 2. tag and push
git tag v0.3.0
git push origin main --tags
```

Pushing a tag that starts with `v` starts the workflow. After about 20–30
minutes the release appears under **Releases** with all installers attached.

To build installers **without** publishing, open **Actions → Release → Run
workflow**. The installers then appear as downloadable artifacts on that
workflow run.

## Code signing (recommended for public distribution)

Unsigned builds work, but users see warnings: macOS Gatekeeper refuses to
open an unsigned app downloaded from the internet until it is allowed in
*System Settings → Privacy & Security*, and Windows SmartScreen shows "Unknown
publisher". Add these repository secrets (**Settings → Secrets and variables
→ Actions**) and the workflow signs automatically:

| Secret | Purpose |
|---|---|
| `MACOS_CERTIFICATE` | Base64 of your *Developer ID Application* `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `MACOS_CERTIFICATE_PWD` | Password of that `.p12` |
| `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Proteen Das (TEAMID)` |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | Apple ID, team ID and an app-specific password, for notarisation and stapling |
| `WINDOWS_CERTIFICATE` | Base64 of your code-signing `.pfx` |
| `WINDOWS_CERTIFICATE_PWD` | Password of that `.pfx` |

An Apple Developer Program membership is required for Developer ID signing and
notarisation. Windows code-signing certificates are available from public CAs
(or Azure Trusted Signing).

## Building installers locally

```bash
packaging/libredwg/build-libredwg.sh dist/libredwg universal      # macOS (or omit 'universal')
cargo about generate packaging/licenses/about.hbs -o dist/THIRD_PARTY_LICENSES.html
LIBREDWG_DIR=dist/libredwg UNIVERSAL=1 packaging/macos/build-dmg.sh
LIBREDWG_DIR=dist/libredwg packaging/linux/build-linux.sh          # on Linux
```

On Windows, build LibreDWG in an MSYS2 MINGW64 shell with the same script,
then compile `packaging/windows/imaginecad.iss` with Inno Setup (see
[BUILDING.md](BUILDING.md)).
