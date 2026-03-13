export type ProviderKind = "openaiCompatible" | "anthropicCompatible";

export interface ProviderSettings {
  kind: ProviderKind;
  baseUrl: string;
  model: string;
  apiKey: string;
  temperature: number;
}

export interface ImageProviderSettings {
  enabled: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
  size: string;
}

export interface SourceDocument {
  name: string;
  path?: string | null;
  extension: string;
  content: string;
  characters: number;
  truncated: boolean;
}

export interface GeneratePresentationRequest {
  provider: ProviderSettings;
  briefing: string;
  audience: string;
  desiredOutcome: string;
  maxSlides: number;
  imageProvider: ImageProviderSettings;
  documents: SourceDocument[];
}

export interface ExportPresentationRequest {
  outline: DeckOutline;
  outputPath: string;
  imageProvider: ImageProviderSettings;
}

export interface DeckSlide {
  title: string;
  layout: "cover" | "standard" | "twoColumn" | "visual" | "closing";
  bullets: string[];
  speakerNotes: string;
  highlight: string;
  imagePrompt: string;
  imageCaption: string;
}

export interface DeckOutline {
  deckTitle: string;
  subtitle: string;
  themeTagline: string;
  slides: DeckSlide[];
}

export interface GenerationResult {
  deckTitle: string;
  subtitle: string;
  slideCount: number;
  outline: DeckOutline;
}

export interface ExportResult {
  outputPath: string;
  deckTitle: string;
  slideCount: number;
  generatedImages: number;
  warnings: string[];
}

export const PROVIDER_PRESETS: Record<ProviderKind, ProviderSettings> = {
  openaiCompatible: {
    kind: "openaiCompatible",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4.1-mini",
    apiKey: "",
    temperature: 0.5,
  },
  anthropicCompatible: {
    kind: "anthropicCompatible",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-20250514",
    apiKey: "",
    temperature: 0.4,
  },
};

export const IMAGE_PROVIDER_PRESET: ImageProviderSettings = {
  enabled: false,
  baseUrl: "https://api.openai.com/v1",
  model: "gpt-image-1",
  apiKey: "",
  size: "1536x1024",
};
