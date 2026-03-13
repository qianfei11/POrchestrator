use serde::{Deserialize, Serialize};

fn default_image_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_image_model() -> String {
    "gpt-image-1".to_string()
}

fn default_image_size() -> String {
    "1536x1024".to_string()
}

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
pub struct ImageProviderSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_image_base_url")]
    pub base_url: String,
    #[serde(default = "default_image_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_image_size")]
    pub size: String,
}

impl Default for ImageProviderSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_image_base_url(),
            model: default_image_model(),
            api_key: String::new(),
            size: default_image_size(),
        }
    }
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
    #[serde(default)]
    pub image_provider: ImageProviderSettings,
    #[serde(default)]
    pub documents: Vec<SourceDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResult {
    pub deck_title: String,
    pub subtitle: String,
    pub slide_count: usize,
    pub outline: DeckOutline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPresentationRequest {
    pub outline: DeckOutline,
    pub output_path: String,
    #[serde(default)]
    pub image_provider: ImageProviderSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub output_path: String,
    pub deck_title: String,
    pub slide_count: usize,
    #[serde(default)]
    pub generated_images: usize,
    #[serde(default)]
    pub warnings: Vec<String>,
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
    #[serde(default)]
    pub image_prompt: String,
    #[serde(default)]
    pub image_caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SlideLayoutHint {
    Cover,
    #[default]
    Standard,
    TwoColumn,
    Visual,
    Closing,
}
