# Porchestrator

Porchestrator is a cross-platform desktop AI agent that turns user-provided context and documents into `.pptx` presentations.

## Stack

- Frontend: React + Vite with a clean 8-bit desktop UI
- Desktop shell: Tauri 2
- Backend: Rust
- LLM adapters: OpenAI-compatible chat completions and Anthropic-compatible messages APIs
- PowerPoint generation: native Rust via `ppt-rs`

## Features

- Paste a brief, audience, and desired outcome
- Load source files from the desktop app
- Extract readable text from `.txt`, `.md`, `.pdf`, `.docx`, `.csv`, `.json`, `.yaml`, and `.toml`
- Generate a structured slide outline through either supported provider style
- Export a PowerPoint deck directly to a chosen `.pptx` path
- Preview the returned slide structure inside the app before presenting

## Local development

```bash
npm install
npm run tauri dev
```

## Production build

```bash
npm run tauri build
```

## Release automation

`.github/workflows/release.yml` builds desktop artifacts for:

- Windows
- Linux
- macOS

Push a tag like `v0.1.0` or trigger the workflow manually to create a draft GitHub release with platform binaries.
