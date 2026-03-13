use std::{fs, path::Path};

use anyhow::Result;
use ppt_rs::generator::{SlideContent, SlideLayout, create_pptx_with_content};

use crate::models::{DeckOutline, DeckSlide, SlideLayoutHint};

const TITLE_COLOR: &str = "11243A";
const CONTENT_COLOR: &str = "39506C";
const ACCENT_COLOR: &str = "39B58A";
const ALERT_COLOR: &str = "F26A4B";

pub fn write_presentation(outline: &DeckOutline, output_path: &str) -> Result<()> {
    if let Some(parent) = Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let slides = outline
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| build_slide(outline, slide, index))
        .collect::<Vec<_>>();

    let pptx = create_pptx_with_content(&outline.deck_title, slides)?;
    fs::write(output_path, pptx)?;
    Ok(())
}

fn build_slide(outline: &DeckOutline, slide: &DeckSlide, index: usize) -> SlideContent {
    let layout = match slide.layout {
        SlideLayoutHint::Cover => SlideLayout::CenteredTitle,
        SlideLayoutHint::TwoColumn => SlideLayout::TwoColumn,
        SlideLayoutHint::Closing => SlideLayout::CenteredTitle,
        SlideLayoutHint::Standard => {
            if slide.bullets.len() > 5 {
                SlideLayout::TitleAndBigContent
            } else {
                SlideLayout::TitleAndContent
            }
        }
    };

    let title_size = if matches!(slide.layout, SlideLayoutHint::Cover) {
        52
    } else {
        38
    };

    let content_size = if matches!(
        slide.layout,
        SlideLayoutHint::Cover | SlideLayoutHint::Closing
    ) {
        22
    } else {
        24
    };

    let mut slide_content = SlideContent::new(&slide.title)
        .layout(layout)
        .title_size(title_size)
        .content_size(content_size)
        .title_bold(true)
        .title_color(TITLE_COLOR)
        .content_color(CONTENT_COLOR)
        .notes(&build_notes(outline, slide, index));

    let mut visible_bullets = slide.bullets.clone();
    if matches!(slide.layout, SlideLayoutHint::Cover) && visible_bullets.is_empty() {
        visible_bullets = vec![outline.subtitle.clone(), outline.theme_tagline.clone()];
    }

    if matches!(slide.layout, SlideLayoutHint::Closing) && visible_bullets.len() < 2 {
        visible_bullets.push("Questions, approval, or next action.".to_string());
    }

    for bullet in visible_bullets.iter().take(6) {
        slide_content = slide_content.add_bullet(bullet);
    }

    if !slide.highlight.trim().is_empty() && !matches!(slide.layout, SlideLayoutHint::Cover) {
        slide_content = slide_content
            .add_bullet(&format!("Signal: {}", slide.highlight))
            .content_color(ACCENT_COLOR);
    } else if matches!(slide.layout, SlideLayoutHint::Closing) {
        slide_content = slide_content.content_color(ALERT_COLOR);
    }

    slide_content
}

fn build_notes(outline: &DeckOutline, slide: &DeckSlide, index: usize) -> String {
    let mut sections = vec![format!(
        "Porchestrator slide {} of {}.",
        index + 1,
        outline.slides.len()
    )];

    if !slide.speaker_notes.trim().is_empty() {
        sections.push(slide.speaker_notes.trim().to_string());
    }

    if !slide.highlight.trim().is_empty() {
        sections.push(format!("Emphasis: {}", slide.highlight.trim()));
    }

    sections.join("\n\n")
}
