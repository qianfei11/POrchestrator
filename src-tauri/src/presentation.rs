use std::{fs, path::Path};

use anyhow::Result;
use ppt_rs::generator::{
    Image, SlideContent, SlideLayout, create_pptx_with_content,
};
use ppt_rs::generator::images::ImageEffect;

use crate::{
    images::GeneratedSlideImage,
    models::{DeckOutline, DeckSlide, SlideLayoutHint},
};

const TITLE_COLOR: &str = "11243A";
const CONTENT_COLOR: &str = "39506C";
const ACCENT_COLOR: &str = "2F7CF6";
const ALERT_COLOR: &str = "F26A4B";

const COVER_IMAGE_X: u32 = 5_166_000;
const COVER_IMAGE_Y: u32 = 1_280_000;
const COVER_IMAGE_WIDTH: u32 = 3_300_000;
const COVER_IMAGE_HEIGHT: u32 = 4_520_000;

const VISUAL_IMAGE_X: u32 = 5_130_000;
const VISUAL_IMAGE_Y: u32 = 1_560_000;
const VISUAL_IMAGE_WIDTH: u32 = 3_360_000;
const VISUAL_IMAGE_HEIGHT: u32 = 3_720_000;

pub fn write_presentation(
    outline: &DeckOutline,
    output_path: &str,
    slide_images: &[Option<GeneratedSlideImage>],
) -> Result<()> {
    if let Some(parent) = Path::new(output_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let slides = outline
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| {
            build_slide(
                outline,
                slide,
                index,
                slide_images.get(index).and_then(Option::as_ref),
            )
        })
        .collect::<Vec<_>>();

    let pptx = create_pptx_with_content(&outline.deck_title, slides)?;
    fs::write(output_path, pptx)?;
    Ok(())
}

fn build_slide(
    outline: &DeckOutline,
    slide: &DeckSlide,
    index: usize,
    rendered_image: Option<&GeneratedSlideImage>,
) -> SlideContent {
    let has_image = rendered_image.is_some();
    let layout = resolve_layout(slide, has_image);
    let title_size = resolve_title_size(slide, has_image);
    let content_size = resolve_content_size(slide, has_image);

    let mut slide_content = SlideContent::new(&slide.title)
        .layout(layout)
        .title_size(title_size)
        .content_size(content_size)
        .title_bold(true)
        .title_color(TITLE_COLOR)
        .content_color(CONTENT_COLOR)
        .notes(&build_notes(outline, slide, index));

    let mut visible_bullets = visible_bullets(outline, slide, has_image);

    if matches!(slide.layout, SlideLayoutHint::Closing) && visible_bullets.len() < 2 {
        visible_bullets.push("Questions, approval, or next action.".to_string());
    }

    for bullet in &visible_bullets {
        slide_content = slide_content.add_bullet(bullet);
    }

    if !slide.highlight.trim().is_empty() && !has_image && !matches!(slide.layout, SlideLayoutHint::Cover) {
        slide_content = slide_content
            .add_bullet(&format!("Signal: {}", slide.highlight))
            .content_color(ACCENT_COLOR);
    } else if matches!(slide.layout, SlideLayoutHint::Closing) {
        slide_content = slide_content.content_color(ALERT_COLOR);
    }

    if let Some(image) = rendered_image.and_then(|image| slide_image(image, slide)) {
        slide_content = slide_content.add_image(image);
    }

    slide_content
}

fn resolve_layout(slide: &DeckSlide, has_image: bool) -> SlideLayout {
    match slide.layout {
        SlideLayoutHint::Cover if has_image => SlideLayout::TitleAndContent,
        SlideLayoutHint::Cover => SlideLayout::CenteredTitle,
        SlideLayoutHint::TwoColumn => SlideLayout::TwoColumn,
        SlideLayoutHint::Visual => SlideLayout::TitleAndContent,
        SlideLayoutHint::Closing => SlideLayout::CenteredTitle,
        SlideLayoutHint::Standard => {
            if slide.bullets.len() > 5 {
                SlideLayout::TitleAndBigContent
            } else {
                SlideLayout::TitleAndContent
            }
        }
    }
}

fn resolve_title_size(slide: &DeckSlide, has_image: bool) -> u32 {
    match slide.layout {
        SlideLayoutHint::Cover if has_image => 40,
        SlideLayoutHint::Cover => 52,
        SlideLayoutHint::Visual => 34,
        _ => 38,
    }
}

fn resolve_content_size(slide: &DeckSlide, has_image: bool) -> u32 {
    match slide.layout {
        SlideLayoutHint::Cover if has_image => 22,
        SlideLayoutHint::Cover | SlideLayoutHint::Closing => 22,
        SlideLayoutHint::Visual if has_image => 20,
        SlideLayoutHint::Visual => 22,
        _ => 24,
    }
}

fn visible_bullets(outline: &DeckOutline, slide: &DeckSlide, has_image: bool) -> Vec<String> {
    let mut bullets = slide.bullets.clone();

    if matches!(slide.layout, SlideLayoutHint::Cover) && bullets.is_empty() {
        bullets = vec![outline.subtitle.clone(), outline.theme_tagline.clone()];
    }

    let max_bullets = match slide.layout {
        SlideLayoutHint::Cover => 2,
        SlideLayoutHint::Visual if has_image => 4,
        _ => 6,
    };

    bullets.into_iter().take(max_bullets).collect()
}

fn slide_image(rendered: &GeneratedSlideImage, slide: &DeckSlide) -> Option<Image> {
    let (box_x, box_y, box_width, box_height) = match slide.layout {
        SlideLayoutHint::Cover => (
            COVER_IMAGE_X,
            COVER_IMAGE_Y,
            COVER_IMAGE_WIDTH,
            COVER_IMAGE_HEIGHT,
        ),
        SlideLayoutHint::Visual => (
            VISUAL_IMAGE_X,
            VISUAL_IMAGE_Y,
            VISUAL_IMAGE_WIDTH,
            VISUAL_IMAGE_HEIGHT,
        ),
        _ => return None,
    };

    let (width, height) = fit_image_within_box(
        rendered.width_px,
        rendered.height_px,
        box_width,
        box_height,
    );
    let x = box_x + (box_width.saturating_sub(width) / 2);
    let y = box_y + (box_height.saturating_sub(height) / 2);

    Some(
        Image::from_bytes(rendered.bytes.clone(), width, height, &rendered.format)
            .position(x, y)
            .with_effect(ImageEffect::Shadow),
    )
}

fn fit_image_within_box(
    width_px: u32,
    height_px: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    if width_px == 0 || height_px == 0 {
        return (max_width, max_height);
    }

    let image_ratio = width_px as f64 / height_px as f64;
    let box_ratio = max_width as f64 / max_height as f64;

    if image_ratio >= box_ratio {
        let height = (max_width as f64 / image_ratio).round() as u32;
        (max_width, height.max(1))
    } else {
        let width = (max_height as f64 * image_ratio).round() as u32;
        (width.max(1), max_height)
    }
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

    if !slide.image_caption.trim().is_empty() {
        sections.push(format!("Visual cue: {}", slide.image_caption.trim()));
    }

    sections.join("\n\n")
}
