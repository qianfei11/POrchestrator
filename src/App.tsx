import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { startTransition, useState } from "react";
import "./App.css";
import {
  PROVIDER_PRESETS,
  type DeckOutline,
  type ExportPresentationRequest,
  type ExportResult,
  type GeneratePresentationRequest,
  type GenerationResult,
  type ProviderKind,
  type ProviderSettings,
  type SourceDocument,
} from "./lib/contracts";

const desktopRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function ensurePptxExtension(filePath: string) {
  return filePath.toLowerCase().endsWith(".pptx") ? filePath : `${filePath}.pptx`;
}

function slugify(input: string) {
  return (
    input
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 48) || "porchestrator-deck"
  );
}

function buildProvider(kind: ProviderKind): ProviderSettings {
  return { ...PROVIDER_PRESETS[kind] };
}

function providerSummary(kind: ProviderKind) {
  return kind === "openaiCompatible"
    ? "Uses /v1/chat/completions."
    : "Uses /v1/messages with the Anthropic version header.";
}

function App() {
  const [provider, setProvider] = useState<ProviderSettings>(
    buildProvider("openaiCompatible"),
  );
  const [briefing, setBriefing] = useState("");
  const [audience, setAudience] = useState("");
  const [desiredOutcome, setDesiredOutcome] = useState("");
  const [maxSlides, setMaxSlides] = useState(8);
  const [documents, setDocuments] = useState<SourceDocument[]>([]);
  const [outputPath, setOutputPath] = useState("");
  const [result, setResult] = useState<GenerationResult | null>(null);
  const [status, setStatus] = useState("Ready for source material or a brief.");
  const [error, setError] = useState("");
  const [isBusy, setIsBusy] = useState(false);
  const [isBriefModalOpen, setIsBriefModalOpen] = useState(false);

  const totalCharacters = documents.reduce(
    (total, document) => total + document.characters,
    0,
  );

  async function chooseDocuments() {
    if (!desktopRuntime) {
      setError("Run Porchestrator inside the Tauri desktop shell to import files.");
      return;
    }

    setError("");
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: "Readable sources",
          extensions: [
            "txt",
            "md",
            "markdown",
            "pdf",
            "docx",
            "csv",
            "json",
            "yaml",
            "yml",
            "toml",
          ],
        },
      ],
    });

    const paths = Array.isArray(selection)
      ? selection
      : selection
        ? [selection]
        : [];

    if (!paths.length) {
      return;
    }

    try {
      setStatus(`Ingesting ${paths.length} document${paths.length === 1 ? "" : "s"}...`);
      const ingested = await invoke<SourceDocument[]>("ingest_documents", { paths });
      setDocuments((current) => {
        const next = new Map(
          current.map((document) => [document.path ?? document.name, document]),
        );
        for (const document of ingested) {
          next.set(document.path ?? document.name, document);
        }
        return [...next.values()];
      });
      setStatus("Source material loaded into the deck builder.");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setStatus("Document import failed.");
    }
  }

  function removeDocument(pathOrName: string) {
    setDocuments((current) =>
      current.filter((document) => (document.path ?? document.name) !== pathOrName),
    );
  }

  function switchProvider(kind: ProviderKind) {
    setProvider(buildProvider(kind));
    setError("");
    setStatus(providerSummary(kind));
  }

  function openBriefModal() {
    setError("");
    setIsBriefModalOpen(true);
  }

  function closeBriefModal() {
    if (!isBusy) {
      setIsBriefModalOpen(false);
    }
  }

  async function exportDeck(outline: DeckOutline) {
    if (!desktopRuntime) {
      setError("Run Porchestrator inside the Tauri desktop shell to export PowerPoint files.");
      return null;
    }

    const selection = await save({
      defaultPath: `${slugify(outline.deckTitle)}.pptx`,
      filters: [{ name: "PowerPoint deck", extensions: ["pptx"] }],
    });

    if (!selection) {
      setStatus(`Outline ready for ${outline.deckTitle}. Export cancelled.`);
      return null;
    }

    const request: ExportPresentationRequest = {
      outline,
      outputPath: ensurePptxExtension(selection),
    };

    const exportResult = await invoke<ExportResult>("export_presentation", {
      request,
    });

    setOutputPath(exportResult.outputPath);
    setStatus(
      `Deck ready: ${exportResult.deckTitle} with ${exportResult.slideCount} slides.`,
    );
    return exportResult;
  }

  async function generateDeck() {
    setError("");
    setOutputPath("");

    if (!provider.apiKey.trim()) {
      setError("An API key is required.");
      return;
    }

    if (!briefing.trim() && !documents.length) {
      setError("Provide a written brief or load at least one document.");
      return;
    }

    const request: GeneratePresentationRequest = {
      provider,
      briefing,
      audience,
      desiredOutcome,
      maxSlides,
      documents,
    };

    try {
      setIsBusy(true);
      setIsBriefModalOpen(false);
      setStatus("Generating slide outline...");
      const generation = await invoke<GenerationResult>("generate_outline", {
        request,
      });
      startTransition(() => setResult(generation));
      setStatus(`Outline ready for ${generation.deckTitle}. Opening save dialog...`);
      await exportDeck(generation.outline);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setStatus("Deck generation failed.");
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <>
      <main className="shell">
        <header className="topbar panel">
          <div className="brand">
            <p className="eyebrow">AI PowerPoint Agent</p>
            <h1>Porchestrator</h1>
            <p className="summary">
              Rust backend, native desktop shell, and a tighter retro-tech layout
              for turning source files into clean PowerPoint decks.
            </p>
          </div>

          <div className="status-pills" aria-hidden="true">
            <div className="status-pill">
              <span>Provider</span>
              <strong>{provider.kind === "openaiCompatible" ? "OpenAI" : "Anthropic"}</strong>
            </div>
            <div className="status-pill">
              <span>Slides</span>
              <strong>{maxSlides}</strong>
            </div>
            <div className="status-pill">
              <span>Docs</span>
              <strong>{documents.length}</strong>
            </div>
          </div>
        </header>

        <section className="workspace">
          <section className="panel">
            <div className="section-heading compact">
              <h2>LLM Settings</h2>
              <p>{providerSummary(provider.kind)}</p>
            </div>

            <div className="segmented-control">
              <button
                className={provider.kind === "openaiCompatible" ? "active" : ""}
                onClick={() => switchProvider("openaiCompatible")}
                type="button"
              >
                OpenAI Style
              </button>
              <button
                className={provider.kind === "anthropicCompatible" ? "active" : ""}
                onClick={() => switchProvider("anthropicCompatible")}
                type="button"
              >
                Anthropic Style
              </button>
            </div>

            <div className="form-grid">
              <label>
                Base URL
                <input
                  value={provider.baseUrl}
                  onChange={(event) =>
                    setProvider((current) => ({
                      ...current,
                      baseUrl: event.target.value,
                    }))
                  }
                  placeholder="https://api.openai.com/v1"
                />
              </label>
              <label>
                Model
                <input
                  value={provider.model}
                  onChange={(event) =>
                    setProvider((current) => ({
                      ...current,
                      model: event.target.value,
                    }))
                  }
                  placeholder="gpt-4.1-mini"
                />
              </label>
              <label className="span-2">
                API Key
                <input
                  type="password"
                  value={provider.apiKey}
                  onChange={(event) =>
                    setProvider((current) => ({
                      ...current,
                      apiKey: event.target.value,
                    }))
                  }
                  placeholder="sk-..."
                />
              </label>
              <label>
                Temperature
                <input
                  type="number"
                  min="0"
                  max="1.2"
                  step="0.1"
                  value={provider.temperature}
                  onChange={(event) =>
                    setProvider((current) => ({
                      ...current,
                      temperature: Number(event.target.value),
                    }))
                  }
                />
              </label>
              <label>
                Slide Budget
                <input
                  type="number"
                  min="4"
                  max="12"
                  value={maxSlides}
                  onChange={(event) => setMaxSlides(Number(event.target.value))}
                />
              </label>
            </div>

            <div className="api-note">
              <strong>Defaults verified:</strong> OpenAI-compatible requests target
              `chat/completions`; Anthropic-compatible requests target `messages`
              with `anthropic-version: 2023-06-01`.
            </div>
          </section>

          <section className="panel">
            <div className="section-heading compact">
              <h2>Source Material</h2>
              <p>Readable text is extracted before the model call and trimmed to stay inside prompt bounds.</p>
            </div>

            <div className="toolbar compact-toolbar">
              <button
                className="primary-button"
                onClick={() => void chooseDocuments()}
                type="button"
              >
                Load Documents
              </button>
              <div className="totals">
                <span>{documents.length} files</span>
                <span>{totalCharacters.toLocaleString()} chars</span>
              </div>
            </div>

            <div className="document-list">
              {documents.length ? (
                documents.map((document) => (
                  <article className="document-card" key={document.path ?? document.name}>
                    <div>
                      <h3>{document.name}</h3>
                      <p>
                        {document.extension || "text"} · {document.characters.toLocaleString()} chars
                        {document.truncated ? " · truncated" : ""}
                      </p>
                    </div>
                    <button
                      className="mini-button"
                      onClick={() => removeDocument(document.path ?? document.name)}
                      type="button"
                    >
                      Remove
                    </button>
                  </article>
                ))
              ) : (
                <div className="empty-state">
                  <p>No documents loaded yet.</p>
                </div>
              )}
            </div>
          </section>
        </section>

        <section className="panel control-panel">
          <div className="section-heading compact">
            <h2>Run</h2>
            <p>{desktopRuntime ? "Briefing is collected only when you click Generate Deck." : "Preview mode only. Launch through Tauri for desktop generation."}</p>
          </div>

          <div className="control-grid">
            <div className="status-block">
              <p>{status}</p>
              <p className="path-label">{outputPath || "No deck exported yet."}</p>
            </div>

            <div className="brief-hint">
              <span>
                {briefing
                  ? `Brief stored • ${briefing.length} chars. Update it from Generate Deck.`
                  : "No brief stored yet. Add it when you click Generate Deck."}
              </span>
            </div>

            <div className="action-row">
              <button
                className="primary-button large"
                disabled={isBusy}
                onClick={openBriefModal}
                type="button"
              >
                {isBusy ? "Working..." : "Generate Deck"}
              </button>
              <button
                className="ghost-button large"
                disabled={isBusy || !result}
                onClick={() => void (result ? exportDeck(result.outline) : Promise.resolve())}
                type="button"
              >
                Save Again
              </button>
            </div>
          </div>

          {error ? <div className="error-banner">{error}</div> : null}
        </section>

        <section className="panel preview-panel">
          <div className="section-heading compact">
            <h2>Deck Preview</h2>
            <p>Outline returned by the model before the PowerPoint file is written.</p>
          </div>

          {result ? (
            <DeckPreview outline={result.outline} outputPath={outputPath} />
          ) : (
            <div className="empty-state preview-empty">
              <p>Generate a deck to inspect the outline.</p>
            </div>
          )}
        </section>
      </main>

      {isBriefModalOpen ? (
        <div
          className="modal-backdrop"
          onClick={closeBriefModal}
          role="presentation"
        >
          <section
            aria-labelledby="brief-modal-title"
            className="brief-modal panel"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="section-heading compact">
              <h2 id="brief-modal-title">Brief</h2>
              <p>Set the narrative only when you are ready to generate.</p>
            </div>

            <div className="form-grid">
              <label>
                Audience
                <input
                  value={audience}
                  onChange={(event) => setAudience(event.target.value)}
                  placeholder="Board, clients, leadership..."
                />
              </label>
              <label>
                Desired Outcome
                <input
                  value={desiredOutcome}
                  onChange={(event) => setDesiredOutcome(event.target.value)}
                  placeholder="Approval, decision, status update..."
                />
              </label>
              <label className="span-2">
                Prompt
                <textarea
                  value={briefing}
                  onChange={(event) => setBriefing(event.target.value)}
                  rows={8}
                  placeholder="Summarize the uploaded material into a concise 8-slide product update for leadership..."
                />
              </label>
            </div>

            <div className="modal-actions">
              <button
                className="ghost-button large"
                disabled={isBusy}
                onClick={closeBriefModal}
                type="button"
              >
                Cancel
              </button>
              <button
                className="primary-button large"
                disabled={isBusy}
                onClick={() => void generateDeck()}
                type="button"
              >
                {isBusy ? "Working..." : "Generate and Save"}
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </>
  );
}

function DeckPreview({
  outline,
  outputPath,
}: {
  outline: DeckOutline;
  outputPath: string;
}) {
  return (
    <div className="preview-grid">
      <article className="preview-summary">
        <p className="eyebrow">LLM Title</p>
        <h3>{outline.deckTitle}</h3>
        <p>{outline.subtitle}</p>
        <p className="theme-tag">{outline.themeTagline}</p>
        <p className="path-label">{outputPath || "Not exported yet."}</p>
      </article>

      <div className="slide-grid">
        {outline.slides.map((slide, index) => (
          <article className="slide-card" key={`${slide.title}-${index}`}>
            <div className="slide-meta">
              <span>{String(index + 1).padStart(2, "0")}</span>
              <span>{slide.layout}</span>
            </div>
            <h3>{slide.title}</h3>
            <ul>
              {slide.bullets.map((bullet) => (
                <li key={bullet}>{bullet}</li>
              ))}
            </ul>
            <p className="slide-highlight">{slide.highlight}</p>
          </article>
        ))}
      </div>
    </div>
  );
}

export default App;
