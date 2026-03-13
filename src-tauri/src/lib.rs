mod documents;
mod llm;
mod models;
mod presentation;

use models::{
    ExportPresentationRequest, ExportResult, GeneratePresentationRequest, GenerationResult,
    SourceDocument,
};

fn format_error(error: anyhow::Error) -> String {
    let mut chain = error.chain();
    let mut parts = Vec::new();

    while let Some(entry) = chain.next() {
        parts.push(entry.to_string());
    }

    parts.join(": ")
}

#[tauri::command]
async fn ingest_documents(paths: Vec<String>) -> Result<Vec<SourceDocument>, String> {
    documents::ingest_documents(paths).map_err(format_error)
}

#[tauri::command]
async fn generate_outline(
    request: GeneratePresentationRequest,
) -> Result<GenerationResult, String> {
    let outline = llm::generate_outline(&request)
        .await
        .map_err(format_error)?;

    Ok(GenerationResult {
        deck_title: outline.deck_title.clone(),
        subtitle: outline.subtitle.clone(),
        slide_count: outline.slides.len(),
        outline,
    })
}

#[tauri::command]
async fn export_presentation(request: ExportPresentationRequest) -> Result<ExportResult, String> {
    presentation::write_presentation(&request.outline, &request.output_path)
        .map_err(format_error)?;

    Ok(ExportResult {
        output_path: request.output_path,
        deck_title: request.outline.deck_title,
        slide_count: request.outline.slides.len(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            ingest_documents,
            generate_outline,
            export_presentation
        ])
        .run(tauri::generate_context!())
        .expect("error while running Porchestrator");
}
