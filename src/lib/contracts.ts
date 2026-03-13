export type ProviderKind = "openaiCompatible" | "anthropicCompatible";

export interface ProviderSettings {
  kind: ProviderKind;
  baseUrl: string;
  model: string;
  apiKey: string;
  temperature: number;
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
  outputPath: string;
  documents: SourceDocument[];
}

export interface DeckSlide {
  title: string;
  layout: "cover" | "standard" | "twoColumn" | "closing";
  bullets: string[];
  speakerNotes: string;
  highlight: string;
}

export interface DeckOutline {
  deckTitle: string;
  subtitle: string;
  themeTagline: string;
  slides: DeckSlide[];
}

export interface GenerationResult {
  outputPath: string;
  deckTitle: string;
  subtitle: string;
  slideCount: number;
  outline: DeckOutline;
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
    model: "claude-3-5-sonnet-latest",
    apiKey: "",
    temperature: 0.4,
  },
};
