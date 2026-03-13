use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    pub extension: String,
    pub content: String,
    pub characters: usize,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    OpenaiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePresentationRequest {
    pub provider: ProviderSettings,
    #[serde(default)]
    pub briefing: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default)]
    pub desired_outcome: String,
    pub max_slides: u8,
    pub output_path: String,
    #[serde(default)]
    pub documents: Vec<SourceDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResult {
    pub output_path: String,
    pub deck_title: String,
    pub subtitle: String,
    pub slide_count: usize,
    pub outline: DeckOutline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckOutline {
    pub deck_title: String,
    pub subtitle: String,
    pub theme_tagline: String,
    pub slides: Vec<DeckSlide>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSlide {
    pub title: String,
    #[serde(default)]
    pub layout: SlideLayoutHint,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub speaker_notes: String,
    #[serde(default)]
    pub highlight: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SlideLayoutHint {
    Cover,
    #[default]
    Standard,
    TwoColumn,
    Closing,
}
