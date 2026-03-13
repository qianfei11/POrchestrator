use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};

use crate::models::{
    DeckOutline, DeckSlide, GeneratePresentationRequest, ProviderKind, SlideLayoutHint,
    SourceDocument,
};

const FALLBACK_SUBTITLE: &str = "Built by Porchestrator from the supplied materials.";
const FALLBACK_THEME: &str = "Clean signal, clear next step.";
const MIN_SLIDES: usize = 4;
const MAX_SLIDES: usize = 20;

pub async fn generate_outline(request: &GeneratePresentationRequest) -> Result<DeckOutline> {
    let client = Client::new();
    let target_slide_count = normalized_target_slide_count(request.max_slides);
    let system_prompt = build_system_prompt(target_slide_count, request.image_provider.enabled);
    let user_prompt = build_user_prompt(request, target_slide_count);

    let raw_response = match request.provider.kind {
        ProviderKind::OpenaiCompatible => {
            request_openai_style(&client, request, &system_prompt, &user_prompt).await?
        }
        ProviderKind::AnthropicCompatible => {
            request_anthropic_style(&client, request, &system_prompt, &user_prompt).await?
        }
    };

    let outline = parse_outline(&raw_response)?;
    Ok(normalize_outline(outline, request))
}

async fn request_openai_style(
    client: &Client,
    request: &GeneratePresentationRequest,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let endpoint = endpoint(&request.provider.base_url, "chat/completions");
    let response = client
        .post(endpoint)
        .header(
            AUTHORIZATION,
            format!("Bearer {}", request.provider.api_key),
        )
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "model": request.provider.model,
            "temperature": request.provider.temperature,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ]
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    extract_openai_content(&response)
        .context("The OpenAI-compatible response was missing text content.")
}

async fn request_anthropic_style(
    client: &Client,
    request: &GeneratePresentationRequest,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let endpoint = endpoint(&request.provider.base_url, "messages");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&request.provider.api_key)?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let response = client
        .post(endpoint)
        .headers(headers)
        .json(&json!({
            "model": request.provider.model,
            "temperature": request.provider.temperature,
            "max_tokens": 2400,
            "system": system_prompt,
            "messages": [{
                "role": "user",
                "content": user_prompt
            }]
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let content = response["content"]
        .as_array()
        .context("Anthropic response did not include a content array.")?;

    let text = content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                block.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.trim().is_empty() {
        bail!("Anthropic-compatible response contained no text blocks.");
    }

    Ok(text)
}

fn extract_openai_content(response: &Value) -> Option<String> {
    let message_content = &response["choices"][0]["message"]["content"];
    if let Some(content) = message_content.as_str() {
        return Some(content.to_string());
    }

    message_content.as_array().map(|parts| {
        parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn build_system_prompt(target_slide_count: usize, image_generation_enabled: bool) -> String {
    let image_instruction = if image_generation_enabled {
        "Generated images will be rendered for cover and visual slides. Make those image prompts vivid, cinematic, presentation-safe, and specific."
    } else {
        "Image generation can be added later, so still provide strong image prompts for cover and visual slides."
    };

    format!(
        "You are Porchestrator, an AI agent that turns raw input into presentation-ready slide plans. Return JSON only. Use this exact schema:
{{
  \"deckTitle\": string,
  \"subtitle\": string,
  \"themeTagline\": string,
  \"slides\": [
    {{
      \"title\": string,
      \"layout\": \"cover\" | \"standard\" | \"twoColumn\" | \"visual\" | \"closing\",
      \"bullets\": string[],
      \"speakerNotes\": string,
      \"highlight\": string,
      \"imagePrompt\": string,
      \"imageCaption\": string
    }}
  ]
}}

Rules:
- Return exactly {target_slide_count} slides, counting cover and closing.
- First slide must be \"cover\" and last slide must be \"closing\".
- Use 2 to 4 slides with layout \"visual\" when a strong supporting image would improve the story.
- Standard and twoColumn slides need 3 to 5 bullets, each 16 words or fewer.
- Visual slides need 2 to 4 bullets and should focus on one core message.
- Speaker notes should be 2 to 4 sentences and should mention document names when evidence comes from them.
- Use only provided information. If data is missing, say that explicitly instead of inventing details.
- Keep titles short and executive-friendly.
- imagePrompt should be 20 to 45 words, describe a vivid presentation-safe visual, and avoid logos, watermarks, or text inside the image.
- imageCaption should be 2 to 8 words.
- Leave imagePrompt blank only for the closing slide or when a visual would be misleading.
- {image_instruction}"
    )
}

fn build_user_prompt(request: &GeneratePresentationRequest, target_slide_count: usize) -> String {
    let visual_line = if request.image_provider.enabled {
        format!(
            "Image generation is enabled with model {} at {}.",
            request.image_provider.model, request.image_provider.base_url
        )
    } else {
        "Image generation is currently disabled, but image prompts are still required for later export."
            .to_string()
    };

    let mut sections = vec![
        format!("TARGET SLIDES\n{}\n", target_slide_count),
        format!("VISUAL OUTPUT\n{}\n", visual_line),
        format!("BRIEFING\n{}\n", request.briefing.trim()),
    ];

    if !request.audience.trim().is_empty() {
        sections.push(format!("AUDIENCE\n{}\n", request.audience.trim()));
    }

    if !request.desired_outcome.trim().is_empty() {
        sections.push(format!(
            "DESIRED OUTCOME\n{}\n",
            request.desired_outcome.trim()
        ));
    }

    if request.documents.is_empty() {
        sections.push("DOCUMENTS\nNo supporting files were uploaded.\n".to_string());
    } else {
        sections.push(format!(
            "DOCUMENTS\n{}\n",
            request
                .documents
                .iter()
                .map(render_document_for_prompt)
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }

    sections.join("\n")
}

fn render_document_for_prompt(document: &SourceDocument) -> String {
    format!(
        "FILE: {}\nEXTENSION: {}\nCHARACTERS: {}\nCONTENT:\n{}",
        document.name, document.extension, document.characters, document.content
    )
}

pub(crate) fn endpoint(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/{}", suffix.trim_start_matches('/'))
    }
}

fn parse_outline(raw_response: &str) -> Result<DeckOutline> {
    let candidate = strip_code_fences(raw_response);
    let json_payload = extract_json_object(&candidate)
        .ok_or_else(|| anyhow!("The model did not return a valid JSON object."))?;
    serde_json::from_str::<DeckOutline>(&json_payload)
        .with_context(|| format!("Failed to parse slide JSON from model output:\n{json_payload}"))
}

fn strip_code_fences(raw_response: &str) -> String {
    raw_response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

fn extract_json_object(input: &str) -> Option<String> {
    let mut start = None;
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => {
                if start.is_none() {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start_index) = start {
                        return Some(input[start_index..=index].to_string());
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn normalize_outline(
    mut outline: DeckOutline,
    request: &GeneratePresentationRequest,
) -> DeckOutline {
    if outline.deck_title.trim().is_empty() {
        outline.deck_title = guess_title(request);
    }

    if outline.subtitle.trim().is_empty() {
        outline.subtitle = FALLBACK_SUBTITLE.to_string();
    }

    if outline.theme_tagline.trim().is_empty() {
        outline.theme_tagline = FALLBACK_THEME.to_string();
    }

    let deck_title = outline.deck_title.clone();
    let subtitle = outline.subtitle.clone();
    let theme_tagline = outline.theme_tagline.clone();

    let mut slides = outline
        .slides
        .into_iter()
        .map(normalize_slide)
        .filter(|slide| !slide.title.is_empty())
        .collect::<Vec<_>>();

    if slides.is_empty() {
        slides = fallback_slides(request, &deck_title, &subtitle, &theme_tagline);
    }

    align_slide_count(
        &mut slides,
        request,
        &deck_title,
        &subtitle,
        &theme_tagline,
    );
    ensure_visual_slide_count(&mut slides, normalized_target_slide_count(request.max_slides));
    finalize_slide_boundaries(&mut slides, &deck_title, &subtitle, &theme_tagline);
    enrich_slides(&mut slides, request, &deck_title, &theme_tagline);

    outline.slides = slides;
    outline
}

fn normalized_target_slide_count(max_slides: u8) -> usize {
    usize::from(max_slides.clamp(MIN_SLIDES as u8, MAX_SLIDES as u8))
}

fn normalize_slide(mut slide: DeckSlide) -> DeckSlide {
    slide.title = slide.title.trim().to_string();
    slide.highlight = slide.highlight.trim().to_string();
    slide.speaker_notes = slide.speaker_notes.trim().to_string();
    slide.image_prompt = slide.image_prompt.trim().to_string();
    slide.image_caption = slide.image_caption.trim().to_string();
    slide.bullets = slide
        .bullets
        .into_iter()
        .map(|bullet| bullet.trim().to_string())
        .filter(|bullet| !bullet.is_empty())
        .take(5)
        .collect();

    if slide.speaker_notes.is_empty() {
        slide.speaker_notes =
            "Use this slide to explain the key point in more detail.".to_string();
    }

    slide
}

fn align_slide_count(
    slides: &mut Vec<DeckSlide>,
    request: &GeneratePresentationRequest,
    deck_title: &str,
    subtitle: &str,
    theme_tagline: &str,
) {
    let target_slide_count = normalized_target_slide_count(request.max_slides);
    slides.truncate(target_slide_count);

    while slides.len() < target_slide_count {
        let insert_at = slides.len().saturating_sub(1).max(1);
        let slide_number = insert_at + 1;
        slides.insert(
            insert_at,
            synthesized_slide(
                slide_number,
                request,
                deck_title,
                subtitle,
                theme_tagline,
            ),
        );
    }
}

fn ensure_visual_slide_count(slides: &mut [DeckSlide], target_slide_count: usize) {
    let desired_visuals = desired_visual_count(target_slide_count);
    let mut visual_count = slides
        .iter()
        .filter(|slide| matches!(slide.layout, SlideLayoutHint::Visual))
        .count();
    let middle_slide_count = slides.len().saturating_sub(2);

    if visual_count >= desired_visuals {
        return;
    }

    for slide in slides.iter_mut().skip(1).take(middle_slide_count) {
        if visual_count >= desired_visuals {
            break;
        }

        if matches!(slide.layout, SlideLayoutHint::Standard | SlideLayoutHint::TwoColumn) {
            slide.layout = SlideLayoutHint::Visual;
            slide.bullets.truncate(4);
            visual_count += 1;
        }
    }
}

fn desired_visual_count(target_slide_count: usize) -> usize {
    match target_slide_count {
        0..=5 => 1,
        6..=9 => 2,
        10..=14 => 3,
        _ => 4,
    }
}

fn finalize_slide_boundaries(
    slides: &mut [DeckSlide],
    deck_title: &str,
    subtitle: &str,
    theme_tagline: &str,
) {
    if let Some(first) = slides.first_mut() {
        first.layout = SlideLayoutHint::Cover;
        if first.title.is_empty() {
            first.title = deck_title.to_string();
        }
        if first.bullets.is_empty() {
            first.bullets = vec![subtitle.to_string(), theme_tagline.to_string()];
        }
    }

    if let Some(last) = slides.last_mut() {
        last.layout = SlideLayoutHint::Closing;
        if last.title.is_empty() {
            last.title = "Next Step".to_string();
        }
        if last.bullets.is_empty() {
            last.bullets = vec![
                "Recommended next step".to_string(),
                "Open discussion or approval prompt".to_string(),
            ];
        }
        last.image_prompt.clear();
        last.image_caption.clear();
    }

    let middle_slide_count = slides.len().saturating_sub(2);

    for slide in slides.iter_mut().skip(1).take(middle_slide_count) {
        if matches!(slide.layout, SlideLayoutHint::Cover | SlideLayoutHint::Closing) {
            slide.layout = SlideLayoutHint::Standard;
        }
    }
}

fn enrich_slides(
    slides: &mut [DeckSlide],
    request: &GeneratePresentationRequest,
    deck_title: &str,
    theme_tagline: &str,
) {
    let slide_count = slides.len();

    for (index, slide) in slides.iter_mut().enumerate() {
        if slide.highlight.is_empty() {
            slide.highlight = default_highlight(slide);
        }

        if slide.bullets.is_empty() && !matches!(slide.layout, SlideLayoutHint::Closing) {
            slide.bullets = default_bullets(index, request);
        }

        if matches!(slide.layout, SlideLayoutHint::Visual) {
            slide.bullets.truncate(4);
        }

        if matches!(slide.layout, SlideLayoutHint::Standard | SlideLayoutHint::TwoColumn) {
            slide.image_prompt.clear();
            slide.image_caption.clear();
        }

        if matches!(slide.layout, SlideLayoutHint::Cover | SlideLayoutHint::Visual)
            && slide.image_prompt.is_empty()
        {
            slide.image_prompt = default_image_prompt(
                slide,
                request,
                deck_title,
                theme_tagline,
                index == 0,
            );
        }

        if slide.image_caption.is_empty()
            && matches!(slide.layout, SlideLayoutHint::Cover | SlideLayoutHint::Visual)
        {
            slide.image_caption = default_image_caption(slide, slide_count, index);
        }
    }
}

fn default_highlight(slide: &DeckSlide) -> String {
    if let Some(first_bullet) = slide.bullets.first() {
        truncate_words(first_bullet, 8)
    } else {
        truncate_words(&slide.title, 6)
    }
}

fn default_bullets(index: usize, request: &GeneratePresentationRequest) -> Vec<String> {
    let source_line = request
        .documents
        .first()
        .map(|document| format!("Reference source: {}", document.name))
        .unwrap_or_else(|| "Reference source: working brief".to_string());

    vec![
        format!("Focus point {}", index + 1),
        source_line,
        "Call out the strongest evidence before drawing a conclusion.".to_string(),
    ]
}

fn default_image_prompt(
    slide: &DeckSlide,
    request: &GeneratePresentationRequest,
    deck_title: &str,
    theme_tagline: &str,
    is_cover: bool,
) -> String {
    let document_context = request
        .documents
        .iter()
        .take(2)
        .map(|document| document.name.as_str())
        .collect::<Vec<_>>()
        .join(" and ");

    let source_context = if document_context.is_empty() {
        if request.briefing.trim().is_empty() {
            "executive presentation context".to_string()
        } else {
            truncate_words(request.briefing.trim(), 16)
        }
    } else {
        format!("insights grounded in {}", document_context)
    };

    let subject = if is_cover {
        format!("hero visual for {}", deck_title)
    } else {
        format!("editorial illustration for {}", slide.title)
    };

    format!(
        "{subject}, {source_context}, theme of {theme_tagline}, vivid lighting, strong focal subject, clean business presentation aesthetic, depth and texture, no text, no watermark, 16:9 composition"
    )
}

fn default_image_caption(slide: &DeckSlide, slide_count: usize, index: usize) -> String {
    if index == 0 {
        "Executive overview".to_string()
    } else if index + 1 == slide_count {
        "Decision time".to_string()
    } else if !slide.highlight.is_empty() {
        truncate_words(&slide.highlight, 6)
    } else {
        truncate_words(&slide.title, 6)
    }
}

fn truncate_words(input: &str, max_words: usize) -> String {
    input.split_whitespace().take(max_words).collect::<Vec<_>>().join(" ")
}

fn guess_title(request: &GeneratePresentationRequest) -> String {
    if let Some(first_line) = request
        .briefing
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        let words = first_line.split_whitespace().take(8).collect::<Vec<_>>();
        return words.join(" ");
    }

    if let Some(first_document) = request.documents.first() {
        return format!("Deck from {}", first_document.name);
    }

    "Porchestrator Deck".to_string()
}

fn fallback_slides(
    request: &GeneratePresentationRequest,
    deck_title: &str,
    subtitle: &str,
    theme_tagline: &str,
) -> Vec<DeckSlide> {
    let top_sources = request
        .documents
        .iter()
        .take(3)
        .map(|document| document.name.clone())
        .collect::<Vec<_>>();

    let source_line = if top_sources.is_empty() {
        "No source files were attached.".to_string()
    } else {
        format!("Built from {}", top_sources.join(", "))
    };

    vec![
        DeckSlide {
            title: deck_title.to_string(),
            layout: SlideLayoutHint::Cover,
            bullets: vec![subtitle.to_string(), theme_tagline.to_string()],
            speaker_notes:
                "Introduce the narrative arc and explain what this deck is trying to accomplish."
                    .to_string(),
            highlight: source_line.clone(),
            image_prompt: format!(
                "hero illustration for {deck_title}, {source_line}, vivid lighting, confident executive mood, clean presentation visual, no text, no watermark, 16:9 framing"
            ),
            image_caption: "Executive overview".to_string(),
        },
        DeckSlide {
            title: "Situation".to_string(),
            layout: SlideLayoutHint::Standard,
            bullets: vec![
                source_line,
                "Use the pasted brief as the framing context.".to_string(),
                "Flag any data gaps before presenting conclusions.".to_string(),
            ],
            speaker_notes: "Summarize the business situation before moving into evidence."
                .to_string(),
            highlight: "Start with context, not detail.".to_string(),
            image_prompt: String::new(),
            image_caption: String::new(),
        },
        DeckSlide {
            title: "Evidence".to_string(),
            layout: SlideLayoutHint::Visual,
            bullets: request
                .documents
                .iter()
                .take(3)
                .map(|document| format!("Source loaded: {}", document.name))
                .collect(),
            speaker_notes: "Walk through what was available and what still needs validation."
                .to_string(),
            highlight: "Use only provided evidence.".to_string(),
            image_prompt:
                "data-rich editorial illustration showing evidence synthesis, document fragments, charts, and structured analysis, vivid contrast, no text, no watermark, 16:9 framing"
                    .to_string(),
            image_caption: "Evidence in focus".to_string(),
        },
        DeckSlide {
            title: "Next Step".to_string(),
            layout: SlideLayoutHint::Closing,
            bullets: vec![
                "Review the exported deck and tighten language.".to_string(),
                "Add more source documents for a sharper second pass.".to_string(),
            ],
            speaker_notes: "Close with a concrete action or decision request.".to_string(),
            highlight: "Refine, regenerate, present.".to_string(),
            image_prompt: String::new(),
            image_caption: String::new(),
        },
    ]
}

fn synthesized_slide(
    slide_number: usize,
    request: &GeneratePresentationRequest,
    deck_title: &str,
    subtitle: &str,
    theme_tagline: &str,
) -> DeckSlide {
    const TITLES: [&str; 8] = [
        "Key Signal",
        "What Changed",
        "Impact View",
        "Risk Lens",
        "Execution Focus",
        "Customer Angle",
        "Metric Snapshot",
        "Decision Support",
    ];
    const HIGHLIGHTS: [&str; 8] = [
        "Clarify the strongest takeaway.",
        "Show what moved since the last update.",
        "Translate evidence into consequences.",
        "Name the key constraint early.",
        "Tie the work to an operational next step.",
        "Explain the user-facing implication.",
        "Anchor the story in measurable evidence.",
        "Prepare the final decision prompt.",
    ];

    let template_index = (slide_number.saturating_sub(2)) % TITLES.len();
    let source_line = request
        .documents
        .first()
        .map(|document| format!("Pull supporting detail from {}", document.name))
        .unwrap_or_else(|| "Pull supporting detail from the working brief".to_string());
    let layout = if slide_number.is_multiple_of(3) {
        SlideLayoutHint::Visual
    } else {
        SlideLayoutHint::Standard
    };
    let title = TITLES[template_index].to_string();
    let highlight = HIGHLIGHTS[template_index].to_string();
    let image_prompt = if matches!(layout, SlideLayoutHint::Visual) {
        format!(
            "presentation visual for {deck_title}, slide titled {title}, {theme_tagline}, vivid contrast, focused subject, polished editorial style, no text, no watermark, 16:9 composition"
        )
    } else {
        String::new()
    };

    DeckSlide {
        title,
        layout,
        bullets: vec![
            truncate_words(subtitle, 10),
            source_line,
            "Keep the narrative tight and evidence-backed.".to_string(),
        ],
        speaker_notes:
            "Porchestrator inserted this slide to meet the requested slide count without inventing unsupported facts."
                .to_string(),
        highlight,
        image_prompt,
        image_caption: "Visual emphasis".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DeckOutline, ImageProviderSettings, ProviderSettings};

    fn request(max_slides: u8) -> GeneratePresentationRequest {
        GeneratePresentationRequest {
            provider: ProviderSettings {
                kind: ProviderKind::OpenaiCompatible,
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-4.1-mini".to_string(),
                api_key: "sk-test".to_string(),
                temperature: 0.4,
            },
            briefing: "Create a leadership update about launch readiness.".to_string(),
            audience: "Leadership".to_string(),
            desired_outcome: "Alignment".to_string(),
            max_slides,
            image_provider: ImageProviderSettings::default(),
            documents: vec![],
        }
    }

    #[test]
    fn normalizes_outline_to_exact_slide_budget() {
        let outline = DeckOutline {
            deck_title: "Launch Readiness".to_string(),
            subtitle: String::new(),
            theme_tagline: String::new(),
            slides: vec![
                DeckSlide {
                    title: "Launch Readiness".to_string(),
                    layout: SlideLayoutHint::Cover,
                    bullets: vec!["Status".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
                DeckSlide {
                    title: "Risks".to_string(),
                    layout: SlideLayoutHint::Standard,
                    bullets: vec!["Primary blocker".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
                DeckSlide {
                    title: "Next Step".to_string(),
                    layout: SlideLayoutHint::Closing,
                    bullets: vec!["Decision".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
            ],
        };

        let normalized = normalize_outline(outline, &request(10));

        assert_eq!(normalized.slides.len(), 10);
        assert!(matches!(normalized.slides.first().unwrap().layout, SlideLayoutHint::Cover));
        assert!(matches!(normalized.slides.last().unwrap().layout, SlideLayoutHint::Closing));
    }

    #[test]
    fn builds_visual_prompts_for_cover_and_visual_slides() {
        let outline = DeckOutline {
            deck_title: "Market Update".to_string(),
            subtitle: "Quarterly view".to_string(),
            theme_tagline: "Signal over noise".to_string(),
            slides: vec![
                DeckSlide {
                    title: "Market Update".to_string(),
                    layout: SlideLayoutHint::Cover,
                    bullets: vec![],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
                DeckSlide {
                    title: "Demand Trend".to_string(),
                    layout: SlideLayoutHint::Visual,
                    bullets: vec!["Trend".to_string(), "Implication".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
                DeckSlide {
                    title: "Next Step".to_string(),
                    layout: SlideLayoutHint::Closing,
                    bullets: vec!["Act".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: "should be cleared".to_string(),
                    image_caption: "caption".to_string(),
                },
                DeckSlide {
                    title: "Unused".to_string(),
                    layout: SlideLayoutHint::Standard,
                    bullets: vec!["Will be truncated".to_string()],
                    speaker_notes: String::new(),
                    highlight: String::new(),
                    image_prompt: String::new(),
                    image_caption: String::new(),
                },
            ],
        };

        let normalized = normalize_outline(outline, &request(4));

        assert!(!normalized.slides[0].image_prompt.is_empty());
        assert!(!normalized.slides[1].image_prompt.is_empty());
        assert!(normalized.slides[2].image_prompt.is_empty());
    }
}
