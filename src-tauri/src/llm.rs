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

pub async fn generate_outline(request: &GeneratePresentationRequest) -> Result<DeckOutline> {
    let client = Client::new();
    let system_prompt = build_system_prompt(request.max_slides);
    let user_prompt = build_user_prompt(request);

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
            "max_tokens": 1800,
            "system": system_prompt,
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": user_prompt }]
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

fn build_system_prompt(max_slides: u8) -> String {
    format!(
        "You are Porchestrator, an AI agent that turns raw input into presentation-ready slide plans. Return JSON only. Use this exact schema:
{{
  \"deckTitle\": string,
  \"subtitle\": string,
  \"themeTagline\": string,
  \"slides\": [
    {{
      \"title\": string,
      \"layout\": \"cover\" | \"standard\" | \"twoColumn\" | \"closing\",
      \"bullets\": string[],
      \"speakerNotes\": string,
      \"highlight\": string
    }}
  ]
}}

Rules:
- Use at most {max_slides} slides.
- First slide must be \"cover\" and last slide must be \"closing\".
- Standard slides need 3 to 5 bullets, each 16 words or fewer.
- Speaker notes should be 2 to 4 sentences and should mention document names when evidence comes from them.
- Use only provided information. If data is missing, say that explicitly instead of inventing details.
- Keep titles short and executive-friendly."
    )
}

fn build_user_prompt(request: &GeneratePresentationRequest) -> String {
    let mut sections = vec![format!("BRIEFING\n{}\n", request.briefing.trim())];

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

fn endpoint(base_url: &str, suffix: &str) -> String {
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

    outline.slides = outline
        .slides
        .into_iter()
        .map(normalize_slide)
        .filter(|slide| !slide.title.is_empty())
        .collect();

    if outline.slides.is_empty() {
        outline.slides = fallback_slides(request, &outline);
    }

    outline
        .slides
        .truncate(usize::from(request.max_slides.max(4)));

    if let Some(first) = outline.slides.first_mut() {
        first.layout = SlideLayoutHint::Cover;
        if first.bullets.is_empty() {
            first.bullets = vec![outline.subtitle.clone(), outline.theme_tagline.clone()];
        }
    }

    if let Some(last) = outline.slides.last_mut() {
        last.layout = SlideLayoutHint::Closing;
        if last.bullets.is_empty() {
            last.bullets = vec![
                "Recommended next step".to_string(),
                "Open discussion or approval prompt".to_string(),
            ];
        }
    }

    while outline.slides.len() < 4 {
        outline.slides.push(DeckSlide {
            title: format!("Supporting Detail {}", outline.slides.len()),
            layout: SlideLayoutHint::Standard,
            bullets: vec![
                "Source material was limited; refine with more evidence.".to_string(),
                "Use this slide for deeper explanation during rehearsal.".to_string(),
                "Replace with document-specific detail after the next run.".to_string(),
            ],
            speaker_notes:
                "Porchestrator inserted this placeholder because the model returned too few slides."
                    .to_string(),
            highlight: "Needs more source depth.".to_string(),
        });
    }

    outline
}

fn normalize_slide(mut slide: DeckSlide) -> DeckSlide {
    slide.title = slide.title.trim().to_string();
    slide.highlight = slide.highlight.trim().to_string();
    slide.speaker_notes = slide.speaker_notes.trim().to_string();
    slide.bullets = slide
        .bullets
        .into_iter()
        .map(|bullet| bullet.trim().to_string())
        .filter(|bullet| !bullet.is_empty())
        .take(6)
        .collect();

    if slide.speaker_notes.is_empty() {
        slide.speaker_notes = "Use this slide to explain the key point in more detail.".to_string();
    }

    slide
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

fn fallback_slides(request: &GeneratePresentationRequest, outline: &DeckOutline) -> Vec<DeckSlide> {
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
            title: outline.deck_title.clone(),
            layout: SlideLayoutHint::Cover,
            bullets: vec![outline.subtitle.clone(), outline.theme_tagline.clone()],
            speaker_notes:
                "Introduce the narrative arc and explain what this deck is trying to accomplish."
                    .to_string(),
            highlight: source_line.clone(),
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
        },
        DeckSlide {
            title: "Evidence".to_string(),
            layout: SlideLayoutHint::Standard,
            bullets: request
                .documents
                .iter()
                .take(3)
                .map(|document| format!("Source loaded: {}", document.name))
                .collect(),
            speaker_notes: "Walk through what was available and what still needs validation."
                .to_string(),
            highlight: "Use only provided evidence.".to_string(),
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
        },
    ]
}
