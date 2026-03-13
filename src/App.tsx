import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { startTransition, useState } from "react";
import "./App.css";
import {
  PROVIDER_PRESETS,
  type DeckOutline,
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

function App() {
  const [provider, setProvider] = useState<ProviderSettings>(
    buildProvider("openaiCompatible"),
  );
  const [briefing, setBriefing] = useState("");
  const [audience, setAudience] = useState("");
  const [desiredOutcome, setDesiredOutcome] = useState("");
  const [deckName, setDeckName] = useState("porchestrator-deck");
  const [maxSlides, setMaxSlides] = useState(8);
  const [documents, setDocuments] = useState<SourceDocument[]>([]);
  const [outputPath, setOutputPath] = useState("");
  const [result, setResult] = useState<GenerationResult | null>(null);
  const [status, setStatus] = useState("Waiting for a brief.");
  const [error, setError] = useState("");
  const [isBusy, setIsBusy] = useState(false);

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
  }

  async function chooseOutputPath() {
    if (!desktopRuntime) {
      setError("Run Porchestrator inside the Tauri desktop shell to export PowerPoint files.");
      return null;
    }

    const selection = await save({
      defaultPath: `${slugify(deckName)}.pptx`,
      filters: [{ name: "PowerPoint deck", extensions: ["pptx"] }],
    });

    if (!selection) {
      return null;
    }

    const normalizedPath = ensurePptxExtension(selection);
    setOutputPath(normalizedPath);
    return normalizedPath;
  }

  async function generateDeck() {
    setError("");
    setResult(null);

    if (!provider.apiKey.trim()) {
      setError("An API key is required.");
      return;
    }

    if (!briefing.trim() && !documents.length) {
      setError("Provide a written brief or load at least one document.");
      return;
    }

    const selectedOutput = outputPath || (await chooseOutputPath());
    if (!selectedOutput) {
      setStatus("Export cancelled.");
      return;
    }

    const request: GeneratePresentationRequest = {
      provider,
      briefing,
      audience,
      desiredOutcome,
      maxSlides,
      outputPath: selectedOutput,
      documents,
    };

    try {
      setIsBusy(true);
      setStatus("Porchestrator is outlining slides and writing the .pptx...");
      const generation = await invoke<GenerationResult>("generate_presentation", {
        request,
      });
      startTransition(() => setResult(generation));
      setDeckName(slugify(generation.deckTitle));
      setStatus(
        `Deck ready: ${generation.deckTitle} with ${generation.slideCount} slides.`,
      );
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setStatus("Deck generation failed.");
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <main className="shell">
      <section className="hero-panel panel">
        <div className="hero-copy">
          <p className="eyebrow">Desktop PowerPoint Agent</p>
          <h1>Porchestrator</h1>
          <p className="hero-text">
            Turn pasted context and source documents into polished PowerPoint
            decks through a Rust backend with OpenAI-compatible and
            Anthropic-compatible model adapters.
          </p>
        </div>

        <div className="hero-grid" aria-hidden="true">
          <div className="pixel-card accent-green">
            <span>LLM</span>
            <strong>{provider.kind === "openaiCompatible" ? "OPENAI" : "ANTHROPIC"}</strong>
          </div>
          <div className="pixel-card accent-gold">
            <span>Slides</span>
            <strong>{maxSlides}</strong>
          </div>
          <div className="pixel-card accent-red">
            <span>Docs</span>
            <strong>{documents.length}</strong>
          </div>
        </div>
      </section>

      <section className="workspace">
        <div className="left-column">
          <section className="panel">
            <div className="section-heading">
              <h2>Model Link</h2>
              <p>Swap between OpenAI-style and Anthropic-style APIs without changing the workflow.</p>
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
          </section>

          <section className="panel">
            <div className="section-heading">
              <h2>Briefing</h2>
              <p>Paste requirements, business context, or speaking notes. The deck can also be created from files alone.</p>
            </div>

            <div className="form-grid">
              <label>
                Audience
                <input
                  value={audience}
                  onChange={(event) => setAudience(event.target.value)}
                  placeholder="Executive team, clients, new hires..."
                />
              </label>
              <label>
                Desired Outcome
                <input
                  value={desiredOutcome}
                  onChange={(event) => setDesiredOutcome(event.target.value)}
                  placeholder="Approval, status update, sales pitch..."
                />
              </label>
              <label className="span-2">
                Deck File Name
                <input
                  value={deckName}
                  onChange={(event) => setDeckName(event.target.value)}
                  placeholder="board-update-q2"
                />
              </label>
              <label className="span-2">
                Prompt
                <textarea
                  value={briefing}
                  onChange={(event) => setBriefing(event.target.value)}
                  rows={8}
                  placeholder="Build an 8-slide investor update focused on product traction, risks, and next-quarter priorities..."
                />
              </label>
            </div>
          </section>
        </div>

        <div className="right-column">
          <section className="panel">
            <div className="section-heading">
              <h2>Source Material</h2>
              <p>Supports `.txt`, `.md`, `.pdf`, `.docx`, `.csv`, `.json`, `.yaml`, and `.toml`.</p>
            </div>

            <div className="toolbar">
              <button
                className="primary-button"
                onClick={() => void chooseDocuments()}
                type="button"
              >
                Load Documents
              </button>
              <button
                className="ghost-button"
                onClick={() => void chooseOutputPath()}
                type="button"
              >
                Pick Export Path
              </button>
            </div>

            <div className="status-row">
              <span>{documents.length} files</span>
              <span>{totalCharacters.toLocaleString()} chars</span>
            </div>

            <div className="document-list">
              {documents.length ? (
                documents.map((document) => (
                  <article className="document-card" key={document.path ?? document.name}>
                    <div>
                      <h3>{document.name}</h3>
                      <p>
                        {document.extension || "text"} · {document.characters.toLocaleString()} chars
                        {document.truncated ? " · truncated for prompt safety" : ""}
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

          <section className="panel">
            <div className="section-heading">
              <h2>Launch</h2>
              <p>{desktopRuntime ? "Ready for desktop generation." : "Preview mode only. Launch with Tauri for full functionality."}</p>
            </div>

            <div className="status-block">
              <p>{status}</p>
              <p className="path-label">{outputPath || "No export path selected yet."}</p>
            </div>

            {error ? <div className="error-banner">{error}</div> : null}

            <button
              className="primary-button large"
              disabled={isBusy}
              onClick={() => void generateDeck()}
              type="button"
            >
              {isBusy ? "Generating..." : "Generate PowerPoint"}
            </button>
          </section>
        </div>
      </section>

      <section className="panel preview-panel">
        <div className="section-heading">
          <h2>Deck Preview</h2>
          <p>Structured outline returned by the model and written into the exported `.pptx`.</p>
        </div>

        {result ? (
          <DeckPreview outline={result.outline} outputPath={result.outputPath} />
        ) : (
          <div className="empty-state preview-empty">
            <p>Generate a deck to inspect the slide outline and export path.</p>
          </div>
        )}
      </section>
    </main>
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
        <p className="eyebrow">Output</p>
        <h3>{outline.deckTitle}</h3>
        <p>{outline.subtitle}</p>
        <p className="theme-tag">{outline.themeTagline}</p>
        <p className="path-label">{outputPath}</p>
      </article>

      <div className="slide-grid">
        {outline.slides.map((slide, index) => (
          <article className="slide-card" key={`${slide.title}-${index}`}>
            <div className="slide-meta">
              <span>0{index + 1}</span>
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
