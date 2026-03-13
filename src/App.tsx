import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { startTransition, useState } from "react";
import "./App.css";
import {
  IMAGE_PROVIDER_PRESET,
  PROVIDER_PRESETS,
  type DeckOutline,
  type ExportPresentationRequest,
  type ExportResult,
  type GeneratePresentationRequest,
  type GenerationResult,
  type ImageProviderSettings,
  type ProviderKind,
  type ProviderSettings,
  type SourceDocument,
} from "./lib/contracts";

const desktopRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const MIN_SLIDE_BUDGET = 4;
const MAX_SLIDE_BUDGET = 20;

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

function buildImageProvider(): ImageProviderSettings {
  return { ...IMAGE_PROVIDER_PRESET };
}

function clampSlideBudget(input: number) {
  if (!Number.isFinite(input)) {
    return 8;
  }

  return Math.min(MAX_SLIDE_BUDGET, Math.max(MIN_SLIDE_BUDGET, Math.round(input)));
}

function providerSummary(kind: ProviderKind) {
  return kind === "openaiCompatible"
    ? "Uses /v1/chat/completions."
    : "Uses /v1/messages with the Anthropic version header.";
}

function imageProviderSummary(enabled: boolean) {
  return enabled
    ? "Uses /v1/images/generations for slide visuals."
    : "Disabled. Enable it to render vivid images into cover and visual slides.";
}

function App() {
  const [provider, setProvider] = useState<ProviderSettings>(
    buildProvider("openaiCompatible"),
  );
  const [imageProvider, setImageProvider] = useState<ImageProviderSettings>(
    buildImageProvider(),
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
  const [warning, setWarning] = useState("");
  const [isBusy, setIsBusy] = useState(false);
  const [isBriefModalOpen, setIsBriefModalOpen] = useState(false);
  const [isBriefSnapshotOpen, setIsBriefSnapshotOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  const totalCharacters = documents.reduce(
    (total, document) => total + document.characters,
    0,
  );
  const storedBriefFields = [audience, desiredOutcome, briefing].filter((value) =>
    value.trim(),
  ).length;
  const providerLabel =
    provider.kind === "openaiCompatible" ? "OpenAI style" : "Anthropic style";
  const settingsDigest = `${providerLabel} · ${provider.model} · ${maxSlides} slides · Visuals ${imageProvider.enabled ? imageProvider.model : "off"}`;
  const briefSnapshotDigest = storedBriefFields
    ? `${storedBriefFields} brief field${storedBriefFields === 1 ? "" : "s"} stored`
    : "No brief stored yet";

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

  function updateProvider<K extends keyof ProviderSettings>(
    key: K,
    value: ProviderSettings[K],
  ) {
    setProvider((current) => ({
      ...current,
      [key]: value,
    }));
  }

  function updateImageProvider<K extends keyof ImageProviderSettings>(
    key: K,
    value: ImageProviderSettings[K],
  ) {
    setImageProvider((current) => ({
      ...current,
      [key]: value,
    }));
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
      imageProvider,
    };

    setWarning("");
    setStatus(
      imageProvider.enabled
        ? `Generating visuals and writing ${outline.deckTitle}...`
        : `Writing ${outline.deckTitle}...`,
    );
    const exportResult = await invoke<ExportResult>("export_presentation", {
      request,
    });

    setOutputPath(exportResult.outputPath);
    setWarning(exportResult.warnings.join(" "));
    setStatus(
      `Deck ready: ${exportResult.deckTitle} with ${exportResult.slideCount} slides${exportResult.generatedImages ? ` and ${exportResult.generatedImages} generated visuals` : ""}.`,
    );
    return exportResult;
  }

  async function generateDeck() {
    setError("");
    setWarning("");
    setOutputPath("");

    if (!provider.apiKey.trim()) {
      setError("An API key is required.");
      return;
    }

    if (imageProvider.enabled && !imageProvider.apiKey.trim()) {
      setError("An image API key is required when image generation is enabled.");
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
      imageProvider,
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
        <header className="topbar topbar-compact panel">
          <div className="brand">
            <h1>Porchestrator</h1>
            <p className="summary">
              Desktop PowerPoint generation with a Rust backend and a cleaner, bounded workspace.
            </p>
          </div>

          <div className="status-pills" aria-hidden="true">
            <div className="status-pill">
              <span>Model</span>
              <strong>{provider.model}</strong>
            </div>
            <div className="status-pill">
              <span>Visuals</span>
              <strong>{imageProvider.enabled ? "On" : "Off"}</strong>
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

        <section className="panel settings-panel">
          <div className="section-heading compact inline-heading">
            <div>
              <h2>LLM Settings</h2>
              <p>{settingsDigest}</p>
            </div>
            <button
              className="ghost-button"
              onClick={() => setIsSettingsOpen((current) => !current)}
              type="button"
            >
              {isSettingsOpen ? "Hide Settings" : "Show Settings"}
            </button>
          </div>

          {isSettingsOpen ? (
            <div className="settings-body">
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
                    onChange={(event) => updateProvider("baseUrl", event.target.value)}
                    placeholder="https://api.openai.com/v1"
                  />
                </label>
                <label>
                  Model
                  <input
                    value={provider.model}
                    onChange={(event) => updateProvider("model", event.target.value)}
                    placeholder="gpt-4.1-mini"
                  />
                </label>
                <label className="span-2">
                  API Key
                  <input
                    type="password"
                    value={provider.apiKey}
                    onChange={(event) => updateProvider("apiKey", event.target.value)}
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
                      updateProvider("temperature", Number(event.target.value))
                    }
                  />
                </label>
                <label>
                  Slide Budget
                  <input
                    type="number"
                    min={MIN_SLIDE_BUDGET}
                    max={MAX_SLIDE_BUDGET}
                    value={maxSlides}
                    onChange={(event) =>
                      setMaxSlides(clampSlideBudget(Number(event.target.value)))
                    }
                  />
                </label>
              </div>

              <div className="api-note">
                <strong>Defaults verified:</strong> {providerSummary(provider.kind)}
              </div>

              <div className="subsection">
                <div className="section-heading compact subsection-heading">
                  <h2>Image Generation</h2>
                  <p>{imageProviderSummary(imageProvider.enabled)}</p>
                </div>

                <div className="segmented-control">
                  <button
                    className={!imageProvider.enabled ? "active" : ""}
                    onClick={() => updateImageProvider("enabled", false)}
                    type="button"
                  >
                    Visuals Off
                  </button>
                  <button
                    className={imageProvider.enabled ? "active" : ""}
                    onClick={() => updateImageProvider("enabled", true)}
                    type="button"
                  >
                    Visuals On
                  </button>
                </div>

                <div className="form-grid">
                  <label>
                    Image Base URL
                    <input
                      disabled={!imageProvider.enabled}
                      value={imageProvider.baseUrl}
                      onChange={(event) =>
                        updateImageProvider("baseUrl", event.target.value)
                      }
                      placeholder="https://api.openai.com/v1"
                    />
                  </label>
                  <label>
                    Image Model
                    <input
                      disabled={!imageProvider.enabled}
                      value={imageProvider.model}
                      onChange={(event) =>
                        updateImageProvider("model", event.target.value)
                      }
                      placeholder="gpt-image-1 or nano-banana-2"
                    />
                  </label>
                  <label className="span-2">
                    Image API Key
                    <input
                      disabled={!imageProvider.enabled}
                      type="password"
                      value={imageProvider.apiKey}
                      onChange={(event) =>
                        updateImageProvider("apiKey", event.target.value)
                      }
                      placeholder="sk-..."
                    />
                  </label>
                  <label>
                    Image Size
                    <input
                      disabled={!imageProvider.enabled}
                      value={imageProvider.size}
                      onChange={(event) =>
                        updateImageProvider("size", event.target.value)
                      }
                      placeholder="1536x1024"
                    />
                  </label>
                </div>

                <div className="api-note">
                  <strong>Image API:</strong> OpenAI-compatible `images/generations`
                  with models such as `gpt-image-1` or compatible deployments
                  like `nano-banana-2`.
                </div>
              </div>
            </div>
          ) : null}
        </section>

        <section className="workspace workspace-main">
          <section className="panel source-panel">
            <div className="section-heading compact">
              <h2>Source Material</h2>
              <p>
                Readable text is extracted before the model call and trimmed to
                stay inside prompt bounds.
              </p>
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

            <div className="subsection brief-subsection">
              <div className="section-heading compact inline-heading subsection-heading">
                <div>
                  <h2>Brief Snapshot</h2>
                  <p>{briefSnapshotDigest}. The editor still opens only when you click Generate Deck.</p>
                </div>
                <button
                  className="ghost-button section-toggle"
                  onClick={() => setIsBriefSnapshotOpen((current) => !current)}
                  type="button"
                >
                  {isBriefSnapshotOpen ? "Hide Snapshot" : "Show Snapshot"}
                </button>
              </div>

              {isBriefSnapshotOpen ? (
                <div className="brief-summary-grid">
                  <article className="brief-summary-card">
                    <span className="brief-label">Audience</span>
                    <strong>{audience.trim() || "Not set"}</strong>
                  </article>
                  <article className="brief-summary-card">
                    <span className="brief-label">Outcome</span>
                    <strong>{desiredOutcome.trim() || "Not set"}</strong>
                  </article>
                  <article className="brief-summary-card span-2">
                    <span className="brief-label">Prompt</span>
                    <strong>
                      {briefing.trim()
                        ? `${briefing.trim().slice(0, 180)}${briefing.trim().length > 180 ? "..." : ""}`
                        : "No prompt stored yet."}
                    </strong>
                  </article>
                </div>
              ) : null}
            </div>
          </section>

          <section className="panel control-panel">
            <div className="section-heading compact">
              <h2>Run</h2>
              <p>
                {desktopRuntime
                  ? `Briefing is collected only when you click Generate Deck, and the exported deck targets exactly ${maxSlides} slides.`
                  : "Preview mode only. Launch through Tauri for desktop generation."}
              </p>
            </div>

            <div className="control-grid">
              <div className="status-block">
                <p>{status}</p>
                <p className="path-label">{outputPath || "No deck exported yet."}</p>
              </div>

              <div className="quick-stats">
                <article className="brief-summary-card">
                  <span className="brief-label">Provider</span>
                  <strong>{providerLabel}</strong>
                </article>
                <article className="brief-summary-card">
                  <span className="brief-label">Image Model</span>
                  <strong>{imageProvider.enabled ? imageProvider.model : "Disabled"}</strong>
                </article>
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
            {warning ? <div className="warning-banner">{warning}</div> : null}
          </section>
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
                  placeholder={`Summarize the uploaded material into a concise ${maxSlides}-slide product update for leadership...`}
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
            {slide.imagePrompt ? (
              <p className="slide-visual">
                Visual: {slide.imageCaption || "Generated image"}.
              </p>
            ) : null}
          </article>
        ))}
      </div>
    </div>
  );
}

export default App;
