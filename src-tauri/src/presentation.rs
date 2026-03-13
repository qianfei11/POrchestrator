use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use ppt_rs::generator::{
    Image, SlideContent, SlideLayout, Shape, ShapeFill, ShapeGradientDirection, ShapeGradientFill,
    ShapeLine, ShapeType, create_pptx_with_content, generate_image_content_type,
    generate_image_relationship, generate_image_xml, inches_to_emu,
};
use ppt_rs::generator::images::{ImageEffect, ImageSource};
use zip::{ZipArchive, ZipWriter, write::FileOptions};

use crate::{
    images::GeneratedSlideImage,
    models::{DeckOutline, DeckSlide, SlideLayoutHint},
};

const TITLE_COLOR: &str = "11243A";
const CONTENT_COLOR: &str = "39506C";
const ACCENT_COLOR: &str = "2F7CF6";
const ALERT_COLOR: &str = "F26A4B";

const SLIDE_WIDTH: u32 = 9_144_000;
const SLIDE_HEIGHT: u32 = 6_858_000;

const COVER_IMAGE_X: u32 = 5_166_000;
const COVER_IMAGE_Y: u32 = 1_280_000;
const COVER_IMAGE_WIDTH: u32 = 3_300_000;
const COVER_IMAGE_HEIGHT: u32 = 4_520_000;

const VISUAL_IMAGE_X: u32 = 5_130_000;
const VISUAL_IMAGE_Y: u32 = 1_560_000;
const VISUAL_IMAGE_WIDTH: u32 = 3_360_000;
const VISUAL_IMAGE_HEIGHT: u32 = 3_720_000;

#[derive(Clone, Debug)]
struct EmbeddedImageSpec {
    slide_number: usize,
    image_number: usize,
    rel_id: usize,
    shape_id: usize,
    filename: String,
    local_filename: String,
    format: String,
    bytes: Vec<u8>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug)]
struct BackgroundDecoration {
    slide_number: usize,
    shapes: Vec<(u32, Shape)>,
}

#[derive(Clone, Debug)]
struct PackageEntry {
    name: String,
    data: Vec<u8>,
    is_dir: bool,
}

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

    let mut embedded_images = Vec::new();
    let mut decorations = Vec::new();
    let slides = outline
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| {
            let rendered_image = slide_images.get(index).and_then(Option::as_ref);
            let (slide_content, image_spec, background_decoration) =
                build_slide(outline, slide, index, rendered_image);

            if let Some(image_spec) = image_spec {
                embedded_images.push(image_spec);
            }
            decorations.push(background_decoration);

            slide_content
        })
        .collect::<Vec<_>>();

    let pptx = create_pptx_with_content(&outline.deck_title, slides)?;
    let pptx = inject_embedded_assets(pptx, &embedded_images, &decorations)?;
    fs::write(output_path, pptx)?;

    if !embedded_images.is_empty() {
        write_local_image_assets(output_path, &embedded_images)?;
    }

    Ok(())
}

fn build_slide(
    outline: &DeckOutline,
    slide: &DeckSlide,
    index: usize,
    rendered_image: Option<&GeneratedSlideImage>,
) -> (SlideContent, Option<EmbeddedImageSpec>, BackgroundDecoration) {
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

    if !slide.highlight.trim().is_empty()
        && !has_image
        && !matches!(slide.layout, SlideLayoutHint::Cover)
    {
        slide_content = slide_content
            .add_bullet(&format!("Signal: {}", slide.highlight))
            .content_color(ACCENT_COLOR);
    } else if matches!(slide.layout, SlideLayoutHint::Closing) {
        slide_content = slide_content.content_color(ALERT_COLOR);
    }

    let image_spec = rendered_image.map(|image| build_embedded_image_spec(index, slide, image));
    let decoration = build_background_decoration(index + 1, slide, image_spec.as_ref());

    (slide_content, image_spec, decoration)
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

fn build_embedded_image_spec(
    index: usize,
    slide: &DeckSlide,
    rendered: &GeneratedSlideImage,
) -> EmbeddedImageSpec {
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
        _ => (
            VISUAL_IMAGE_X,
            VISUAL_IMAGE_Y,
            VISUAL_IMAGE_WIDTH,
            VISUAL_IMAGE_HEIGHT,
        ),
    };

    let (width, height) = fit_image_within_box(
        rendered.width_px,
        rendered.height_px,
        box_width,
        box_height,
    );
    let x = box_x + (box_width.saturating_sub(width) / 2);
    let y = box_y + (box_height.saturating_sub(height) / 2);
    let format = normalize_format(&rendered.format);
    let extension = extension_for_format(&format);
    let slide_number = index + 1;
    let local_filename = format!(
        "slide-{slide_number:02}-{}.{}",
        slugify(&slide.title),
        extension
    );

    EmbeddedImageSpec {
        slide_number,
        image_number: slide_number,
        rel_id: 0,
        shape_id: 700 + slide_number,
        filename: format!("embedded-slide-{slide_number:02}.{extension}"),
        local_filename,
        format,
        bytes: rendered.bytes.clone(),
        x,
        y,
        width,
        height,
    }
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

fn build_background_decoration(
    slide_number: usize,
    slide: &DeckSlide,
    image: Option<&EmbeddedImageSpec>,
) -> BackgroundDecoration {
    let mut shapes = vec![
        (
            300 + slide_number as u32 * 10,
            Shape::new(ShapeType::Rectangle, 0, 0, SLIDE_WIDTH, SLIDE_HEIGHT).with_gradient(
                background_gradient(slide.layout.clone()),
            ),
        ),
        (
            301 + slide_number as u32 * 10,
            Shape::new(
                ShapeType::Rectangle,
                0,
                0,
                inches_to_emu(0.24),
                SLIDE_HEIGHT,
            )
            .with_fill(ShapeFill::new(ACCENT_COLOR).with_transparency(16)),
        ),
        (
            302 + slide_number as u32 * 10,
            Shape::new(
                ShapeType::Rectangle,
                0,
                SLIDE_HEIGHT - inches_to_emu(0.26),
                SLIDE_WIDTH,
                inches_to_emu(0.26),
            )
            .with_fill(ShapeFill::new("DCEBFF").with_transparency(12)),
        ),
    ];

    if let Some(image) = image {
        shapes.push((
            303 + slide_number as u32 * 10,
            Shape::new(
                ShapeType::Rectangle,
                image.x.saturating_sub(inches_to_emu(0.08)),
                image.y.saturating_sub(inches_to_emu(0.08)),
                image.width + inches_to_emu(0.16),
                image.height + inches_to_emu(0.16),
            )
            .with_gradient(ShapeGradientFill::three_color(
                "E6F0FF",
                "D5E5FF",
                "B7CCFF",
                ShapeGradientDirection::DiagonalDown,
            ))
            .with_line(ShapeLine::new("89A8E8", 16000)),
        ));
    }

    BackgroundDecoration {
        slide_number,
        shapes,
    }
}

fn background_gradient(layout: SlideLayoutHint) -> ShapeGradientFill {
    match layout {
        SlideLayoutHint::Cover => ShapeGradientFill::three_color(
            "F9FCFF",
            "EAF2FF",
            "DCEAFF",
            ShapeGradientDirection::DiagonalDown,
        ),
        SlideLayoutHint::Visual => ShapeGradientFill::three_color(
            "FFFFFF",
            "EDF4FF",
            "DAE9FF",
            ShapeGradientDirection::Horizontal,
        ),
        SlideLayoutHint::Closing => ShapeGradientFill::three_color(
            "FFF9F2",
            "FFF1E2",
            "FFE7D3",
            ShapeGradientDirection::Horizontal,
        ),
        _ => ShapeGradientFill::three_color(
            "FFFFFF",
            "F4F8FF",
            "EDF4FF",
            ShapeGradientDirection::Vertical,
        ),
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

fn inject_embedded_assets(
    package_bytes: Vec<u8>,
    embedded_images: &[EmbeddedImageSpec],
    decorations: &[BackgroundDecoration],
) -> Result<Vec<u8>> {
    if embedded_images.is_empty() && decorations.is_empty() {
        return Ok(package_bytes);
    }

    let mut entries = read_package_entries(package_bytes)?;
    let mut mutable_images = embedded_images.to_vec();
    assign_relationship_ids(&mut mutable_images, &entries)?;

    for entry in &mut entries {
        if entry.is_dir {
            continue;
        }

        if entry.name == "[Content_Types].xml" {
            let xml = String::from_utf8(entry.data.clone())
                .context("Could not read [Content_Types].xml as UTF-8.")?;
            entry.data = inject_content_types(&xml, &mutable_images).into_bytes();
            continue;
        }

        if let Some(slide_number) = slide_xml_number(&entry.name) {
            let xml = String::from_utf8(entry.data.clone())
                .with_context(|| format!("Could not read {} as UTF-8.", entry.name))?;
            let decoration = decorations
                .iter()
                .find(|item| item.slide_number == slide_number);
            entry.data = inject_slide_xml(&xml, slide_number, &mutable_images, decoration)
                .with_context(|| format!("Could not inject image XML into {}.", entry.name))?
                .into_bytes();
            continue;
        }

        if let Some(slide_number) = slide_rels_number(&entry.name) {
            let xml = String::from_utf8(entry.data.clone())
                .with_context(|| format!("Could not read {} as UTF-8.", entry.name))?;
            entry.data = inject_slide_relationships(&xml, slide_number, &mutable_images)
                .with_context(|| format!("Could not inject image relationships into {}.", entry.name))?
                .into_bytes();
        }
    }

    for image in &mutable_images {
        entries.push(PackageEntry {
            name: image.package_path(),
            data: image.bytes.clone(),
            is_dir: false,
        });
    }

    write_package_entries(entries)
}

fn read_package_entries(package_bytes: Vec<u8>) -> Result<Vec<PackageEntry>> {
    let cursor = Cursor::new(package_bytes);
    let mut archive = ZipArchive::new(cursor)?;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        let is_dir = file.is_dir();
        let mut data = Vec::new();
        if !is_dir {
            file.read_to_end(&mut data)?;
        }

        entries.push(PackageEntry { name, data, is_dir });
    }

    Ok(entries)
}

fn assign_relationship_ids(
    embedded_images: &mut [EmbeddedImageSpec],
    entries: &[PackageEntry],
) -> Result<()> {
    for image in embedded_images.iter_mut() {
        let rels_name = format!("ppt/slides/_rels/slide{}.xml.rels", image.slide_number);
        let rels_xml = entries
            .iter()
            .find(|entry| entry.name == rels_name)
            .ok_or_else(|| anyhow!("Missing relationship file for slide {}.", image.slide_number))?;
        let rels_text = String::from_utf8(rels_xml.data.clone())
            .with_context(|| format!("Could not read {rels_name} as UTF-8."))?;
        image.rel_id = next_relationship_id(&rels_text);
    }

    Ok(())
}

fn next_relationship_id(rels_xml: &str) -> usize {
    let mut max_id = 1_usize;
    let mut search_from = 0_usize;

    while let Some(found) = rels_xml[search_from..].find("Id=\"rId") {
        let digits_start = search_from + found + 7;
        let digits = rels_xml[digits_start..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if let Ok(value) = digits.parse::<usize>() {
            max_id = max_id.max(value + 1);
        }
        search_from = digits_start;
    }

    max_id
}

fn inject_content_types(content_types_xml: &str, embedded_images: &[EmbeddedImageSpec]) -> String {
    let mut updated = content_types_xml.to_string();

    for extension in embedded_images
        .iter()
        .map(|image| extension_for_format(&image.format))
        .collect::<Vec<_>>()
    {
        if !updated.contains(&format!("Extension=\"{extension}\"")) {
            updated = insert_before(
                &updated,
                "</Types>",
                &format!("\n{}", generate_image_content_type(extension)),
            );
        }
    }

    updated
}

fn inject_slide_xml(
    slide_xml: &str,
    slide_number: usize,
    embedded_images: &[EmbeddedImageSpec],
    decoration: Option<&BackgroundDecoration>,
) -> Result<String> {
    let mut updated = slide_xml.to_string();

    if let Some(decoration) = decoration {
        let decoration_xml = decoration
            .shapes
            .iter()
            .map(|(shape_id, shape)| ppt_rs::generator::generate_shape_xml(shape, *shape_id))
            .collect::<Vec<_>>()
            .join("\n");
        updated = insert_after(&updated, "</p:grpSpPr>", &format!("\n{decoration_xml}"))
            .ok_or_else(|| anyhow!("Could not find group shape transform block in slide XML."))?;
    }

    let images_xml = embedded_images
        .iter()
        .filter(|image| image.slide_number == slide_number)
        .map(EmbeddedImageSpec::to_xml)
        .collect::<Vec<_>>()
        .join("\n");

    if !images_xml.is_empty() {
        updated = insert_before(&updated, "</p:spTree>", &format!("\n{images_xml}"));
    }

    Ok(updated)
}

fn inject_slide_relationships(
    slide_rels_xml: &str,
    slide_number: usize,
    embedded_images: &[EmbeddedImageSpec],
) -> Result<String> {
    let relationships = embedded_images
        .iter()
        .filter(|image| image.slide_number == slide_number)
        .map(EmbeddedImageSpec::to_relationship_xml)
        .collect::<Vec<_>>()
        .join("\n");

    if relationships.is_empty() {
        return Ok(slide_rels_xml.to_string());
    }

    Ok(insert_before(
        slide_rels_xml,
        "</Relationships>",
        &format!("\n{relationships}"),
    ))
}

fn write_package_entries(entries: Vec<PackageEntry>) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options: FileOptions<'_, ()> = FileOptions::default();

    for entry in entries {
        if entry.is_dir {
            writer.add_directory(entry.name, options)?;
        } else {
            writer.start_file(entry.name, options)?;
            writer.write_all(&entry.data)?;
        }
    }

    Ok(writer.finish()?.into_inner())
}

fn write_local_image_assets(output_path: &str, embedded_images: &[EmbeddedImageSpec]) -> Result<()> {
    let output_path = Path::new(output_path);
    let assets_dir = assets_directory_for(output_path);
    fs::create_dir_all(&assets_dir)?;

    for image in embedded_images {
        fs::write(assets_dir.join(&image.local_filename), &image.bytes)?;
    }

    Ok(())
}

fn assets_directory_for(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("porchestrator-deck");

    output_path.with_file_name(format!("{stem}_assets"))
}

fn slide_xml_number(entry_name: &str) -> Option<usize> {
    entry_name
        .strip_prefix("ppt/slides/slide")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

fn slide_rels_number(entry_name: &str) -> Option<usize> {
    entry_name
        .strip_prefix("ppt/slides/_rels/slide")?
        .strip_suffix(".xml.rels")?
        .parse()
        .ok()
}

fn insert_before(source: &str, marker: &str, insertion: &str) -> String {
    if let Some(position) = source.rfind(marker) {
        let mut updated = source.to_string();
        updated.insert_str(position, insertion);
        updated
    } else {
        source.to_string()
    }
}

fn insert_after(source: &str, marker: &str, insertion: &str) -> Option<String> {
    let position = source.find(marker)?;
    let insertion_point = position + marker.len();
    let mut updated = source.to_string();
    updated.insert_str(insertion_point, insertion);
    Some(updated)
}

fn normalize_format(format: &str) -> String {
    match format.to_ascii_uppercase().as_str() {
        "JPG" => "JPEG".to_string(),
        other => other.to_string(),
    }
}

fn extension_for_format(format: &str) -> &'static str {
    match format {
        "JPEG" => "jpg",
        "PNG" => "png",
        "GIF" => "gif",
        _ => "png",
    }
}

fn slugify(input: &str) -> String {
    let slug = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();

    slug.trim_matches('-')
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

impl EmbeddedImageSpec {
    fn package_path(&self) -> String {
        format!("ppt/media/image{}.{}", self.image_number, extension_for_format(&self.format))
    }

    fn to_xml(&self) -> String {
        let image = Image {
            filename: self.filename.clone(),
            width: self.width,
            height: self.height,
            x: self.x,
            y: self.y,
            format: self.format.clone(),
            source: Some(ImageSource::Bytes(self.bytes.clone())),
            crop: None,
            effects: vec![ImageEffect::Shadow],
        };

        generate_image_xml(&image, self.shape_id, self.rel_id)
    }

    fn to_relationship_xml(&self) -> String {
        generate_image_relationship(self.rel_id, &format!("../media/image{}.{}", self.image_number, extension_for_format(&self.format)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ppt_rs::generator::SlideContent;

    const TINY_PNG_BASE64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn injects_embedded_images_into_package() {
        let base_package =
            create_pptx_with_content("Test Deck", vec![SlideContent::new("Slide 1")]).unwrap();
        let image_spec = EmbeddedImageSpec {
            slide_number: 1,
            image_number: 1,
            rel_id: 0,
            shape_id: 701,
            filename: "embedded-slide-01.png".to_string(),
            local_filename: "slide-01.png".to_string(),
            format: "PNG".to_string(),
            bytes: base64::engine::general_purpose::STANDARD
                .decode(TINY_PNG_BASE64)
                .unwrap(),
            x: 1_000_000,
            y: 1_000_000,
            width: 1_000_000,
            height: 1_000_000,
        };
        let decoration = BackgroundDecoration {
            slide_number: 1,
            shapes: vec![(
                310,
                Shape::new(ShapeType::Rectangle, 0, 0, SLIDE_WIDTH, SLIDE_HEIGHT)
                    .with_fill(ShapeFill::new("F4F8FF")),
            )],
        };

        let patched = inject_embedded_assets(base_package, &[image_spec], &[decoration]).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(patched)).unwrap();

        let mut slide_xml = String::new();
        archive
            .by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut slide_xml)
            .unwrap();
        assert!(slide_xml.contains("<p:pic>"));

        let mut rels_xml = String::new();
        archive
            .by_name("ppt/slides/_rels/slide1.xml.rels")
            .unwrap()
            .read_to_string(&mut rels_xml)
            .unwrap();
        assert!(rels_xml.contains("relationships/image"));

        let media = archive.by_name("ppt/media/image1.png");
        assert!(media.is_ok());
    }
}
