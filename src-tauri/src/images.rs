use std::io::Cursor;

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine as _;
use image::GenericImageView;
use reqwest::{
    Client,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde_json::{Value, json};

use crate::{
    llm::endpoint,
    models::{DeckOutline, DeckSlide, ImageProviderSettings, SlideLayoutHint},
};

#[derive(Clone, Debug, Default)]
pub struct GeneratedSlideImage {
    pub bytes: Vec<u8>,
    pub width_px: u32,
    pub height_px: u32,
    pub format: String,
}

#[derive(Debug, Default)]
pub struct ImageGenerationSummary {
    pub generated: usize,
    pub images: Vec<Option<GeneratedSlideImage>>,
    pub warnings: Vec<String>,
}

enum ImagePayload {
    Base64(String),
    Url(String),
}

pub async fn generate_slide_images(
    outline: &DeckOutline,
    provider: &ImageProviderSettings,
) -> Result<ImageGenerationSummary> {
    let mut summary = ImageGenerationSummary {
        generated: 0,
        images: vec![None; outline.slides.len()],
        warnings: Vec::new(),
    };

    if !provider.enabled {
        return Ok(summary);
    }

    validate_provider(provider)?;

    let client = Client::new();

    for (index, slide) in outline.slides.iter().enumerate() {
        if !slide_wants_image(slide) {
            continue;
        }

        match generate_image_for_slide(&client, provider, outline, slide).await {
            Ok(image) => {
                summary.images[index] = Some(image);
                summary.generated += 1;
            }
            Err(error) => {
                summary
                    .warnings
                    .push(format!("{}: {}", slide.title.trim(), error));
            }
        }
    }

    Ok(summary)
}

fn validate_provider(provider: &ImageProviderSettings) -> Result<()> {
    ensure!(
        !provider.base_url.trim().is_empty(),
        "Image generation base URL is required."
    );
    ensure!(
        !provider.model.trim().is_empty(),
        "Image generation model is required."
    );
    ensure!(
        !provider.api_key.trim().is_empty(),
        "Image generation API key is required."
    );

    Ok(())
}

fn slide_wants_image(slide: &DeckSlide) -> bool {
    !slide.image_prompt.trim().is_empty()
        && matches!(
            slide.layout,
            SlideLayoutHint::Cover | SlideLayoutHint::Visual
        )
}

async fn generate_image_for_slide(
    client: &Client,
    provider: &ImageProviderSettings,
    outline: &DeckOutline,
    slide: &DeckSlide,
) -> Result<GeneratedSlideImage> {
    let prompt = build_image_prompt(outline, slide);
    let endpoint = endpoint(&provider.base_url, "images/generations");
    let response = client
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {}", provider.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "model": provider.model,
            "prompt": prompt,
            "size": provider.size,
            "n": 1
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let payload = parse_image_payload(&response)?;
    let raw_bytes = match payload {
        ImagePayload::Base64(data) => base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .context("The image provider returned invalid base64 data.")?,
        ImagePayload::Url(url) => client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec(),
    };

    normalize_image(&raw_bytes)
}

fn build_image_prompt(outline: &DeckOutline, slide: &DeckSlide) -> String {
    let caption = if slide.image_caption.trim().is_empty() {
        slide.title.trim()
    } else {
        slide.image_caption.trim()
    };

    format!(
        "{}

Deck context: {}. Slide title: {}. Caption intent: {}.
Requirements: vivid, high-detail, presentation-safe, strong focal subject, rich color contrast, polished editorial or cinematic style, no text, no watermark, no logos, 16:9 composition.",
        slide.image_prompt.trim(),
        outline.theme_tagline.trim(),
        slide.title.trim(),
        caption,
    )
}

fn parse_image_payload(response: &Value) -> Result<ImagePayload> {
    let entry = first_image_entry(response)?;

    for key in ["b64_json", "base64", "image_base64"] {
        if let Some(data) = entry.get(key).and_then(Value::as_str) {
            return Ok(ImagePayload::Base64(data.to_string()));
        }
    }

    for key in ["url", "image_url"] {
        if let Some(url) = entry.get(key).and_then(Value::as_str) {
            return Ok(ImagePayload::Url(url.to_string()));
        }
    }

    Err(anyhow!(
        "The image provider response did not include b64_json or url data."
    ))
}

fn first_image_entry(response: &Value) -> Result<&Value> {
    if let Some(data) = response.get("data").and_then(Value::as_array) {
        if let Some(first) = data.first() {
            return Ok(first);
        }
    }

    if let Some(result) = response.get("result") {
        return Ok(result);
    }

    bail!("The image provider response did not include a usable image payload.")
}

fn normalize_image(bytes: &[u8]) -> Result<GeneratedSlideImage> {
    let detected_format = image::guess_format(bytes)
        .context("Could not detect the generated image format.")?;
    let image = image::load_from_memory_with_format(bytes, detected_format)
        .context("Could not decode the generated image bytes.")?;
    let (width_px, height_px) = image.dimensions();
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .context("Could not normalize the generated image into PNG.")?;

    Ok(GeneratedSlideImage {
        bytes: output.into_inner(),
        width_px,
        height_px,
        format: "PNG".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_base64_payloads() {
        let payload = parse_image_payload(&json!({
            "data": [{ "b64_json": "abcd" }]
        }))
        .unwrap();

        match payload {
            ImagePayload::Base64(value) => assert_eq!(value, "abcd"),
            ImagePayload::Url(_) => panic!("expected base64 payload"),
        }
    }

    #[test]
    fn parses_url_payloads() {
        let payload = parse_image_payload(&json!({
            "data": [{ "url": "https://example.com/image.png" }]
        }))
        .unwrap();

        match payload {
            ImagePayload::Url(value) => assert_eq!(value, "https://example.com/image.png"),
            ImagePayload::Base64(_) => panic!("expected URL payload"),
        }
    }
}
