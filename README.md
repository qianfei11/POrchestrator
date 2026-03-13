# Porchestrator

Porchestrator is a Tauri desktop AI agent that turns a user brief and source documents into a polished PowerPoint deck. It uses a Rust backend for document ingestion, model orchestration, image generation, and `.pptx` export.

Current public release: [`v0.1.0`](https://github.com/qianfei11/POrchestrator/releases/tag/v0.1.0)

## Stack

- Frontend: React + Vite
- Desktop shell: Tauri 2
- Backend: Rust
- LLM text adapters:
  - OpenAI-compatible `/v1/chat/completions`
  - Anthropic-compatible `/v1/messages`
- Image generation: OpenAI-compatible `/v1/images/generations`
- PowerPoint generation: native Rust via `ppt-rs` plus package patching for embedded media

## Features

- Load source files from the desktop app
- Extract readable text from:
  - `.txt`
  - `.md` / `.markdown`
  - `.pdf`
  - `.docx`
  - `.csv`
  - `.json`
  - `.yaml` / `.yml`
  - `.toml`
- Add a brief, audience, and desired outcome when generating a deck
- Generate an exact slide budget from 4 to 20 slides
- Use either OpenAI-style or Anthropic-style text model settings
- Optionally use a separate image model such as `gpt-image-1` or a compatible deployment like `nano-banana-2`
- Preview the LLM-generated outline before export
- Use the LLM-generated deck title as the default `.pptx` filename
- Export valid `.pptx` files with embedded images
- Save generated images locally beside the deck in a sibling `*_assets` folder
- Use a compact desktop UI with foldable LLM settings and foldable brief snapshot sections

## Desktop output

Porchestrator writes a PowerPoint deck to a user-selected path and stores any generated slide images in a local asset folder next to the deck:

```text
My Deck.pptx
My Deck_assets/
```

## Local development

### Prerequisites

- Node.js 22
- Rust stable
- On Linux, the Tauri/WebKitGTK build dependencies used in the release workflow

### Install dependencies

```bash
npm ci
```

### Start the desktop app

```bash
npm run tauri dev
```

### Verify the project

```bash
npm run build
npm run lint
cargo test --manifest-path src-tauri/Cargo.toml
```

## Production build

```bash
npm run tauri build
```

On Linux this produces release artifacts under `src-tauri/target/release/`, including bundled packages such as:

- `.AppImage`
- `.deb`
- `.rpm`

## Release automation

The release workflow is in `.github/workflows/release.yml`.

It runs on:

- tag push matching `v*`
- manual workflow dispatch

It publishes cross-platform binaries for the current app version:

- Windows x64:
  - `.exe` installer
  - `.msi`
- macOS Apple Silicon:
  - `.dmg`
  - `.app.tar.gz`
- Linux x64:
  - `.AppImage`
  - `.deb`
  - `.rpm`

For `v0.1.0`, the published release assets are:

- `Porchestrator_0.1.0_x64-setup.exe`
- `Porchestrator_0.1.0_x64_en-US.msi`
- `Porchestrator_0.1.0_aarch64.dmg`
- `Porchestrator_aarch64.app.tar.gz`
- `Porchestrator_0.1.0_amd64.AppImage`
- `Porchestrator_0.1.0_amd64.deb`
- `Porchestrator-0.1.0-1.x86_64.rpm`
- `porchestrator-v0.1.0-sha256.txt`
