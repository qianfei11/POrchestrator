use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use quick_xml::{Reader, events::Event};
use zip::ZipArchive;

use crate::models::SourceDocument;

const MAX_DOCUMENT_CHARS: usize = 24_000;

pub fn ingest_documents(paths: Vec<String>) -> Result<Vec<SourceDocument>> {
    paths.into_iter().map(ingest_document).collect()
}

fn ingest_document(path: String) -> Result<SourceDocument> {
    let path_buf = PathBuf::from(&path);
    let file_name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .context("Unable to read the selected file name.")?;

    let extension = path_buf
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("txt")
        .to_ascii_lowercase();

    let raw_content = match extension.as_str() {
        "pdf" => pdf_extract::extract_text(&path_buf)
            .with_context(|| format!("Failed to extract text from PDF: {file_name}"))?,
        "docx" => extract_docx_text(&path_buf)
            .with_context(|| format!("Failed to extract text from DOCX: {file_name}"))?,
        _ => fs::read_to_string(&path_buf)
            .with_context(|| format!("Failed to read text file: {file_name}"))?,
    };

    let normalized = normalize_whitespace(&raw_content);
    if normalized.is_empty() {
        bail!("{file_name} did not contain any readable text.");
    }

    let characters = normalized.chars().count();
    let truncated = characters > MAX_DOCUMENT_CHARS;
    let content = truncate_chars(&normalized, MAX_DOCUMENT_CHARS);

    Ok(SourceDocument {
        name: file_name,
        path: Some(path),
        extension,
        content,
        characters,
        truncated,
    })
}

fn extract_docx_text(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .context("Missing word/document.xml in the DOCX archive.")?
        .read_to_string(&mut document_xml)?;

    let mut reader = Reader::from_str(&document_xml);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut extracted = String::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Text(text)) => {
                extracted.push_str(&text.xml_content()?.into_owned());
                extracted.push(' ');
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"w:p" => extracted.push('\n'),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }

        buffer.clear();
    }

    Ok(extracted)
}

fn normalize_whitespace(input: &str) -> String {
    let mut cleaned_lines = Vec::new();

    for line in input.lines() {
        let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !compact.is_empty() {
            cleaned_lines.push(compact);
        }
    }

    cleaned_lines.join("\n")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let truncated = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        format!("{truncated}\n\n[Truncated by Porchestrator for prompt safety]")
    } else {
        truncated
    }
}
