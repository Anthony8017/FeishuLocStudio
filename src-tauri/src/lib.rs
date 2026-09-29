mod reliability;
mod writeback;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::{Client, StatusCode};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    feishu_app_id: String,
    feishu_app_secret: String,
    #[serde(default = "default_ai_provider")]
    ai_provider: String,
    openai_api_key: String,
    openai_base_url: String,
    openai_model: String,
    #[serde(default = "default_ui_language")]
    ui_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Column {
    id: String,
    name: String,
    sample: String,
    sheet_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    row_id: String,
    row_number: usize,
    sheet_id: String,
    sheet_name: String,
    available_column_ids: Vec<String>,
    values: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SheetPreview {
    spreadsheet_id: String,
    sheet_ids: Vec<String>,
    sheet_names: Vec<String>,
    columns: Vec<Column>,
    rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadSheetResponse {
    preview: SheetPreview,
    read_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectAiProviderResponse {
    ai_provider: String,
    base_url: String,
    suggested_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvailableModelsResponse {
    ai_provider: String,
    base_url: String,
    models: Vec<String>,
    selected_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunHistoryEntry {
    id: String,
    timestamp: String,
    mode: String,
    processed_rows: usize,
    failed_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationSummaryState {
    changed_cells: usize,
    processed_rows: usize,
    skipped_rows: usize,
    failed_rows: Vec<String>,
    #[serde(default)]
    failed_row_ids: Vec<String>,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowStateSnapshot {
    sheet_link: String,
    locked_columns: Vec<String>,
    target_columns: Vec<String>,
    source_column_id: String,
    write_mode: String,
    glossary: Vec<GlossaryTerm>,
    prompt: String,
    glossary_sheet: String,
    glossary_column_widths: HashMap<String, u32>,
    #[serde(default)]
    glossary_local_only_mode: bool,
    #[serde(default)]
    sheet_state: Option<SheetPreview>,
    #[serde(default)]
    read_success: bool,
    #[serde(default)]
    summary: Option<TranslationSummaryState>,
    #[serde(default)]
    history: Vec<RunHistoryEntry>,
    #[serde(default)]
    collapsed_panels: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceBundle {
    version: String,
    app_config: AppConfig,
    workflow: WorkflowStateSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceImportResponse {
    app_config: AppConfig,
    workflow: WorkflowStateSnapshot,
    path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiUsageSnapshot {
    request_count: usize,
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
    provider: String,
    model: String,
    last_updated_at: String,
}

static AI_USAGE: OnceLock<Mutex<AiUsageSnapshot>> = OnceLock::new();
static CANCELLED_TASKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

const LLM_STREAM_EVENT: &str = "llm-stream";
const GLOSSARY_PATCH_EVENT: &str = "glossary-patch";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LlmStreamEvent {
    task: String,
    phase: String,
    message: String,
    delta: String,
    debug_prompt: String,
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
    current: usize,
    total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryPatchEvent {
    task: String,
    message: String,
    terms: Vec<GlossaryTerm>,
    rows: Vec<Row>,
    removed_glossary_ids: Vec<String>,
}

#[derive(Clone)]
struct StreamReporter {
    app: AppHandle,
    task: String,
}

impl StreamReporter {
    fn new(app: &AppHandle, task: &str) -> Self {
        Self {
            app: app.clone(),
            task: task.to_string(),
        }
    }

    fn emit(
        &self,
        phase: &str,
        message: impl Into<String>,
        delta: impl Into<String>,
        current: usize,
        total: usize,
    ) {
        let payload = LlmStreamEvent {
            task: self.task.clone(),
            phase: phase.to_string(),
            message: message.into(),
            delta: delta.into(),
            debug_prompt: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            current,
            total,
        };
        let _ = self.app.emit(LLM_STREAM_EVENT, payload);
    }

    fn emit_debug(
        &self,
        message: impl Into<String>,
        debug_prompt: impl Into<String>,
        current: usize,
        total: usize,
    ) {
        let payload = LlmStreamEvent {
            task: self.task.clone(),
            phase: "debug".to_string(),
            message: message.into(),
            delta: String::new(),
            debug_prompt: debug_prompt.into(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            current,
            total,
        };
        let _ = self.app.emit(LLM_STREAM_EVENT, payload);
    }

    fn emit_usage(
        &self,
        prompt_tokens: usize,
        completion_tokens: usize,
        total_tokens: usize,
        current: usize,
        total: usize,
    ) {
        let payload = LlmStreamEvent {
            task: self.task.clone(),
            phase: "usage".to_string(),
            message: String::new(),
            delta: String::new(),
            debug_prompt: String::new(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            current,
            total,
        };
        let _ = self.app.emit(LLM_STREAM_EVENT, payload);
    }

    fn start(&self, message: impl Into<String>, total: usize) {
        self.emit("start", message, "", 0, total);
    }

    fn status(&self, message: impl Into<String>, current: usize, total: usize) {
        self.emit("status", message, "", current, total);
    }

    fn delta(&self, delta: &str, current: usize, total: usize) {
        if !delta.is_empty() {
            self.emit("delta", "", delta, current, total);
        }
    }

    fn complete(&self, message: impl Into<String>, current: usize, total: usize) {
        self.emit("complete", message, "", current, total);
    }

    fn error(&self, message: impl Into<String>, current: usize, total: usize) {
        self.emit("error", message, "", current, total);
    }

    fn glossary_patch(
        &self,
        message: impl Into<String>,
        terms: Vec<GlossaryTerm>,
        rows: Vec<Row>,
        removed_glossary_ids: Vec<String>,
    ) {
        let payload = GlossaryPatchEvent {
            task: self.task.clone(),
            message: message.into(),
            terms,
            rows,
            removed_glossary_ids,
        };
        let _ = self.app.emit(GLOSSARY_PATCH_EVENT, payload);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryTerm {
    #[serde(default)]
    glossary_id: String,
    term: String,
    category: String,
    note: String,
    enabled: bool,
    #[serde(default)]
    source_sheet: String,
    #[serde(default)]
    row_id: String,
    #[serde(default)]
    row_number: usize,
    #[serde(default)]
    sheet_id: String,
    translations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryRequest {
    rows: Vec<Row>,
    source_column_id: String,
    target_columns: Vec<Column>,
    #[serde(default)]
    prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryResponse {
    terms: Vec<GlossaryTerm>,
    #[serde(default)]
    rows: Vec<Row>,
    #[serde(default)]
    live_sync_denied: bool,
    #[serde(default)]
    sync_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillGlossaryRequest {
    spreadsheet_id: String,
    columns: Vec<Column>,
    rows: Vec<Row>,
    source_column_id: String,
    glossary: Vec<GlossaryTerm>,
    target_columns: Vec<Column>,
    #[serde(default)]
    prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossarySyncRequest {
    spreadsheet_id: String,
    columns: Vec<Column>,
    rows: Vec<Row>,
    source_column_id: String,
    target_column_ids: Vec<String>,
    glossary_term: GlossaryTerm,
    #[serde(default)]
    remove: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossarySyncResponse {
    term: Option<GlossaryTerm>,
    row: Option<Row>,
    removed: bool,
    #[serde(default)]
    permission_denied: bool,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryWritebackRequest {
    spreadsheet_id: String,
    columns: Vec<Column>,
    rows: Vec<Row>,
    source_column_id: String,
    target_column_ids: Vec<String>,
    glossary: Vec<GlossaryTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptSuggestionRequest {
    source_column_name: String,
    source_column_id: String,
    rows: Vec<Row>,
    target_columns: Vec<Column>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptSuggestionResponse {
    prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopTaskResponse {
    stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationRequest {
    spreadsheet_id: String,
    columns: Vec<Column>,
    rows: Vec<Row>,
    locked_column_ids: Vec<String>,
    target_column_ids: Vec<String>,
    source_column_id: String,
    glossary: Vec<GlossaryTerm>,
    prompt: String,
    write_mode: String,
    #[serde(default)]
    translation_token_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationSummary {
    changed_cells: usize,
    processed_rows: usize,
    skipped_rows: usize,
    failed_rows: Vec<String>,
    #[serde(default)]
    failed_row_ids: Vec<String>,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationResponse {
    rows: Vec<Row>,
    summary: TranslationSummary,
}

#[derive(Debug, Clone)]
struct MultiTargetBatchItem {
    row_id: String,
    source: String,
    missing_targets: Vec<TargetColumnInfo>,
    existing_translations: HashMap<String, String>,
    auxiliary_context: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct TargetColumnInfo {
    id: String,
    name: String,
}

#[derive(Debug, Clone)]
struct SheetInfo {
    sheet_id: String,
    sheet_name: String,
}

#[derive(Debug, Clone)]
struct ColumnAccumulator {
    name: String,
    sample: String,
    sheet_ids: HashSet<String>,
}

fn default_ui_language() -> String {
    "zh-CN".into()
}

fn cancel_task(task: &str) {
    let cancelled = CANCELLED_TASKS.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut tasks) = cancelled.lock() {
        tasks.insert(task.to_string());
    }
}

fn clear_cancelled_task(task: &str) {
    let cancelled = CANCELLED_TASKS.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut tasks) = cancelled.lock() {
        tasks.remove(task);
    }
}

fn is_task_cancelled(task: &str) -> bool {
    let cancelled = CANCELLED_TASKS.get_or_init(|| Mutex::new(HashSet::new()));
    cancelled
        .lock()
        .map(|tasks| tasks.contains(task))
        .unwrap_or(false)
}

fn ensure_task_not_cancelled(task: &str) -> Result<(), String> {
    if is_task_cancelled(task) {
        return Err("TRANSLATION_CANCELLED".to_string());
    }
    Ok(())
}

fn compact_multilingual_glossary_payload(
    glossary: &[GlossaryTerm],
    target_columns: &[TargetColumnInfo],
) -> String {
    let target_ids = target_columns
        .iter()
        .map(|column| column.id.as_str())
        .collect::<HashSet<_>>();
    glossary
        .iter()
        .filter(|term| term.enabled)
        .filter(|term| !term.term.trim().is_empty())
        .filter_map(|term| {
            let translations = target_columns
                .iter()
                .filter_map(|column| {
                    term.translations
                        .get(&column.id)
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(|value| format!("{}({})={}", column.name, column.id, value))
                })
                .collect::<Vec<_>>();
            if translations.is_empty()
                && !term
                    .translations
                    .keys()
                    .any(|key| target_ids.contains(key.as_str()))
            {
                return None;
            }
            Some(format!(
                "{} [{}{}] => {}",
                term.term,
                term.category,
                if term.note.trim().is_empty() {
                    String::new()
                } else {
                    format!("; {}", term.note.trim())
                },
                if translations.is_empty() {
                    "None".to_string()
                } else {
                    translations.join(", ")
                }
            ))
        })
        .take(120)
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_runtime_translation_prompt_multi(
    prompt: &str,
    source_column_name: &str,
    target_columns: &[TargetColumnInfo],
) -> String {
    let target_list = target_columns
        .iter()
        .map(|column| format!("{}({})", column.name, column.id))
        .collect::<Vec<_>>()
        .join(", ");
    let base = format!(
        "Translate from {} into the requested target columns for a game localization sheet. Fill only the target column ids listed in missingTargets for each row. Keep terminology consistent across all requested target languages. Preserve placeholders, rich text tags, sprite tags, escaped newlines, real line breaks, numbers, and symbols exactly. Do not explain or add notes. Keep short UI text concise.\n\nTarget columns:\n{}",
        source_column_name,
        target_list
    );
    if prompt.trim().is_empty() {
        return base;
    }
    format!("{}\n\nProject guidance:\n{}", base, prompt.trim())
}

fn write_mode_uses_existing_translations(write_mode: &str) -> bool {
    write_mode == "fill_empty"
}

fn translation_chunk_size(target_count: usize) -> usize {
    match target_count {
        0 | 1 => 32,
        2 => 24,
        3 => 18,
        4 => 14,
        _ => 10,
    }
}

fn estimate_token_count(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed.chars().count().div_ceil(3).max(1)
}

fn estimate_translation_item_tokens(
    item: &MultiTargetBatchItem,
    include_existing_translations: bool,
) -> usize {
    let source_tokens = estimate_token_count(&item.source);
    let context_tokens = item
        .auxiliary_context
        .iter()
        .map(|(key, value)| estimate_token_count(key) + estimate_token_count(value))
        .sum::<usize>();
    let existing_tokens = if include_existing_translations {
        item.existing_translations
            .iter()
            .map(|(key, value)| estimate_token_count(key) + estimate_token_count(value))
            .sum::<usize>()
    } else {
        0
    };
    let target_tokens = item
        .missing_targets
        .iter()
        .map(|column| estimate_token_count(&column.id) + estimate_token_count(&column.name) + 12)
        .sum::<usize>();
    source_tokens + context_tokens + existing_tokens + target_tokens + 24
}

fn resolve_translation_token_budget(
    requested_budget: usize,
    items: &[MultiTargetBatchItem],
    prompt: &str,
    glossary_text: &str,
    target_count: usize,
    include_existing_translations: bool,
) -> usize {
    if requested_budget > 0 {
        return requested_budget.max(2000);
    }
    let baseline_rows = translation_chunk_size(target_count).max(1);
    let target_rows = baseline_rows.saturating_mul(5).max(1);
    let sample_count = items.len().min(24);
    let average_item_tokens = if sample_count == 0 {
        80
    } else {
        items
            .iter()
            .take(sample_count)
            .map(|item| estimate_translation_item_tokens(item, include_existing_translations))
            .sum::<usize>()
            .div_ceil(sample_count)
    };
    let base_tokens = estimate_token_count(prompt)
        + estimate_token_count(glossary_text)
        + (target_count * 28)
        + 240;
    (base_tokens + average_item_tokens.saturating_mul(target_rows)).clamp(12000, 240000)
}

fn chunk_multi_target_translation_items<'a>(
    items: &'a [MultiTargetBatchItem],
    prompt: &str,
    glossary_text: &str,
    target_count: usize,
    include_existing_translations: bool,
    token_budget: usize,
) -> Vec<&'a [MultiTargetBatchItem]> {
    if items.is_empty() {
        return Vec::new();
    }
    let base_tokens = estimate_token_count(prompt)
        + estimate_token_count(glossary_text)
        + (target_count * 28)
        + 240;
    let max_rows = translation_chunk_size(target_count)
        .saturating_mul(5)
        .max(1);
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < items.len() {
        let mut estimated_tokens = base_tokens;
        let mut end = start;
        while end < items.len() && end.saturating_sub(start) < max_rows {
            let next_tokens =
                estimate_translation_item_tokens(&items[end], include_existing_translations);
            if end > start && estimated_tokens.saturating_add(next_tokens) > token_budget {
                break;
            }
            estimated_tokens = estimated_tokens.saturating_add(next_tokens);
            end += 1;
        }
        if end == start {
            end = (start + 1).min(items.len());
        }
        chunks.push(&items[start..end]);
        start = end;
    }
    chunks
}

fn trimmed_context_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut result = trimmed.chars().take(240).collect::<String>();
    if trimmed.chars().count() > 240 {
        result.push_str("...");
    }
    Some(result)
}

fn is_feishu_permission_denied(error: &str) -> bool {
    error.contains("HTTP 403")
        || error.contains("\"code\":91403")
        || error.contains("code:91403")
        || error.contains("Forbidden")
}

fn make_glossary_id(seed: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let sanitized = seed
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("glossary-{}-{}", millis, sanitized.trim_matches('-'))
}

fn build_preferred_row_id(
    row_values: &HashMap<String, String>,
    sheet_id: &str,
    row_number: usize,
    used_row_ids: &mut HashSet<String>,
) -> String {
    let fallback = format!("{}:{}", sheet_id, row_number);
    let key_value = row_values
        .get("key")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    if let Some(key_value) = key_value {
        let direct = key_value.to_string();
        if used_row_ids.insert(direct.clone()) {
            return direct;
        }
        let scoped = format!("{}:{}", sheet_id, key_value);
        if used_row_ids.insert(scoped.clone()) {
            return scoped;
        }
    }
    used_row_ids.insert(fallback.clone());
    fallback
}

fn ensure_glossary_identity(term: &mut GlossaryTerm) {
    if term.glossary_id.trim().is_empty() {
        let seed = if !term.row_id.trim().is_empty() {
            term.row_id.clone()
        } else if !term.term.trim().is_empty() {
            term.term.clone()
        } else {
            "term".to_string()
        };
        term.glossary_id = make_glossary_id(&seed);
    }
}

fn bind_glossary_term_to_row(term: &mut GlossaryTerm, row: &Row) {
    term.row_id = row.row_id.clone();
    term.row_number = row.row_number;
    term.sheet_id = row.sheet_id.clone();
    term.source_sheet = row.sheet_name.clone();
}

fn is_blank_glossary_term(term: &GlossaryTerm, target_column_ids: &[String]) -> bool {
    if !term.term.trim().is_empty() {
        return false;
    }
    target_column_ids.iter().all(|column_id| {
        term.translations
            .get(column_id)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    })
}

fn resolve_glossary_row(term: &GlossaryTerm, rows: &[Row], source_column_id: &str) -> Option<Row> {
    if !term.row_id.trim().is_empty() {
        if let Some(row) = rows.iter().find(|row| row.row_id == term.row_id) {
            return Some(row.clone());
        }
    }
    if term.term.trim().is_empty() {
        return None;
    }
    let normalized_term = term.term.trim();
    let candidates = rows
        .iter()
        .filter(|row| term.source_sheet.trim().is_empty() || row.sheet_name == term.source_sheet)
        .filter(|row| {
            row.values
                .get(source_column_id)
                .map(|value| value.trim() == normalized_term)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    None
}

fn build_glossary_row_template(
    term: &GlossaryTerm,
    rows: &[Row],
    source_column_id: &str,
    target_column_ids: &[String],
) -> Result<Row, String> {
    let sheet_name = if !term.source_sheet.trim().is_empty() {
        term.source_sheet.clone()
    } else {
        rows.first()
            .map(|row| row.sheet_name.clone())
            .ok_or_else(|| "No sheet rows available to create glossary entry.".to_string())?
    };
    let mut same_sheet_rows = rows
        .iter()
        .filter(|row| row.sheet_name == sheet_name)
        .cloned()
        .collect::<Vec<_>>();
    if same_sheet_rows.is_empty() {
        return Err(format!(
            "Unable to resolve sheet for glossary term: {}",
            sheet_name
        ));
    }
    same_sheet_rows.sort_by_key(|row| row.row_number);
    let template = same_sheet_rows
        .first()
        .cloned()
        .ok_or_else(|| format!("Missing sheet template for {}", sheet_name))?;
    let next_row_number = same_sheet_rows
        .iter()
        .map(|row| row.row_number)
        .max()
        .unwrap_or(1)
        + 1;
    let mut available_column_ids = template.available_column_ids.clone();
    if !available_column_ids
        .iter()
        .any(|value| value == source_column_id)
    {
        available_column_ids.push(source_column_id.to_string());
    }
    for column_id in target_column_ids {
        if !available_column_ids.iter().any(|value| value == column_id) {
            available_column_ids.push(column_id.clone());
        }
    }
    Ok(Row {
        row_id: format!("{}:{}", template.sheet_id, next_row_number),
        row_number: next_row_number,
        sheet_id: template.sheet_id,
        sheet_name: template.sheet_name,
        available_column_ids,
        values: HashMap::new(),
    })
}

fn find_optional_column_id(columns: &[Column], aliases: &[&str]) -> Option<String> {
    columns
        .iter()
        .find(|column| {
            aliases.iter().any(|alias| {
                column.name.eq_ignore_ascii_case(alias) || column.id.eq_ignore_ascii_case(alias)
            })
        })
        .map(|column| column.id.clone())
}

fn apply_glossary_term_to_row(
    term: &GlossaryTerm,
    row: &mut Row,
    source_column_id: &str,
    target_column_ids: &[String],
    category_column_id: Option<&str>,
    note_column_id: Option<&str>,
) {
    row.values
        .insert(source_column_id.to_string(), term.term.clone());
    for column_id in target_column_ids {
        row.values.insert(
            column_id.clone(),
            term.translations
                .get(column_id)
                .cloned()
                .unwrap_or_default(),
        );
    }
    if let Some(column_id) = category_column_id {
        row.values
            .insert(column_id.to_string(), term.category.clone());
    }
    if let Some(column_id) = note_column_id {
        row.values.insert(column_id.to_string(), term.note.clone());
    }
}

fn default_ai_provider() -> String {
    "openai".into()
}

fn default_project_config() -> AppConfig {
    AppConfig {
        feishu_app_id: String::new(),
        feishu_app_secret: String::new(),
        ai_provider: "packy".into(),
        openai_api_key: String::new(),
        openai_base_url: "https://www.packyapi.ai/v1".into(),
        openai_model: "gemini-3.8-flash".into(),
        ui_language: default_ui_language(),
    }
}

#[tauri::command]
fn load_local_config() -> Result<AppConfig, String> {
    let path = local_config_path()?;
    if !path.exists() {
        return Ok(default_project_config());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read local config: {}", error))?;
    let mut config: AppConfig = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse local config: {}", error))?;
    if config.ai_provider == "openai"
        && reqwest::Url::parse(&config.openai_base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .as_deref()
            == Some("www.packyapi.ai")
    {
        config.ai_provider = "packy".into();
    }
    Ok(config)
}

#[tauri::command]
fn save_local_config(config: AppConfig) -> Result<(), String> {
    let path = local_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create config directory: {}", error))?;
    }
    let text = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("Failed to serialize config: {}", error))?;
    fs::write(path, text).map_err(|error| format!("Failed to write local config: {}", error))
}

#[tauri::command]
fn load_local_workflow() -> Result<Option<WorkflowStateSnapshot>, String> {
    let path = local_workflow_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read local workflow: {}", error))?;
    let workflow = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse local workflow: {}", error))?;
    Ok(Some(workflow))
}

#[tauri::command]
fn save_local_workflow(workflow: WorkflowStateSnapshot) -> Result<(), String> {
    let path = local_workflow_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create workflow directory: {}", error))?;
    }
    let text = serde_json::to_string_pretty(&workflow)
        .map_err(|error| format!("Failed to serialize local workflow: {}", error))?;
    fs::write(path, text).map_err(|error| format!("Failed to write local workflow: {}", error))
}

#[tauri::command]
fn export_workspace(
    app_config: AppConfig,
    workflow: WorkflowStateSnapshot,
) -> Result<String, String> {
    let file = FileDialog::new()
        .set_title("Export Feishu Loc Workspace")
        .add_filter("Feishu Loc Workspace", &["json"])
        .set_file_name("feishu-loc-workspace.json")
        .save_file()
        .ok_or_else(|| "WORKSPACE_DIALOG_CANCELLED".to_string())?;
    let bundle = WorkspaceBundle {
        version: "0.1.1".into(),
        app_config,
        workflow,
    };
    let text = serde_json::to_string_pretty(&bundle)
        .map_err(|error| format!("Failed to serialize workspace: {}", error))?;
    fs::write(&file, text).map_err(|error| format!("Failed to write workspace file: {}", error))?;
    Ok(file.display().to_string())
}

#[tauri::command]
fn import_workspace() -> Result<WorkspaceImportResponse, String> {
    let file = FileDialog::new()
        .set_title("Import Feishu Loc Workspace")
        .add_filter("Feishu Loc Workspace", &["json"])
        .pick_file()
        .ok_or_else(|| "WORKSPACE_DIALOG_CANCELLED".to_string())?;
    let text = fs::read_to_string(&file)
        .map_err(|error| format!("Failed to read workspace file: {}", error))?;
    let bundle: WorkspaceBundle = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse workspace file: {}", error))?;
    Ok(WorkspaceImportResponse {
        app_config: bundle.app_config,
        workflow: bundle.workflow,
        path: file.display().to_string(),
    })
}

#[tauri::command]
async fn detect_ai_provider(config: AppConfig) -> Result<DetectAiProviderResponse, String> {
    if config.openai_api_key.trim().is_empty() {
        return Err("Missing AI API key.".into());
    }

    let client = build_http_client()?;
    let current_provider = normalize_ai_provider(&config.ai_provider);
    let current_base_url = config.openai_base_url.trim().to_string();
    let mut candidates = Vec::<(String, String)>::new();

    let current_base_url = if current_base_url.is_empty() {
        default_ai_base_url(&current_provider).to_string()
    } else {
        current_base_url
    };
    candidates.push((
        current_provider.clone(),
        normalize_base_url_for_provider(&current_provider, &current_base_url),
    ));

    let mut errors = Vec::new();
    for (provider, base_url) in candidates {
        match probe_ai_provider(&client, &config, &provider, &base_url).await {
            Ok(model) => {
                return Ok(DetectAiProviderResponse {
                    ai_provider: provider,
                    base_url,
                    suggested_model: model,
                })
            }
            Err(error) => errors.push(error),
        }
    }

    Err(format!(
        "Unable to detect AI provider. {}",
        errors.join(" | ")
    ))
}

#[tauri::command]
async fn fetch_available_models(config: AppConfig) -> Result<AvailableModelsResponse, String> {
    if config.openai_api_key.trim().is_empty() {
        return Err("Missing AI API key.".into());
    }
    let client = build_http_client()?;
    let provider = normalize_ai_provider(&config.ai_provider);
    let base_url = if config.openai_base_url.trim().is_empty() {
        default_ai_base_url(&provider).to_string()
    } else {
        config.openai_base_url.trim().to_string()
    };
    let base_url = normalize_base_url_for_provider(&provider, &base_url);
    let models = fetch_provider_models(&client, &config, &provider, &base_url).await?;
    let model_refs = models.iter().map(String::as_str).collect::<Vec<_>>();
    let selected_model = pick_preferred_model(
        config.openai_model.trim(),
        &model_refs,
        preferred_models_for_provider(&provider),
    );
    Ok(AvailableModelsResponse {
        ai_provider: provider,
        base_url,
        models,
        selected_model,
    })
}

#[tauri::command]
fn get_ai_usage() -> Result<AiUsageSnapshot, String> {
    let usage = AI_USAGE.get_or_init(|| Mutex::new(AiUsageSnapshot::default()));
    usage
        .lock()
        .map(|snapshot| snapshot.clone())
        .map_err(|error| format!("Failed to read AI usage: {}", error))
}

#[tauri::command]
async fn load_sheet_preview(link: String, config: AppConfig) -> Result<LoadSheetResponse, String> {
    ensure_feishu_config(&config)?;
    let spreadsheet_id = parse_spreadsheet_id(&link)?;
    let client = build_http_client()?;
    let tenant_token = fetch_feishu_tenant_token(&client, &config).await?;
    let metainfo = fetch_feishu_metainfo(&client, &tenant_token, &spreadsheet_id).await?;
    let sheets = parse_sheet_infos(&metainfo)?;
    if sheets.is_empty() {
        return Err("No child sheets found.".into());
    }

    let mut rows = Vec::new();
    let mut column_map: BTreeMap<String, ColumnAccumulator> = BTreeMap::new();
    let mut used_row_ids = HashSet::new();

    for sheet in &sheets {
        let values =
            fetch_sheet_values(&client, &tenant_token, &spreadsheet_id, &sheet.sheet_id).await?;
        if values.is_empty() {
            continue;
        }

        let headers = values.first().cloned().unwrap_or_default();
        let header_defs = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                let id = normalize_column_id(header, index);
                let name = if header.trim().is_empty() {
                    format!("Column{}", index + 1)
                } else {
                    header.trim().to_string()
                };
                (index, id, name)
            })
            .collect::<Vec<_>>();

        let sample_row = values.get(1).cloned().unwrap_or_default();
        for (index, column_id, column_name) in &header_defs {
            let entry = column_map
                .entry(column_id.clone())
                .or_insert_with(|| ColumnAccumulator {
                    name: column_name.clone(),
                    sample: sample_row.get(*index).cloned().unwrap_or_default(),
                    sheet_ids: HashSet::new(),
                });
            if entry.sample.is_empty() {
                entry.sample = sample_row.get(*index).cloned().unwrap_or_default();
            }
            entry.sheet_ids.insert(sheet.sheet_id.clone());
        }

        for (index, raw_row) in values.iter().enumerate().skip(1) {
            let mut row_values = HashMap::new();
            let mut available_column_ids = Vec::new();
            for (column_index, column_id, _) in &header_defs {
                available_column_ids.push(column_id.clone());
                row_values.insert(
                    column_id.clone(),
                    raw_row.get(*column_index).cloned().unwrap_or_default(),
                );
            }

            rows.push(Row {
                row_id: build_preferred_row_id(
                    &row_values,
                    &sheet.sheet_id,
                    index + 1,
                    &mut used_row_ids,
                ),
                row_number: index + 1,
                sheet_id: sheet.sheet_id.clone(),
                sheet_name: sheet.sheet_name.clone(),
                available_column_ids,
                values: row_values,
            });
        }
    }

    let columns = column_map
        .into_iter()
        .map(|(id, accumulator)| Column {
            id,
            name: accumulator.name,
            sample: accumulator.sample,
            sheet_count: accumulator.sheet_ids.len(),
        })
        .collect::<Vec<_>>();

    Ok(LoadSheetResponse {
        preview: SheetPreview {
            spreadsheet_id,
            sheet_ids: sheets.iter().map(|sheet| sheet.sheet_id.clone()).collect(),
            sheet_names: sheets
                .iter()
                .map(|sheet| sheet.sheet_name.clone())
                .collect(),
            columns,
            rows,
        },
        read_success: true,
    })
}

#[tauri::command]
async fn generate_glossary(
    app: AppHandle,
    request: GlossaryRequest,
    config: AppConfig,
) -> Result<GlossaryResponse, String> {
    ensure_ai_config(&config)?;
    let client = build_http_client()?;
    let reporter = StreamReporter::new(&app, "glossary");
    let source_texts = request
        .rows
        .iter()
        .filter_map(|row| row.values.get(&request.source_column_id))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .take(120)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if source_texts.is_empty() {
        reporter.error("The selected source column is empty.", 0, 1);
        return Err("The selected source column is empty.".into());
    }
    reporter.start(
        format!(
            "Analyzing {} source texts and extracting glossary terms.",
            source_texts.len()
        ),
        1,
    );

    let translation_properties = request
        .target_columns
        .iter()
        .map(|column| (column.id.clone(), json!({ "type": "string" })))
        .collect::<serde_json::Map<String, Value>>();
    let required_keys = request
        .target_columns
        .iter()
        .map(|column| Value::String(column.id.clone()))
        .collect::<Vec<_>>();

    let messages = vec![
        json!({"role":"system","content":"You are a game localization glossary assistant. Extract high-value reusable terms from the source language texts. Focus on proper nouns, UI labels, system terms, weapon names, skill names, and world-specific nouns."}),
        json!({"role":"user","content":format!(
            "Target columns: {}\nPrompt reference: {}\n\nGenerate up to 18 glossary entries from these texts.\n\n{}",
            request.target_columns.iter().map(|column| format!("{}({})", column.name, column.id)).collect::<Vec<_>>().join(", "),
            if request.prompt.trim().is_empty() { "None" } else { request.prompt.trim() },
            source_texts.iter().enumerate().map(|(index, text)| format!("{}. {}", index + 1, text)).collect::<Vec<_>>().join("\n")
        )}),
    ];

    let schema = json!({
        "type":"object",
        "properties":{"terms":{"type":"array","items":{"type":"object","properties":{
            "term":{"type":"string"},
            "category":{"type":"string"},
            "note":{"type":"string"},
            "translations":{"type":"object","properties":translation_properties,"required":required_keys,"additionalProperties":false}
        },"required":["term","category","note","translations"],"additionalProperties":false}}},
        "required":["terms"],
        "additionalProperties":false
    });

    let parsed = chat_completion_json(
        &client,
        &config,
        messages,
        "glossary_terms",
        schema,
        Some(&reporter),
        1,
        1,
    )
    .await?;
    let mut terms = Vec::new();
    if let Some(items) = parsed.get("terms").and_then(Value::as_array) {
        for item in items {
            let term = item
                .get("term")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if term.is_empty() {
                continue;
            }
            let category = item
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("General")
                .to_string();
            let note = item
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("Auto generated glossary term.")
                .to_string();
            let translations = item
                .get("translations")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, value)| (key.clone(), json_value_to_string(value)))
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            let mut glossary_term = GlossaryTerm {
                glossary_id: make_glossary_id(&term),
                term,
                category,
                note,
                enabled: true,
                source_sheet: String::new(),
                row_id: String::new(),
                row_number: 0,
                sheet_id: String::new(),
                translations,
            };
            if let Some(row) =
                resolve_glossary_row(&glossary_term, &request.rows, &request.source_column_id)
            {
                bind_glossary_term_to_row(&mut glossary_term, &row);
            }
            terms.push(glossary_term);
        }
    }
    reporter.complete(format!("Generated {} glossary terms.", terms.len()), 1, 1);
    Ok(GlossaryResponse {
        terms,
        rows: Vec::new(),
        live_sync_denied: false,
        sync_message: String::new(),
    })
}

#[tauri::command]
async fn fill_glossary_translations(
    app: AppHandle,
    request: FillGlossaryRequest,
    config: AppConfig,
) -> Result<GlossaryResponse, String> {
    ensure_feishu_config(&config)?;
    ensure_ai_config(&config)?;
    let client = build_http_client()?;
    let tenant_token = fetch_feishu_tenant_token(&client, &config).await?;
    let reporter = StreamReporter::new(&app, "fillGlossary");
    let mut glossary = request.glossary.clone();
    let mut rows = request.rows.clone();
    if glossary.is_empty()
        || request.target_columns.is_empty()
        || request.source_column_id.trim().is_empty()
    {
        reporter.complete("No glossary rows or target columns to fill.", 0, 0);
        return Ok(GlossaryResponse {
            terms: glossary,
            rows: Vec::new(),
            live_sync_denied: false,
            sync_message: String::new(),
        });
    }

    for term in &mut glossary {
        ensure_glossary_identity(term);
    }

    let fillable_indexes = glossary
        .iter()
        .enumerate()
        .filter(|(_, term)| term.enabled)
        .filter(|(_, term)| !term.term.trim().is_empty())
        .filter(|(_, term)| {
            request.target_columns.iter().any(|column| {
                term.translations
                    .get(&column.id)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            })
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let chunk_size = 24usize;
    let total_chunks = fillable_indexes.len().div_ceil(chunk_size).max(1);
    reporter.start("Filling empty glossary translations.", total_chunks);
    let category_column_id = find_optional_column_id(&request.columns, &["Category", "分类"]);
    let note_column_id = find_optional_column_id(&request.columns, &["Note", "备注"]);
    let target_column_ids = request
        .target_columns
        .iter()
        .map(|column| column.id.clone())
        .collect::<Vec<_>>();
    let prompt = if request.prompt.trim().is_empty() {
        "Fill missing glossary translations. Keep terms short and consistent.".to_string()
    } else {
        request.prompt.clone()
    };
    let mut completed_chunks = 0usize;
    let mut changed_rows: BTreeMap<String, Row> = BTreeMap::new();
    let mut live_sync_denied = false;
    let mut sync_message = String::new();

    for chunk_indexes in fillable_indexes.chunks(chunk_size) {
        let next_chunk = completed_chunks + 1;
        reporter.status(
            format!(
                "Filling glossary batch {}/{} ({} terms x {} targets)",
                next_chunk,
                total_chunks,
                chunk_indexes.len(),
                request.target_columns.len()
            ),
            next_chunk,
            total_chunks,
        );
        let chunk_terms = chunk_indexes
            .iter()
            .map(|index| glossary[*index].clone())
            .collect::<Vec<_>>();
        let translated_map = match translate_glossary_batch_multi(
            &client,
            &config,
            &prompt,
            &request.target_columns,
            &chunk_terms,
            Some(&reporter),
            next_chunk,
            total_chunks,
        )
        .await
        {
            Ok(chunk_map) => chunk_map,
            Err(error) => {
                reporter.error(error.clone(), next_chunk, total_chunks);
                return Err(error);
            }
        };

        let mut updated_terms = Vec::new();
        let mut rows_to_sync = Vec::new();

        for index in chunk_indexes {
            let Some(translations) = translated_map.get(&glossary[*index].glossary_id) else {
                continue;
            };
            let mut term = glossary[*index].clone();
            let mut term_changed = false;
            for column in &request.target_columns {
                let current = term
                    .translations
                    .get(&column.id)
                    .cloned()
                    .unwrap_or_default();
                if !current.trim().is_empty() {
                    continue;
                }
                if let Some(translated) = translations.get(&column.id) {
                    if !translated.trim().is_empty() {
                        term.translations
                            .insert(column.id.clone(), translated.clone());
                        term_changed = true;
                    }
                }
            }
            if !term_changed {
                continue;
            }

            let mut row =
                resolve_glossary_row(&term, &rows, &request.source_column_id).or_else(|| {
                    build_glossary_row_template(
                        &term,
                        &rows,
                        &request.source_column_id,
                        &target_column_ids,
                    )
                    .ok()
                });
            if let Some(mut row) = row.take() {
                apply_glossary_term_to_row(
                    &term,
                    &mut row,
                    &request.source_column_id,
                    &target_column_ids,
                    category_column_id.as_deref(),
                    note_column_id.as_deref(),
                );
                bind_glossary_term_to_row(&mut term, &row);
                if let Some(existing_row) = rows
                    .iter_mut()
                    .find(|existing| existing.row_id == row.row_id)
                {
                    *existing_row = row.clone();
                } else {
                    rows.push(row.clone());
                }
                changed_rows.insert(row.row_id.clone(), row.clone());
                rows_to_sync.push(row);
            }

            glossary[*index] = term.clone();
            updated_terms.push(term);
        }

        if !rows_to_sync.is_empty() && !live_sync_denied {
            let mut sync_columns = vec![request.source_column_id.clone()];
            sync_columns.extend(target_column_ids.iter().cloned());
            if let Some(column_id) = &category_column_id {
                sync_columns.push(column_id.clone());
            }
            if let Some(column_id) = &note_column_id {
                sync_columns.push(column_id.clone());
            }
            if let Err(error) = write_glossary_rows_to_feishu(
                &client,
                &tenant_token,
                &request.spreadsheet_id,
                &rows_to_sync,
                &sync_columns,
            )
            .await
            {
                if is_feishu_permission_denied(&error) {
                    live_sync_denied = true;
                    sync_message = error.clone();
                    reporter.status(
                        "Glossary live sync denied. Switched to local-only mode for this run.",
                        next_chunk,
                        total_chunks,
                    );
                } else {
                    return Err(error);
                }
            }
        }

        if !updated_terms.is_empty() {
            reporter.glossary_patch(
                format!("Filled glossary batch {}.", next_chunk),
                updated_terms.clone(),
                rows_to_sync.clone(),
                Vec::new(),
            );
        }
        completed_chunks = next_chunk;
    }

    reporter.complete(
        "Glossary translation fill completed.",
        completed_chunks,
        total_chunks,
    );
    Ok(GlossaryResponse {
        terms: glossary,
        rows: changed_rows.into_values().collect(),
        live_sync_denied,
        sync_message,
    })
}

#[tauri::command]
async fn sync_glossary_term(
    request: GlossarySyncRequest,
    config: AppConfig,
) -> Result<GlossarySyncResponse, String> {
    ensure_feishu_config(&config)?;
    let client = build_http_client()?;
    let tenant_token = fetch_feishu_tenant_token(&client, &config).await?;
    let mut term = request.glossary_term.clone();
    ensure_glossary_identity(&mut term);

    if request.remove {
        let Some(mut row) = resolve_glossary_row(&term, &request.rows, &request.source_column_id)
        else {
            return Ok(GlossarySyncResponse {
                term: None,
                row: None,
                removed: true,
                permission_denied: false,
                message: String::new(),
            });
        };
        row.values
            .insert(request.source_column_id.clone(), String::new());
        for column_id in &request.target_column_ids {
            row.values.insert(column_id.clone(), String::new());
        }
        let mut sync_columns = vec![request.source_column_id.clone()];
        sync_columns.extend(request.target_column_ids.iter().cloned());
        if let Err(error) = write_row_cells_to_feishu(
            &client,
            &tenant_token,
            &request.spreadsheet_id,
            &row,
            &sync_columns,
        )
        .await
        {
            if is_feishu_permission_denied(&error) {
                return Ok(GlossarySyncResponse {
                    term: None,
                    row: Some(row),
                    removed: true,
                    permission_denied: true,
                    message: error,
                });
            }
            return Err(error);
        }
        return Ok(GlossarySyncResponse {
            term: None,
            row: Some(row),
            removed: true,
            permission_denied: false,
            message: String::new(),
        });
    }

    if is_blank_glossary_term(&term, &request.target_column_ids) {
        return Ok(GlossarySyncResponse {
            term: Some(term),
            row: None,
            removed: false,
            permission_denied: false,
            message: String::new(),
        });
    }

    let category_column_id = find_optional_column_id(&request.columns, &["Category", "分类"]);
    let note_column_id = find_optional_column_id(&request.columns, &["Note", "备注"]);
    let mut row = resolve_glossary_row(&term, &request.rows, &request.source_column_id)
        .or_else(|| {
            build_glossary_row_template(
                &term,
                &request.rows,
                &request.source_column_id,
                &request.target_column_ids,
            )
            .ok()
        })
        .ok_or_else(|| "Unable to resolve or create a sheet row for glossary sync.".to_string())?;
    apply_glossary_term_to_row(
        &term,
        &mut row,
        &request.source_column_id,
        &request.target_column_ids,
        category_column_id.as_deref(),
        note_column_id.as_deref(),
    );
    let mut sync_columns = vec![request.source_column_id.clone()];
    sync_columns.extend(request.target_column_ids.iter().cloned());
    if let Some(column_id) = &category_column_id {
        sync_columns.push(column_id.clone());
    }
    if let Some(column_id) = &note_column_id {
        sync_columns.push(column_id.clone());
    }
    if let Err(error) = write_row_cells_to_feishu(
        &client,
        &tenant_token,
        &request.spreadsheet_id,
        &row,
        &sync_columns,
    )
    .await
    {
        if is_feishu_permission_denied(&error) {
            bind_glossary_term_to_row(&mut term, &row);
            return Ok(GlossarySyncResponse {
                term: Some(term),
                row: Some(row),
                removed: false,
                permission_denied: true,
                message: error,
            });
        }
        return Err(error);
    }
    bind_glossary_term_to_row(&mut term, &row);
    Ok(GlossarySyncResponse {
        term: Some(term),
        row: Some(row),
        removed: false,
        permission_denied: false,
        message: String::new(),
    })
}

#[tauri::command]
async fn write_glossary_to_feishu(
    request: GlossaryWritebackRequest,
    config: AppConfig,
) -> Result<GlossaryResponse, String> {
    ensure_feishu_config(&config)?;
    let client = build_http_client()?;
    let tenant_token = fetch_feishu_tenant_token(&client, &config).await?;
    let category_column_id = find_optional_column_id(&request.columns, &["Category", "分类"]);
    let note_column_id = find_optional_column_id(&request.columns, &["Note", "备注"]);
    let mut rows = request.rows.clone();
    let mut glossary = request.glossary.clone();
    let mut rows_to_sync = Vec::new();

    for term in &mut glossary {
        ensure_glossary_identity(term);
        if is_blank_glossary_term(term, &request.target_column_ids) {
            continue;
        }
        let mut row = resolve_glossary_row(term, &rows, &request.source_column_id)
            .or_else(|| {
                build_glossary_row_template(
                    term,
                    &rows,
                    &request.source_column_id,
                    &request.target_column_ids,
                )
                .ok()
            })
            .ok_or_else(|| {
                "Unable to resolve or create a sheet row for glossary write-back.".to_string()
            })?;
        apply_glossary_term_to_row(
            term,
            &mut row,
            &request.source_column_id,
            &request.target_column_ids,
            category_column_id.as_deref(),
            note_column_id.as_deref(),
        );
        bind_glossary_term_to_row(term, &row);
        if let Some(existing_row) = rows
            .iter_mut()
            .find(|existing| existing.row_id == row.row_id)
        {
            *existing_row = row.clone();
        } else {
            rows.push(row.clone());
        }
        rows_to_sync.push(row);
    }

    let mut sync_columns = vec![request.source_column_id.clone()];
    sync_columns.extend(request.target_column_ids.iter().cloned());
    if let Some(column_id) = &category_column_id {
        sync_columns.push(column_id.clone());
    }
    if let Some(column_id) = &note_column_id {
        sync_columns.push(column_id.clone());
    }
    write_glossary_rows_to_feishu(
        &client,
        &tenant_token,
        &request.spreadsheet_id,
        &rows_to_sync,
        &sync_columns,
    )
    .await?;

    Ok(GlossaryResponse {
        terms: glossary,
        rows: rows_to_sync,
        live_sync_denied: false,
        sync_message: String::new(),
    })
}

#[tauri::command]
async fn generate_translation_prompt(
    app: AppHandle,
    request: PromptSuggestionRequest,
    config: AppConfig,
) -> Result<PromptSuggestionResponse, String> {
    ensure_ai_config(&config)?;
    let client = build_http_client()?;
    let reporter = StreamReporter::new(&app, "prompt");
    reporter.start("Generating translation prompt.", 1);
    let source_samples = collect_source_samples(&request.rows, &request.source_column_id, 80);
    if source_samples.is_empty() {
        reporter.error("The selected source column is empty.", 1, 1);
        return Err("The selected source column is empty.".into());
    }
    let source_analysis = analyze_source_samples(&source_samples);
    let messages = vec![
        json!({"role":"system","content":"You create concise but practical translation prompts for game localization tools. Base the prompt on the provided source texts, formatting patterns, and target languages."}),
        json!({"role":"user","content":format!(
            "Source column: {} ({})\nTarget columns: {}\n\nSource text analysis:\n{}\n\nRepresentative source texts:\n{}\n\nWrite one practical translation prompt for a game localization workflow. The prompt should:\n1. reflect the real text style above\n2. explicitly preserve placeholders, rich text tags, sprite tags, escaped newlines, real line breaks, numbers, and symbols\n3. keep UI text concise when the source is concise\n4. maintain terminology consistency across the whole sheet\n5. avoid explanations and extra notes\n6. be ready to paste into the tool as-is\n\nReturn only the final prompt text.",
            request.source_column_name,
            request.source_column_id,
            request.target_columns.iter().map(|column| column.name.clone()).collect::<Vec<_>>().join(", ")
            ,
            source_analysis,
            source_samples
                .iter()
                .take(24)
                .enumerate()
                .map(|(index, text)| format!("{}. {}", index + 1, text))
                .collect::<Vec<_>>()
                .join("\n")
        )}),
    ];
    let schema = json!({"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"],"additionalProperties":false});
    let parsed = match chat_completion_json(
        &client,
        &config,
        messages,
        "translation_prompt",
        schema,
        Some(&reporter),
        1,
        1,
    )
    .await
    {
        Ok(parsed) => parsed,
        Err(error) => {
            reporter.error(error.clone(), 1, 1);
            return Err(error);
        }
    };
    let response = PromptSuggestionResponse {
        prompt: parsed
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    reporter.complete("Translation prompt generated.", 1, 1);
    Ok(response)
}

#[tauri::command]
async fn run_translation(
    app: AppHandle,
    request: TranslationRequest,
    config: AppConfig,
) -> Result<TranslationResponse, String> {
    ensure_feishu_config(&config)?;
    ensure_ai_config(&config)?;
    clear_cancelled_task("translation");
    let reporter = StreamReporter::new(&app, "translation");
    if request.source_column_id.trim().is_empty() {
        reporter.error("A source column is required.", 0, 1);
        return Err("A source column is required.".into());
    }

    let client = build_http_client()?;
    let tenant_token = fetch_feishu_tenant_token(&client, &config).await?;
    let locked = request
        .locked_column_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let column_map = request
        .columns
        .iter()
        .map(|column| (column.id.clone(), column.clone()))
        .collect::<HashMap<_, _>>();
    let target_columns = request
        .target_column_ids
        .iter()
        .filter(|column_id| !locked.contains(column_id.as_str()))
        .filter_map(|column_id| {
            column_map.get(column_id).map(|column| TargetColumnInfo {
                id: column.id.clone(),
                name: column.name.clone(),
            })
        })
        .collect::<Vec<_>>();
    let target_column_ids = target_columns
        .iter()
        .map(|column| column.id.as_str())
        .collect::<HashSet<_>>();
    if target_columns.is_empty() {
        reporter.error("No writable target columns selected.", 0, 1);
        return Err("No writable target columns selected.".into());
    }
    let context_columns = request
        .columns
        .iter()
        .filter(|column| column.id != request.source_column_id)
        .filter(|column| !target_column_ids.contains(column.id.as_str()))
        .filter(|column| column.id != "key" && column.id != "id")
        .cloned()
        .collect::<Vec<_>>();

    let mut rows = request.rows.clone();
    let mut changed_cells = 0usize;
    let mut processed_rows = HashSet::new();
    let mut skipped_rows = 0usize;
    let mut failed_rows = Vec::new();
    let mut failed_row_ids = Vec::new();
    let mut successful_chunks = 0usize;
    let source_column_name = column_map
        .get(&request.source_column_id)
        .map(|column| column.name.as_str())
        .unwrap_or(&request.source_column_id);
    let items = collect_multi_target_translation_items(
        &rows,
        &request.source_column_id,
        &target_columns,
        &context_columns,
        &request.write_mode,
    );
    let include_existing_translations = write_mode_uses_existing_translations(&request.write_mode);
    let merged_prompt = build_runtime_translation_prompt_multi(
        &request.prompt,
        source_column_name,
        &target_columns,
    );
    let glossary_context =
        compact_multilingual_glossary_payload(&request.glossary, &target_columns);
    let token_budget = resolve_translation_token_budget(
        request.translation_token_budget,
        &items,
        &merged_prompt,
        &glossary_context,
        target_columns.len(),
        include_existing_translations,
    );
    let item_chunks = chunk_multi_target_translation_items(
        &items,
        &merged_prompt,
        &glossary_context,
        target_columns.len(),
        include_existing_translations,
        token_budget,
    );
    let total_chunks = item_chunks.len().max(1);
    let mut completed_chunks = 0usize;
    reporter.start("Starting translation and write-back.", total_chunks);

    for chunk in item_chunks {
        if let Err(error) = ensure_task_not_cancelled("translation") {
            reporter.complete(
                "Translation stopped. No write-back was performed.",
                completed_chunks,
                total_chunks,
            );
            clear_cancelled_task("translation");
            return Err(error);
        }
        let next_chunk = completed_chunks + 1;
        reporter.status(
            format!(
                "Translating batch {}/{} ({} rows x {} targets, ~{} token budget)",
                next_chunk,
                total_chunks,
                chunk.len(),
                target_columns.len(),
                token_budget
            ),
            next_chunk,
            total_chunks,
        );
        match translate_batch_multi_target(
            &client,
            &config,
            &merged_prompt,
            &glossary_context,
            &request.source_column_id,
            &target_columns,
            chunk,
            include_existing_translations,
            Some(&reporter),
            next_chunk,
            total_chunks,
        )
        .await
        {
            Ok(chunk_map) => {
                successful_chunks += 1;
                for row in &mut rows {
                    let Some(translations) = chunk_map.get(&row.row_id) else {
                        continue;
                    };
                    for column in &target_columns {
                        let Some(translated) = translations.get(&column.id) else {
                            continue;
                        };
                        let current = row.values.get(&column.id).cloned().unwrap_or_default();
                        if current != *translated {
                            row.values.insert(column.id.clone(), translated.clone());
                            changed_cells += 1;
                            processed_rows.insert(row.row_id.clone());
                        }
                    }
                }
            }
            Err(error) => {
                reporter.error(error.clone(), next_chunk, total_chunks);
                if error == "TRANSLATION_CANCELLED" {
                    return Err(error);
                }
                failed_row_ids.extend(chunk.iter().map(|item| item.row_id.clone()));
                failed_rows.extend(
                    chunk
                        .iter()
                        .map(|item| format!("{}: {}", item.row_id, error.clone())),
                );
            }
        }
        completed_chunks = next_chunk;
    }

    for row in &rows {
        let source = row
            .values
            .get(&request.source_column_id)
            .map(|value| value.trim())
            .unwrap_or_default();
        if source.is_empty() {
            skipped_rows += 1;
        }
    }

    ensure_task_not_cancelled("translation")?;
    let rows_to_sync = rows
        .iter()
        .filter(|row| processed_rows.contains(&row.row_id))
        .cloned()
        .collect::<Vec<_>>();
    if !rows_to_sync.is_empty() {
        let target_column_ids = target_columns
            .iter()
            .map(|column| column.id.clone())
            .collect::<Vec<_>>();
        writeback::sync(
            &client,
            &tenant_token,
            &request.spreadsheet_id,
            &request.rows,
            &rows_to_sync,
            &target_column_ids,
        )
        .await?;
    }
    clear_cancelled_task("translation");
    let status = translation_outcome(failed_rows.len(), successful_chunks, changed_cells);
    let message = format!(
        "Translation {}. {} verified cell changes; {} failed rows.",
        status,
        changed_cells,
        failed_rows.len()
    );
    if failed_rows.is_empty() {
        reporter.complete(message, completed_chunks, total_chunks);
    } else {
        reporter.error(message, completed_chunks, total_chunks);
    }
    Ok(TranslationResponse {
        rows,
        summary: TranslationSummary {
            changed_cells,
            processed_rows: processed_rows.len(),
            skipped_rows,
            failed_rows,
            failed_row_ids,
            status: status.into(),
        },
    })
}

#[tauri::command]
async fn stop_translation() -> Result<StopTaskResponse, String> {
    cancel_task("translation");
    Ok(StopTaskResponse { stopped: true })
}

async fn fetch_feishu_tenant_token(client: &Client, config: &AppConfig) -> Result<String, String> {
    let response = client
        .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
        .json(&json!({"app_id": config.feishu_app_id, "app_secret": config.feishu_app_secret}))
        .send()
        .await
        .map_err(|error| format!("Failed to fetch Feishu tenant token: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Feishu auth response: {}", error))?;
    if !status.is_success() {
        return Err(format!("Feishu auth failed: HTTP {} {}", status, body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse Feishu auth JSON: {}", error))?;
    if json.get("code").and_then(Value::as_i64).unwrap_or(-1) != 0 {
        return Err(format!(
            "Feishu auth failed: {}",
            json.get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        ));
    }
    json.get("tenant_access_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Feishu auth succeeded but tenant_access_token is missing.".into())
}

async fn fetch_feishu_metainfo(
    client: &Client,
    token: &str,
    spreadsheet_id: &str,
) -> Result<Value, String> {
    let response = client
        .get(format!(
            "https://open.feishu.cn/open-apis/sheets/v2/spreadsheets/{}/metainfo",
            spreadsheet_id
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("Failed to fetch spreadsheet metainfo: {}", error))?;
    parse_feishu_json_response(response, "Failed to fetch spreadsheet metainfo").await
}

async fn fetch_sheet_values(
    client: &Client,
    token: &str,
    spreadsheet_id: &str,
    sheet_id: &str,
) -> Result<Vec<Vec<String>>, String> {
    let encoded_range = urlencoding::encode(sheet_id);
    let response = client
        .get(format!(
            "https://open.feishu.cn/open-apis/sheets/v2/spreadsheets/{}/values/{}",
            spreadsheet_id, encoded_range
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("Failed to fetch sheet values: {}", error))?;
    let json = parse_feishu_json_response(response, "Failed to fetch sheet values").await?;
    let values = json
        .get("data")
        .and_then(|data| data.get("valueRange"))
        .and_then(|range| range.get("values"))
        .and_then(Value::as_array)
        .ok_or_else(|| "valueRange.values is missing.".to_string())?;
    Ok(values
        .iter()
        .map(|row| {
            row.as_array()
                .map(|cells| cells.iter().map(json_value_to_string).collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect())
}

async fn parse_feishu_json_response(
    response: reqwest::Response,
    prefix: &str,
) -> Result<Value, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("{}: {}", prefix, error))?;
    if !status.is_success() {
        return Err(format!("{}: HTTP {} {}", prefix, status, body));
    }
    let json: Value =
        serde_json::from_str(&body).map_err(|error| format!("{}: {}", prefix, error))?;
    if json.get("code").and_then(Value::as_i64).unwrap_or(-1) != 0 {
        return Err(format!(
            "{}: {}",
            prefix,
            json.get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        ));
    }
    Ok(json)
}

async fn write_row_cells_to_feishu(
    client: &Client,
    tenant_token: &str,
    spreadsheet_id: &str,
    row: &Row,
    column_ids: &[String],
) -> Result<(), String> {
    let mut value_ranges = Vec::new();
    for column_id in column_ids {
        if !row
            .available_column_ids
            .iter()
            .any(|value| value == column_id)
        {
            continue;
        }
        let Some(column_index) = row
            .available_column_ids
            .iter()
            .position(|value| value == column_id)
        else {
            continue;
        };
        let column_letter = to_excel_column_name(column_index);
        let range = format!(
            "{}!{}{}:{}{}",
            row.sheet_id, column_letter, row.row_number, column_letter, row.row_number
        );
        let value = row.values.get(column_id).cloned().unwrap_or_default();
        value_ranges.push(json!({ "range": range, "values": [[value]] }));
    }
    if value_ranges.is_empty() {
        return Ok(());
    }
    let response = client
        .post(format!(
            "https://open.feishu.cn/open-apis/sheets/v2/spreadsheets/{}/values_batch_update",
            spreadsheet_id
        ))
        .bearer_auth(tenant_token)
        .json(&json!({ "valueRanges": value_ranges }))
        .send()
        .await
        .map_err(|error| format!("Failed to sync glossary row to Feishu: {}", error))?;
    parse_feishu_json_response(response, "Failed to sync glossary row to Feishu").await?;
    Ok(())
}

async fn write_glossary_rows_to_feishu(
    client: &Client,
    tenant_token: &str,
    spreadsheet_id: &str,
    rows: &[Row],
    column_ids: &[String],
) -> Result<(), String> {
    let mut value_ranges = Vec::new();
    for row in rows {
        for column_id in column_ids {
            if !row
                .available_column_ids
                .iter()
                .any(|value| value == column_id)
            {
                continue;
            }
            let Some(column_index) = row
                .available_column_ids
                .iter()
                .position(|value| value == column_id)
            else {
                continue;
            };
            let column_letter = to_excel_column_name(column_index);
            let range = format!(
                "{}!{}{}:{}{}",
                row.sheet_id, column_letter, row.row_number, column_letter, row.row_number
            );
            let value = row.values.get(column_id).cloned().unwrap_or_default();
            value_ranges.push(json!({ "range": range, "values": [[value]] }));
        }
    }
    if value_ranges.is_empty() {
        return Ok(());
    }
    let response = client
        .post(format!(
            "https://open.feishu.cn/open-apis/sheets/v2/spreadsheets/{}/values_batch_update",
            spreadsheet_id
        ))
        .bearer_auth(tenant_token)
        .json(&json!({ "valueRanges": value_ranges }))
        .send()
        .await
        .map_err(|error| format!("Failed to sync glossary rows to Feishu: {}", error))?;
    parse_feishu_json_response(response, "Failed to sync glossary rows to Feishu").await?;
    Ok(())
}

fn translation_outcome(failed: usize, successful_batches: usize, changed: usize) -> &'static str {
    if failed > 0 {
        if successful_batches == 0 {
            "failed"
        } else {
            "partial"
        }
    } else if changed == 0 {
        "unchanged"
    } else {
        "complete"
    }
}

fn parse_translation_batch(
    parsed: &Value,
    expected: &[MultiTargetBatchItem],
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let items = parsed
        .get("items")
        .and_then(Value::as_array)
        .ok_or("Translation response has no items.")?;
    let mut result = HashMap::new();
    for item in items {
        let id = item
            .get("rowId")
            .and_then(Value::as_str)
            .ok_or("Translation rowId missing.")?;
        let source = expected
            .iter()
            .find(|row| row.row_id == id)
            .ok_or("Translation returned an unknown rowId.")?;
        if result.contains_key(id) {
            return Err("Translation returned duplicate rowIds.".into());
        }
        let mut values = HashMap::new();
        for target in &source.missing_targets {
            let text = item
                .get("translations")
                .and_then(|v| v.get(&target.id))
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| {
                    format!("Missing translation for row {id}, column {}.", target.id)
                })?;
            values.insert(target.id.clone(), text.to_string());
        }
        result.insert(id.to_string(), values);
    }
    if result.len() != expected.len() {
        return Err("Translation response omitted requested rows.".into());
    }
    Ok(result)
}

async fn translate_batch_multi_target(
    client: &Client,
    config: &AppConfig,
    prompt: &str,
    glossary_text: &str,
    source_column_id: &str,
    target_columns: &[TargetColumnInfo],
    items: &[MultiTargetBatchItem],
    include_existing_translations: bool,
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let mut translation_properties = serde_json::Map::new();
    for column in target_columns {
        translation_properties.insert(column.id.clone(), json!({ "type": "string" }));
    }
    let messages = vec![
        json!({
            "role":"system",
            "content":format!(
                "You are a professional game localization translator. Translate the source column {} into the requested target columns. Return one result per input row. For each row, only fill the target column ids listed in missingTargets. Auxiliary rowContext fields may be provided to help you understand gameplay meaning, state, notes, descriptions, limits, or type information, but they must never be copied unless they help produce a better translation. {} Preserve placeholders, rich text tags, sprite tags, line breaks, escaped newlines, numbering, and punctuation exactly.\n\nUser prompt:\n{}\n\nGlossary:\n{}",
                source_column_id,
                if include_existing_translations {
                    "existingTranslations may be provided only as consistency references for already translated sibling languages."
                } else {
                    "Do not rely on old target-language translations; regenerate the requested targets from the source text, glossary, and rowContext."
                },
                prompt,
                if glossary_text.is_empty() { "None" } else { glossary_text }
            )
        }),
        json!({
            "role":"user",
            "content":items
                .iter()
                .map(|item| {
                    json!({
                        "rowId": item.row_id,
                        "source": item.source,
                        "rowContext": item.auxiliary_context,
                        "missingTargets": item
                            .missing_targets
                            .iter()
                            .map(|column| json!({"id": column.id, "name": column.name}))
                            .collect::<Vec<_>>(),
                        "existingTranslations": if include_existing_translations {
                            json!(item.existing_translations)
                        } else {
                            json!({})
                        }
                    })
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        }),
    ];
    let schema = json!({
        "type":"object",
        "properties":{"items":{"type":"array","items":{"type":"object","properties":{
            "rowId":{"type":"string"},
            "translations":{
                "type":"object",
                "properties":translation_properties,
                "additionalProperties":false
            }
        },"required":["rowId","translations"],"additionalProperties":false}}},
        "required":["items"],
        "additionalProperties":false
    });
    let parsed = chat_completion_json(
        client,
        config,
        messages,
        "translation_batch_multi",
        schema,
        reporter,
        current,
        total,
    )
    .await?;
    parse_translation_batch(&parsed, items)
}

async fn translate_glossary_batch_multi(
    client: &Client,
    config: &AppConfig,
    prompt: &str,
    target_columns: &[Column],
    terms: &[GlossaryTerm],
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let mut translation_properties = serde_json::Map::new();
    for column in target_columns {
        translation_properties.insert(column.id.clone(), json!({ "type": "string" }));
    }
    let schema = json!({
        "type":"object",
        "properties":{
            "items":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{
                        "rowId":{"type":"string"},
                        "translations":{
                            "type":"object",
                            "properties":translation_properties,
                            "additionalProperties":false
                        }
                    },
                    "required":["rowId","translations"],
                    "additionalProperties":false
                }
            }
        },
        "required":["items"],
        "additionalProperties":false
    });
    let target_list = target_columns
        .iter()
        .map(|column| format!("{}({})", column.name, column.id))
        .collect::<Vec<_>>()
        .join(", ");
    let messages = vec![
        json!({
            "role":"system",
            "content":format!(
                "You are a professional game localization glossary translator. Fill only the missing translations for each glossary term. Return a translations object keyed by target column id. Keep terms short, consistent, and preserve placeholders, tags, escapes, numbers, and punctuation exactly.\n\nTarget columns:\n{}\n\nUser prompt:\n{}",
                target_list,
                prompt
            )
        }),
        json!({
            "role":"user",
            "content":terms
                .iter()
                .map(|term| {
                    let missing_targets = target_columns
                        .iter()
                        .filter(|column| term.translations.get(&column.id).map(|value| value.trim().is_empty()).unwrap_or(true))
                        .map(|column| json!({"id": column.id, "name": column.name}))
                        .collect::<Vec<_>>();
                    json!({
                        "rowId": term.glossary_id,
                        "term": term.term,
                        "category": term.category,
                        "note": term.note,
                        "sourceSheet": term.source_sheet,
                        "existingTranslations": term.translations,
                        "missingTargets": missing_targets
                    })
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        }),
    ];
    let parsed = chat_completion_json(
        client,
        config,
        messages,
        "glossary_fill_batch",
        schema,
        reporter,
        current,
        total,
    )
    .await?;
    let mut result = HashMap::new();
    if let Some(items) = parsed.get("items").and_then(Value::as_array) {
        for item in items {
            let row_id = item
                .get("rowId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let translations = item
                .get("translations")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, value)| (key.clone(), json_value_to_string(value)))
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            if !row_id.is_empty() {
                result.insert(row_id.to_string(), translations);
            }
        }
    }
    Ok(result)
}

async fn chat_completion_json(
    client: &Client,
    config: &AppConfig,
    messages: Vec<Value>,
    schema_name: &str,
    schema: Value,
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<Value, String> {
    if let Some(reporter) = reporter {
        reporter.emit_debug(
            format!(
                "Sending {} request to {} / {}",
                schema_name,
                ai_provider_display_name(config),
                config.openai_model
            ),
            messages_to_text(&messages),
            current,
            total,
        );
    }
    let task = reporter
        .map(|reporter| reporter.task.as_str())
        .unwrap_or("ai");
    reliability::cancellable(task, Duration::from_secs(900), async {
        match config.ai_provider.trim().to_ascii_lowercase().as_str() {
            "gemini" => {
                chat_completion_json_gemini(
                    client, config, messages, schema, reporter, current, total,
                )
                .await
            }
            "claude" => {
                chat_completion_json_claude(client, config, messages, reporter, current, total)
                    .await
            }
            _ => {
                chat_completion_json_openai(
                    client,
                    config,
                    messages,
                    schema_name,
                    schema,
                    reporter,
                    current,
                    total,
                )
                .await
            }
        }
    })
    .await
}

async fn probe_ai_provider(
    client: &Client,
    config: &AppConfig,
    provider: &str,
    base_url: &str,
) -> Result<String, String> {
    let normalized = normalize_ai_provider(provider);
    let models = fetch_provider_models(client, config, &normalized, base_url).await?;
    let model_refs = models.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(pick_preferred_model(
        config.openai_model.trim(),
        &model_refs,
        preferred_models_for_provider(&normalized),
    ))
}

async fn fetch_provider_models(
    client: &Client,
    config: &AppConfig,
    provider: &str,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let provider = normalize_ai_provider(provider);
    let base_url = normalize_base_url_for_provider(&provider, base_url);
    match provider.as_str() {
        "gemini" => fetch_gemini_models(client, config, &base_url).await,
        "claude" => fetch_claude_models(client, config, &base_url).await,
        _ => fetch_openai_compatible_models(client, config, &base_url).await,
    }
}

async fn fetch_openai_compatible_models(
    client: &Client,
    config: &AppConfig,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let response = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .bearer_auth(config.openai_api_key.trim())
        .send()
        .await
        .map_err(|error| format!("OpenAI probe failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read OpenAI probe response: {}", error))?;
    if !status.is_success() {
        return Err(format!("OpenAI probe failed: HTTP {} {}", status, body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse OpenAI probe JSON: {}", error))?;
    let models = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("OpenAI probe returned no model list: {}", body))?;
    let mut ids = models
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .filter(|id| is_supported_text_model("openai", id))
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("OpenAI probe returned an empty model list.".into());
    }
    sort_models(&mut ids, preferred_models_for_provider("openai"));
    Ok(ids)
}

async fn fetch_gemini_models(
    client: &Client,
    config: &AppConfig,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let response = client
        .get(format!(
            "{}/models?key={}",
            base_url.trim_end_matches('/'),
            config.openai_api_key.trim()
        ))
        .send()
        .await
        .map_err(|error| format!("Gemini probe failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Gemini probe response: {}", error))?;
    if !status.is_success() {
        return Err(format!("Gemini probe failed: HTTP {} {}", status, body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse Gemini probe JSON: {}", error))?;
    let models = json
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Gemini probe returned no model list: {}", body))?;
    let mut ids = models
        .iter()
        .filter(|item| {
            item.get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .map(|methods| {
                    methods
                        .iter()
                        .any(|method| method.as_str() == Some("generateContent"))
                })
                .unwrap_or(true)
        })
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .map(|name| name.trim_start_matches("models/"))
        .filter(|id| is_supported_text_model("gemini", id))
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("Gemini probe returned an empty model list.".into());
    }
    sort_models(&mut ids, preferred_models_for_provider("gemini"));
    Ok(ids)
}

async fn fetch_claude_models(
    client: &Client,
    config: &AppConfig,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let response = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .header("x-api-key", config.openai_api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|error| format!("Claude model request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Claude model response: {}", error))?;
    if !status.is_success() {
        return Err(format!(
            "Claude model request failed: HTTP {} {}",
            status, body
        ));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse Claude model JSON: {}", error))?;
    let models = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Claude model request returned no model list: {}", body))?;
    let mut ids = models
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .filter(|id| is_supported_text_model("claude", id))
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("Claude model request returned an empty model list.".into());
    }
    sort_models(&mut ids, preferred_models_for_provider("claude"));
    Ok(ids)
}

fn pick_preferred_model(current_model: &str, ids: &[&str], preferred: &[&str]) -> String {
    if !current_model.is_empty() {
        return current_model.to_string();
    }
    if let Some(model) = preferred
        .iter()
        .find(|target| ids.iter().any(|id| id == *target))
    {
        return (*model).to_string();
    }
    ids.first().copied().unwrap_or_default().to_string()
}

fn normalize_ai_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "claude" => "claude".into(),
        "deepseek" => "deepseek".into(),
        "doubao" => "doubao".into(),
        "custom" => "custom".into(),
        "packy" => "packy".into(),
        "gemini" => "gemini".into(),
        "openai" => "openai".into(),
        _ => "openai".into(),
    }
}

fn normalize_base_url_for_provider(provider: &str, base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return default_ai_base_url(provider).to_string();
    }
    match normalize_ai_provider(provider).as_str() {
        "openai" | "deepseek" | "custom" | "packy" => ensure_openai_compatible_base_url(&trimmed),
        _ => trimmed,
    }
}

fn ensure_openai_compatible_base_url(base_url: &str) -> String {
    let last_segment = base_url
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if is_version_path_segment(&last_segment) {
        return base_url.to_string();
    }
    format!("{}/v1", base_url)
}

fn is_version_path_segment(segment: &str) -> bool {
    if segment == "v1beta" {
        return true;
    }
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
}

fn default_ai_base_url(provider: &str) -> &'static str {
    match normalize_ai_provider(provider).as_str() {
        "gemini" => "https://generativelanguage.googleapis.com/v1beta",
        "claude" => "https://api.anthropic.com/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "doubao" => "https://ark.cn-beijing.volces.com/api/v3",
        "custom" => "https://api.vectorengine.cn/v1",
        "packy" => "https://www.packyapi.ai/v1",
        _ => "https://api.openai.com/v1",
    }
}

fn preferred_models_for_provider(provider: &str) -> &'static [&'static str] {
    match normalize_ai_provider(provider).as_str() {
        "gemini" => &[
            "gemini-3-flash-preview",
            "gemini-3-pro-preview",
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-2.5-flash-lite",
            "gemini-2.0-flash",
        ],
        "claude" => &[
            "claude-3-7-sonnet-latest",
            "claude-3-5-sonnet-latest",
            "claude-3-5-haiku-latest",
        ],
        "deepseek" => &["deepseek-chat", "deepseek-reasoner"],
        "doubao" => &["doubao-pro-32k", "doubao-lite-32k"],
        "custom" => &["gemini-3.1-pro-preview-thinking"],
        "packy" => &["gemini-3.8-flash"],
        _ => &["gpt-4.1-mini", "gpt-4o-mini", "gpt-4.1", "gpt-4o"],
    }
}

fn sort_models(ids: &mut Vec<String>, preferred: &[&str]) {
    ids.sort_by(|left, right| {
        let left_rank = preferred
            .iter()
            .position(|item| item.eq_ignore_ascii_case(left))
            .unwrap_or(preferred.len());
        let right_rank = preferred
            .iter()
            .position(|item| item.eq_ignore_ascii_case(right))
            .unwrap_or(preferred.len());
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
    });
    ids.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

fn is_supported_text_model(provider: &str, model_id: &str) -> bool {
    let id = model_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return false;
    }
    let blocked_keywords = [
        "embedding",
        "embed",
        "image",
        "vision",
        "tts",
        "speech",
        "whisper",
        "audio",
        "transcribe",
        "moderation",
        "rerank",
        "search",
    ];
    if blocked_keywords.iter().any(|keyword| id.contains(keyword)) {
        return false;
    }
    if provider == "gemini" && !id.starts_with("gemini") {
        return false;
    }
    true
}

async fn chat_completion_json_openai(
    client: &Client,
    config: &AppConfig,
    messages: Vec<Value>,
    schema_name: &str,
    schema: Value,
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<Value, String> {
    let provider_name = ai_provider_display_name(config);
    let base_url = normalize_base_url_for_provider(&config.ai_provider, &config.openai_base_url);
    let base_url = base_url.trim_end_matches('/');
    let mut request_body = json!({
        "model": config.openai_model,
        "messages": messages,
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": schema_name,
                "strict": true,
                "schema": schema
            }
        }
    });
    configure_openai_request(&mut request_body, config, reporter.is_some());
    if let Some(reporter) = reporter {
        let response = client
            .post(format!("{}/chat/completions", base_url))
            .bearer_auth(config.openai_api_key.trim())
            .json(&request_body)
            .send()
            .await
            .map_err(|error| format!("{} request failed: {}", provider_name, error))?;
        let status = response.status();
        if status != StatusCode::OK {
            let body = response
                .text()
                .await
                .map_err(|error| format!("Failed to read {} response: {}", provider_name, error))?;
            return Err(format!(
                "{} request failed: HTTP {} {}",
                provider_name, status, body
            ));
        }

        let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()).unwrap_or_default().to_string();
        let mut stream = response.bytes_stream();
        let mut parser = reliability::OpenAiStream::default();
        let result = async {
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| format!("{} stream failed: {}", provider_name, error))?;
                for delta in parser.push(&chunk)? {
                    reporter.delta(&delta, current, total);
                }
            }
            parser.finish()
        }.await;
        let mut diagnostic = parser.diagnostics();
        diagnostic["httpStatus"] = json!(status.as_u16());
        diagnostic["contentType"] = json!(content_type);
        diagnostic["model"] = json!(config.openai_model);
        diagnostic["batch"] = json!(current);
        diagnostic["success"] = json!(result.is_ok());
        let log_result = save_stream_diagnostic(&diagnostic);
        let parsed = result.map_err(|error| match &log_result {
            Ok(path) => format!("{} Diagnostic: {}", error, path.display()),
            Err(log_error) => format!("{} Diagnostic logging failed: {}", error, log_error),
        })?;
        if !parser.usage.is_null() {
            let (prompt, completion, total_tokens) =
                update_ai_usage_from_response(&parser.usage, provider_name, &config.openai_model);
            reporter.emit_usage(prompt, completion, total_tokens, current, total);
        }
        return Ok(parsed);
    }
    let response = client
        .post(format!("{}/chat/completions", base_url))
        .bearer_auth(config.openai_api_key.trim())
        .json(&request_body)
        .send()
        .await
        .map_err(|error| format!("{} request failed: {}", provider_name, error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read {} response: {}", provider_name, error))?;
    if status != StatusCode::OK {
        return Err(format!(
            "{} request failed: HTTP {} {}",
            provider_name, status, body
        ));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse {} JSON: {}", provider_name, error))?;
    if json
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        != Some("stop")
    {
        return Err("AI response did not finish successfully; output will not be written.".into());
    }
    let (prompt_tokens, completion_tokens, total_tokens) = update_ai_usage_from_response(
        &json,
        ai_provider_display_name(config),
        &config.openai_model,
    );
    if let Some(reporter) = reporter {
        reporter.emit_usage(
            prompt_tokens,
            completion_tokens,
            total_tokens,
            current,
            total,
        );
    }
    let content = json
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{} content missing in response: {}", provider_name, body))?;
    serde_json::from_str::<Value>(content).map_err(|error| {
        format!(
            "Failed to parse structured JSON from {}: {}",
            provider_name, error
        )
    })
}

async fn chat_completion_json_gemini(
    client: &Client,
    config: &AppConfig,
    messages: Vec<Value>,
    schema: Value,
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<Value, String> {
    let base_url = config.openai_base_url.trim_end_matches('/');
    let prompt = messages_to_text(&messages);
    let request_body = json!({
        "contents": [
            {
                "parts": [
                    { "text": prompt }
                ]
            }
        ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": schema
        }
    });
    if let Some(reporter) = reporter {
        let response = client
            .post(format!(
                "{}/models/{}:streamGenerateContent?alt=sse&key={}",
                base_url,
                config.openai_model.trim(),
                config.openai_api_key.trim()
            ))
            .json(&request_body)
            .send()
            .await
            .map_err(|error| format!("Gemini request failed: {}", error))?;
        let status = response.status();
        if status != StatusCode::OK {
            let body = response
                .text()
                .await
                .map_err(|error| format!("Failed to read Gemini response: {}", error))?;
            return Err(format!("Gemini request failed: HTTP {} {}", status, body));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut decoder = reliability::Utf8Stream::default();
        let mut collected = String::new();
        let mut usage_json = Value::Null;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("Gemini stream failed: {}", error))?;
            let text = decoder.push(&chunk)?;
            buffer.push_str(&text);
            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim().to_string();
                buffer = buffer[index + 1..].to_string();
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }
                    let json: Value = serde_json::from_str(data).map_err(|error| {
                        format!("Failed to parse Gemini stream JSON: {}", error)
                    })?;
                    let delta = json
                        .get("candidates")
                        .and_then(Value::as_array)
                        .and_then(|candidates| candidates.first())
                        .and_then(|candidate| candidate.get("content"))
                        .and_then(|content| content.get("parts"))
                        .and_then(Value::as_array)
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part.get("text").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .unwrap_or_default();
                    if !delta.is_empty() {
                        collected.push_str(&delta);
                        reporter.delta(&delta, current, total);
                    }
                    if json.get("usageMetadata").is_some() {
                        usage_json = json;
                    }
                }
            }
        }
        decoder.finish()?;
        if !usage_json.is_null() {
            let (prompt_tokens, completion_tokens, total_tokens) = update_ai_usage_from_response(
                &usage_json,
                ai_provider_display_name(config),
                &config.openai_model,
            );
            reporter.emit_usage(
                prompt_tokens,
                completion_tokens,
                total_tokens,
                current,
                total,
            );
        }
        return parse_streamed_json(&collected, "Gemini");
    }
    let response = client
        .post(format!(
            "{}/models/{}:generateContent?key={}",
            base_url,
            config.openai_model.trim(),
            config.openai_api_key.trim()
        ))
        .json(&request_body)
        .send()
        .await
        .map_err(|error| format!("Gemini request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Gemini response: {}", error))?;
    if status != StatusCode::OK {
        return Err(format!("Gemini request failed: HTTP {} {}", status, body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse Gemini JSON: {}", error))?;
    let (prompt_tokens, completion_tokens, total_tokens) = update_ai_usage_from_response(
        &json,
        ai_provider_display_name(config),
        &config.openai_model,
    );
    if let Some(reporter) = reporter {
        reporter.emit_usage(
            prompt_tokens,
            completion_tokens,
            total_tokens,
            current,
            total,
        );
    }
    let text = json
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| {
            let joined = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        })
        .ok_or_else(|| format!("Gemini content missing in response: {}", body))?;
    serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("Failed to parse structured JSON from Gemini: {}", error))
}

async fn chat_completion_json_claude(
    client: &Client,
    config: &AppConfig,
    messages: Vec<Value>,
    reporter: Option<&StreamReporter>,
    current: usize,
    total: usize,
) -> Result<Value, String> {
    let base_url = config.openai_base_url.trim_end_matches('/');
    let system = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let user_text = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) != Some("system"))
        .filter_map(|message| message.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let request_body = json!({
        "model": config.openai_model,
        "max_tokens": 4096,
        "system": system,
        "messages": [
            {
                "role": "user",
                "content": user_text
            }
        ]
    });
    if let Some(reporter) = reporter {
        let request_body = json!({
            "model": config.openai_model,
            "max_tokens": 4096,
            "system": system,
            "stream": true,
            "messages": [
                {
                    "role": "user",
                    "content": user_text
                }
            ]
        });
        let response = client
            .post(format!("{}/messages", base_url))
            .header("x-api-key", config.openai_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&request_body)
            .send()
            .await
            .map_err(|error| format!("Claude request failed: {}", error))?;
        let status = response.status();
        if status != StatusCode::OK {
            let body = response
                .text()
                .await
                .map_err(|error| format!("Failed to read Claude response: {}", error))?;
            return Err(format!("Claude request failed: HTTP {} {}", status, body));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut decoder = reliability::Utf8Stream::default();
        let mut current_event = String::new();
        let mut collected = String::new();
        let mut usage_json = Value::Null;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("Claude stream failed: {}", error))?;
            let text = decoder.push(&chunk)?;
            buffer.push_str(&text);
            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim().to_string();
                buffer = buffer[index + 1..].to_string();
                if let Some(event_name) = line.strip_prefix("event:") {
                    current_event = event_name.trim().to_string();
                    continue;
                }
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let json: Value = serde_json::from_str(data).map_err(|error| {
                        format!("Failed to parse Claude stream JSON: {}", error)
                    })?;
                    if current_event == "content_block_delta" {
                        if let Some(delta) = json
                            .get("delta")
                            .and_then(|delta| delta.get("text"))
                            .and_then(Value::as_str)
                        {
                            collected.push_str(delta);
                            reporter.delta(delta, current, total);
                        }
                    }
                    if json.get("usage").is_some()
                        || json
                            .get("message")
                            .and_then(|message| message.get("usage"))
                            .is_some()
                    {
                        usage_json = json;
                    }
                }
            }
        }
        decoder.finish()?;
        if !usage_json.is_null() {
            let (prompt_tokens, completion_tokens, total_tokens) = update_ai_usage_from_response(
                &usage_json,
                ai_provider_display_name(config),
                &config.openai_model,
            );
            reporter.emit_usage(
                prompt_tokens,
                completion_tokens,
                total_tokens,
                current,
                total,
            );
        }
        return parse_streamed_json(&collected, "Claude");
    }
    let response = client
        .post(format!("{}/messages", base_url))
        .header("x-api-key", config.openai_api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .json(&request_body)
        .send()
        .await
        .map_err(|error| format!("Claude request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Claude response: {}", error))?;
    if status != StatusCode::OK {
        return Err(format!("Claude request failed: HTTP {} {}", status, body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse Claude JSON: {}", error))?;
    let (prompt_tokens, completion_tokens, total_tokens) = update_ai_usage_from_response(
        &json,
        ai_provider_display_name(config),
        &config.openai_model,
    );
    if let Some(reporter) = reporter {
        reporter.emit_usage(
            prompt_tokens,
            completion_tokens,
            total_tokens,
            current,
            total,
        );
    }
    let text = json
        .get("content")
        .and_then(Value::as_array)
        .and_then(|parts| {
            let joined = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        })
        .ok_or_else(|| format!("Claude content missing in response: {}", body))?;
    serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("Failed to parse structured JSON from Claude: {}", error))
}

fn parse_streamed_json(text: &str, provider: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(text).map_err(|error| {
        format!(
            "Failed to parse structured JSON from {} stream: {}. Raw output: {}",
            provider, error, text
        )
    })
}

fn messages_to_text(messages: &[Value]) -> String {
    messages
        .iter()
        .map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_uppercase();
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("{}:\n{}", role, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn ai_provider_display_name(config: &AppConfig) -> &'static str {
    match config.ai_provider.trim().to_ascii_lowercase().as_str() {
        "gemini" => "Gemini",
        "packy" => "PackyAPI (OpenAI)",
        "claude" => "Claude",
        "deepseek" => "DeepSeek",
        "doubao" => "Doubao",
        "custom" => "Custom LLM",
        _ => "OpenAI",
    }
}

fn extract_ai_usage_from_response(json: &Value) -> (usize, usize, usize) {
    let prompt_tokens = json
        .get("usage")
        .and_then(|usage| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            json.get("usage")
                .and_then(|usage| usage.get("input_tokens"))
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            json.get("usageMetadata")
                .and_then(|usage| usage.get("promptTokenCount"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0) as usize;
    let completion_tokens = json
        .get("usage")
        .and_then(|usage| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            json.get("usage")
                .and_then(|usage| usage.get("output_tokens"))
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            json.get("usageMetadata")
                .and_then(|usage| usage.get("candidatesTokenCount"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0) as usize;
    let total_tokens = json
        .get("usage")
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            json.get("usageMetadata")
                .and_then(|usage| usage.get("totalTokenCount"))
                .and_then(Value::as_u64)
        })
        .unwrap_or((prompt_tokens + completion_tokens) as u64) as usize;
    (prompt_tokens, completion_tokens, total_tokens)
}

fn update_ai_usage_from_response(
    json: &Value,
    provider: &str,
    model: &str,
) -> (usize, usize, usize) {
    let (prompt_tokens, completion_tokens, total_tokens) = extract_ai_usage_from_response(json);

    let usage = AI_USAGE.get_or_init(|| Mutex::new(AiUsageSnapshot::default()));
    if let Ok(mut snapshot) = usage.lock() {
        snapshot.request_count += 1;
        snapshot.prompt_tokens += prompt_tokens;
        snapshot.completion_tokens += completion_tokens;
        snapshot.total_tokens += total_tokens;
        snapshot.provider = provider.to_string();
        snapshot.model = model.to_string();
        snapshot.last_updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_default();
    }
    (prompt_tokens, completion_tokens, total_tokens)
}

fn ensure_feishu_config(config: &AppConfig) -> Result<(), String> {
    if config.feishu_app_id.trim().is_empty() || config.feishu_app_secret.trim().is_empty() {
        return Err("Missing Feishu App ID or App Secret.".into());
    }
    Ok(())
}

fn ensure_ai_config(config: &AppConfig) -> Result<(), String> {
    if config.openai_api_key.trim().is_empty() {
        return Err(format!(
            "Missing {} API key.",
            ai_provider_display_name(config)
        ));
    }
    if config.openai_model.trim().is_empty() {
        return Err(format!(
            "Missing {} model.",
            ai_provider_display_name(config)
        ));
    }
    if normalize_ai_provider(&config.ai_provider) == "custom"
        && normalize_base_url_for_provider(&config.ai_provider, &config.openai_base_url)
            .to_ascii_lowercase()
            .contains("api.vectorengine.cn")
    {
        let api_key = config.openai_api_key.trim();
        if api_key.starts_with("AIza") {
            return Err(
                "Custom LLM is using a Google Gemini API key. Paste the VectorEngine third-party API key that starts with sk-."
                    .into(),
            );
        }
        if let Some(key_body) = api_key.strip_prefix("sk-") {
            if key_body.contains('-') {
                return Err(
                    "Custom LLM API key looks like it includes a channel suffix after the token. VectorEngine ordinary users cannot specify a channel; remove the extra suffix after the real sk- key."
                        .into(),
                );
            }
        }
    }
    Ok(())
}

fn collect_source_samples(rows: &[Row], source_column_id: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    rows.iter()
        .filter_map(|row| row.values.get(source_column_id))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert((*value).to_string()))
        .take(limit)
        .map(ToOwned::to_owned)
        .collect()
}

fn analyze_source_samples(samples: &[String]) -> String {
    if samples.is_empty() {
        return "No source samples available.".to_string();
    }

    let rich_tag_regex = Regex::new(r"<[^>\n]+>").expect("valid rich tag regex");
    let brace_regex = Regex::new(r"\{[^{}\n]+\}").expect("valid brace regex");

    let total = samples.len();
    let total_chars = samples
        .iter()
        .map(|text| text.chars().count())
        .sum::<usize>();
    let short_count = samples
        .iter()
        .filter(|text| text.chars().count() <= 16)
        .count();
    let medium_count = samples
        .iter()
        .filter(|text| {
            let len = text.chars().count();
            (17..=40).contains(&len)
        })
        .count();
    let long_count = total.saturating_sub(short_count + medium_count);
    let rich_tag_count = samples
        .iter()
        .filter(|text| rich_tag_regex.is_match(text))
        .count();
    let brace_count = samples
        .iter()
        .filter(|text| brace_regex.is_match(text))
        .count();
    let escaped_newline_count = samples.iter().filter(|text| text.contains("\\n")).count();
    let real_newline_count = samples.iter().filter(|text| text.contains('\n')).count();
    let digit_count = samples
        .iter()
        .filter(|text| text.chars().any(|ch| ch.is_ascii_digit()))
        .count();
    let ui_like_count = samples
        .iter()
        .filter(|text| text.chars().count() <= 12 && !text.contains('，') && !text.contains('。'))
        .count();
    let descriptive_count = samples
        .iter()
        .filter(|text| {
            text.contains('，') || text.contains('。') || text.contains(':') || text.contains('：')
        })
        .count();

    format!(
        "sample_count: {}\navg_chars: {:.1}\nshort_texts(<=16 chars): {}\nmedium_texts(17-40 chars): {}\nlong_texts(>40 chars): {}\ncontains_rich_tags: {}\ncontains_brace_placeholders: {}\ncontains_escaped_newlines: {}\ncontains_real_newlines: {}\ncontains_digits: {}\nui_like_short_labels: {}\ndescriptive_sentences: {}",
        total,
        total_chars as f32 / total as f32,
        short_count,
        medium_count,
        long_count,
        rich_tag_count,
        brace_count,
        escaped_newline_count,
        real_newline_count,
        digit_count,
        ui_like_count,
        descriptive_count
    )
}

fn build_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(120))
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|error| format!("Failed to build HTTP client: {}", error))
}

fn parse_sheet_infos(metainfo: &Value) -> Result<Vec<SheetInfo>, String> {
    let sheets = metainfo
        .get("data")
        .and_then(|data| data.get("sheets"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Spreadsheet sheets metadata is missing.".to_string())?;
    Ok(sheets
        .iter()
        .filter_map(|sheet| {
            let sheet_id = sheet
                .get("sheetId")
                .or_else(|| sheet.get("sheet_id"))
                .and_then(Value::as_str)?;
            let sheet_name = sheet
                .get("title")
                .or_else(|| sheet.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(sheet_id);
            Some(SheetInfo {
                sheet_id: sheet_id.to_string(),
                sheet_name: sheet_name.to_string(),
            })
        })
        .collect())
}

fn normalize_column_id(header: &str, index: usize) -> String {
    let trimmed = header.trim();
    let lower = trimmed.to_lowercase();
    let mapped = match trimmed {
        "中文" | "简体中文" | "简中" => "chinese".to_string(),
        "英文" => "english".to_string(),
        "日文" | "日语" => "japanese".to_string(),
        "韩文" | "韩语" => "korean".to_string(),
        "备注" => "notes".to_string(),
        _ => match lower.as_str() {
            "key" => "key".to_string(),
            "chinese" => "chinese".to_string(),
            "english" => "english".to_string(),
            "japanese" => "japanese".to_string(),
            "korean" => "korean".to_string(),
            "notes" => "notes".to_string(),
            _ => lower
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character
                    } else if character.is_whitespace() {
                        '_'
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .trim_matches('_')
                .to_string(),
        },
    };
    if mapped.is_empty() {
        format!("column_{}", index + 1)
    } else {
        mapped
    }
}

fn collect_multi_target_translation_items(
    rows: &[Row],
    source_column_id: &str,
    target_columns: &[TargetColumnInfo],
    context_columns: &[Column],
    write_mode: &str,
) -> Vec<MultiTargetBatchItem> {
    let include_existing_translations = write_mode_uses_existing_translations(write_mode);
    rows.iter()
        .filter_map(|row| {
            let source = row.values.get(source_column_id)?.trim().to_string();
            if source.is_empty() {
                return None;
            }
            let mut missing_targets = Vec::new();
            let mut existing_translations = HashMap::new();
            for column in target_columns {
                if !row
                    .available_column_ids
                    .iter()
                    .any(|value| value == &column.id)
                {
                    continue;
                }
                let current = row.values.get(&column.id).cloned().unwrap_or_default();
                if include_existing_translations && !current.trim().is_empty() {
                    existing_translations.insert(column.id.clone(), current.clone());
                }
                if write_mode == "fill_empty" && !current.trim().is_empty() {
                    continue;
                }
                missing_targets.push(column.clone());
            }
            if missing_targets.is_empty() {
                return None;
            }
            let auxiliary_context = context_columns
                .iter()
                .filter_map(|column| {
                    row.values
                        .get(&column.id)
                        .and_then(|value| trimmed_context_value(value))
                        .map(|value| (column.name.clone(), value))
                })
                .collect::<HashMap<_, _>>();
            Some(MultiTargetBatchItem {
                row_id: row.row_id.clone(),
                source,
                missing_targets,
                existing_translations,
                auxiliary_context,
            })
        })
        .collect()
}

fn to_excel_column_name(index: usize) -> String {
    let mut result = String::new();
    let mut value = index + 1;
    while value > 0 {
        let remainder = (value - 1) % 26;
        result.insert(0, char::from_u32(65 + remainder as u32).unwrap_or('A'));
        value = (value - 1) / 26;
    }
    result
}

fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        _ => value.to_string(),
    }
}

fn parse_spreadsheet_id(link: &str) -> Result<String, String> {
    let marker = "/sheets/";
    let start = link
        .find(marker)
        .ok_or_else(|| "Spreadsheet token not found in link.".to_string())?
        + marker.len();
    let tail = &link[start..];
    let end = tail
        .find(|character: char| character == '?' || character == '/' || character == '#')
        .unwrap_or(tail.len());
    let id = tail[..end].trim();
    if id.is_empty() {
        return Err("Spreadsheet token is empty.".into());
    }
    Ok(id.to_string())
}

fn configure_openai_request(body: &mut Value, config: &AppConfig, stream: bool) {
    // Gemini's OpenAI-compatible API maps high to its highest thinking level.
    if config.openai_model.trim() == "gemini-3.8-flash" {
        body["reasoning_effort"] = json!("high");
    }
    if stream {
        body["stream"] = json!(true);
        body["stream_options"] = json!({ "include_usage": true });
    }
}

#[cfg(test)]
mod packy_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Uses the local PackyAPI credential and makes two billable requests"]
    async fn live_packy_structured_output_in_both_modes() {
        let config = load_local_config().expect("Local configuration required");
        assert_eq!(
            config.openai_base_url.trim_end_matches('/'),
            "https://www.packyapi.ai/v1"
        );
        assert_eq!(config.openai_model, "gemini-3.8-flash");
        let client = build_http_client().unwrap();
        let messages = vec![
            json!({"role":"user","content":"Return JSON with text equal to 蚁巢・日本語🐜. Do not translate it."}),
        ];
        let schema = json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false});
        let result = chat_completion_json(
            &client,
            &config,
            messages.clone(),
            "unicode_smoke",
            schema.clone(),
            None,
            1,
            1,
        )
        .await
        .unwrap();
        assert_eq!(result["text"], "蚁巢・日本語🐜");
        let mut body = json!({"model":config.openai_model,"messages":messages,
            "response_format":{"type":"json_schema","json_schema":{"name":"unicode_smoke","strict":true,"schema":schema}}});
        configure_openai_request(&mut body, &config, true);
        let response = client
            .post(format!(
                "{}/chat/completions",
                config.openai_base_url.trim_end_matches('/')
            ))
            .bearer_auth(config.openai_api_key.trim())
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "HTTP {}", response.status());
        let mut stream = response.bytes_stream();
        let mut parser = reliability::OpenAiStream::default();
        while let Some(chunk) = stream.next().await {
            parser.push(&chunk.unwrap()).unwrap();
        }
        assert_eq!(parser.finish().unwrap()["text"], "蚁巢・日本語🐜");
        assert!(
            parser
                .usage
                .pointer("/usage/total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
        );
    }

    #[test]
    fn failure_states_never_report_success() {
        assert_eq!(translation_outcome(5, 0, 0), "failed");
        assert_eq!(translation_outcome(5, 1, 0), "partial");
        assert_eq!(translation_outcome(0, 1, 0), "unchanged");
        assert_eq!(translation_outcome(0, 1, 2), "complete");
    }

    #[test]
    fn missing_translations_fail_and_unrequested_columns_are_ignored() {
        let expected = vec![MultiTargetBatchItem {
            row_id: "r1".into(),
            source: "蚁巢".into(),
            missing_targets: vec![TargetColumnInfo {
                id: "english".into(),
                name: "English".into(),
            }],
            existing_translations: HashMap::new(),
            auxiliary_context: HashMap::new(),
        }];
        for parsed in [
            json!({"items":[]}),
            json!({"items":[{"rowId":"r1","translations":{}}]}),
            json!({"items":[{"rowId":"r1","translations":{"english":""}}]}),
            json!({"items":[{"rowId":"other","translations":{"english":"Ant"}}]}),
        ] {
            assert!(parse_translation_batch(&parsed, &expected).is_err());
        }
        let parsed = json!({"items":[{"rowId":"r1","translations":{"english":"Ant nest","japanese":"must not overwrite"}}]});
        let result = parse_translation_batch(&parsed, &expected).unwrap();
        assert_eq!(result["r1"].len(), 1);
        assert_eq!(result["r1"]["english"], "Ant nest");
    }

    #[test]
    fn packy_preset_and_manual_model_survive_catalog_refresh() {
        assert_eq!(normalize_ai_provider("packy"), "packy");
        assert_eq!(default_ai_base_url("packy"), "https://www.packyapi.ai/v1");
        assert_eq!(
            pick_preferred_model("gemini-3.8-flash", &["gpt-4.1"], &["gpt-4.1"]),
            "gemini-3.8-flash"
        );
    }

    #[test]
    fn highest_thinking_preserves_structured_output_in_both_modes() {
        let config = default_project_config();
        for stream in [false, true] {
            let mut body = json!({"messages": [{"role": "user", "content": "test"}],
                "response_format": {"type": "json_schema"}});
            configure_openai_request(&mut body, &config, stream);
            assert_eq!(body["reasoning_effort"], "high");
            assert_eq!(body["response_format"]["type"], "json_schema");
            assert_eq!(body["messages"][0]["content"], "test");
            assert_eq!(body["stream"].as_bool().unwrap_or(false), stream);
            if stream {
                assert_eq!(body["stream_options"]["include_usage"], true);
            }
        }
    }

    #[test]
    fn other_models_keep_existing_reasoning_behavior() {
        let mut config = default_project_config();
        config.openai_model = "gpt-4.1-mini".into();
        let mut body = json!({});
        configure_openai_request(&mut body, &config, true);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn packy_url_uses_one_v1_prefix() {
        for url in ["https://www.packyapi.ai", "https://www.packyapi.ai/v1/"] {
            assert_eq!(
                normalize_base_url_for_provider("openai", url),
                "https://www.packyapi.ai/v1"
            );
        }
    }
}

fn save_stream_diagnostic(diagnostic: &Value) -> Result<PathBuf, String> {
    use std::io::Write;
    let directory = local_config_path()?.parent().unwrap().join("diagnostics");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("streams.jsonl");
    let mut record = diagnostic.clone();
    record["unixTimeMs"] = json!(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{}", record).map_err(|error| error.to_string())?;
    Ok(path)
}

fn local_config_path() -> Result<PathBuf, String> {
    let app_data =
        std::env::var("APPDATA").map_err(|error| format!("APPDATA is not available: {}", error))?;
    Ok(PathBuf::from(app_data)
        .join("FeishuLocStudio")
        .join("config.json"))
}

fn local_workflow_path() -> Result<PathBuf, String> {
    let app_data =
        std::env::var("APPDATA").map_err(|error| format!("APPDATA is not available: {}", error))?;
    Ok(PathBuf::from(app_data)
        .join("FeishuLocStudio")
        .join("workflow.json"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            load_local_config,
            save_local_config,
            load_local_workflow,
            save_local_workflow,
            export_workspace,
            import_workspace,
            detect_ai_provider,
            fetch_available_models,
            get_ai_usage,
            load_sheet_preview,
            generate_glossary,
            fill_glossary_translations,
            sync_glossary_term,
            write_glossary_to_feishu,
            generate_translation_prompt,
            run_translation,
            stop_translation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
