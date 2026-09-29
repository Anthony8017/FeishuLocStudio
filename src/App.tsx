import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

declare const __APP_VERSION__: string;

type UiLanguage = "zh-CN" | "en-US";
type AiProvider = "packy" | "openai" | "gemini" | "claude" | "deepseek" | "doubao" | "custom";
type WriteMode = "fill_empty" | "overwrite_unlocked";
type BusyState =
  | "idle"
  | "loading"
  | "glossary"
  | "prompt"
  | "translating"
  | "rollback"
  | "saving"
  | "detecting"
  | "models";
type ColumnRole = "source" | "target" | "locked";

type AppConfig = {
  feishuAppId: string;
  feishuAppSecret: string;
  aiProvider: AiProvider;
  openaiApiKey: string;
  openaiBaseUrl: string;
  openaiModel: string;
  uiLanguage: UiLanguage;
};

type Column = {
  id: string;
  name: string;
  sample: string;
  sheetCount: number;
};

type Row = {
  rowId: string;
  rowNumber: number;
  sheetId: string;
  sheetName: string;
  availableColumnIds: string[];
  values: Record<string, string>;
};

type SheetPreview = {
  spreadsheetId: string;
  sheetIds: string[];
  sheetNames: string[];
  columns: Column[];
  rows: Row[];
};

type LoadSheetResponse = {
  preview: SheetPreview;
  readSuccess: boolean;
};

type GlossaryTerm = {
  glossaryId?: string;
  term: string;
  category: string;
  note: string;
  enabled: boolean;
  sourceSheet?: string;
  rowId?: string;
  rowNumber?: number;
  sheetId?: string;
  translations: Record<string, string>;
  localTargetClears?: string[];
};

type GlossaryResponse = {
  terms: GlossaryTerm[];
  rows?: Row[];
  liveSyncDenied?: boolean;
  syncMessage?: string;
};

type GlossarySyncResponse = {
  term?: GlossaryTerm | null;
  row?: Row | null;
  removed: boolean;
  permissionDenied?: boolean;
  message?: string;
};

type GlossaryPatchEvent = {
  task: string;
  message: string;
  terms: GlossaryTerm[];
  rows: Row[];
  removedGlossaryIds: string[];
};

type DetectAiProviderResponse = {
  aiProvider: AiProvider;
  baseUrl: string;
  suggestedModel: string;
};

type AvailableModelsResponse = {
  aiProvider: AiProvider;
  baseUrl: string;
  models: string[];
  selectedModel: string;
};

type AiUsageSnapshot = {
  requestCount: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  provider: string;
  model: string;
  lastUpdatedAt: string;
};

type WorkflowState = {
  sheetLink: string;
  lockedColumns: string[];
  targetColumns: string[];
  sourceColumnId: string;
  writeMode: WriteMode;
  translationTokenBudget?: number;
  glossary: GlossaryTerm[];
  prompt: string;
  glossarySheet: string;
  glossaryColumnWidths: Record<string, number>;
  glossaryLocalOnlyMode?: boolean;
  sheetState?: SheetPreview | null;
  readSuccess?: boolean;
  summary?: TranslationSummary | null;
  history?: RunHistory[];
  collapsedPanels?: Record<string, boolean>;
};

type WorkspaceImportResponse = {
  appConfig: AppConfig;
  workflow: WorkflowState;
  path: string;
};

type PromptSuggestionResponse = {
  prompt: string;
};

type TranslationSummary = {
  changedCells: number;
  processedRows: number;
  skippedRows: number;
  failedRows: string[];
  failedRowIds?: string[];
  status?: "complete" | "partial" | "failed" | "unchanged";
};

type TranslationResponse = {
  rows: Row[];
  summary: TranslationSummary;
};

type RunHistory = {
  id: string;
  timestamp: string;
  mode: WriteMode;
  processedRows: number;
  failedRows: number;
};

type Toast = {
  tone: "success" | "warning" | "error";
  message: string;
};

type LlmStreamEvent = {
  task: string;
  phase: string;
  message: string;
  delta: string;
  debugPrompt: string;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  current: number;
  total: number;
};

type UsageLogEntry = {
  id: string;
  task: string;
  current: number;
  total: number;
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
};

type FillEmptyPreviewItem = {
  id: string;
  key: string;
  sheetName: string;
  rowNumber: number;
  source: string;
  missingTargets: string[];
};

type MessageSet = {
  connection: string;
  currentTask: string;
  language: string;
  feishuAppId: string;
  feishuAppSecret: string;
  openaiApiKey: string;
  openaiBaseUrl: string;
  model: string;
  exportWorkspace: string;
  importWorkspace: string;
  workspaceConfig: string;
  saving: string;
  sheets: string;
  columns: string;
  targets: string;
  source: string;
  sourceEmpty: string;
  workflow: string;
  heroTitle: string;
  heroCopy: string;
  success: string;
  notLoaded: string;
  readStatus: string;
  childSheets: string;
  step1: string;
  readSpreadsheet: string;
  loading: string;
  readFeishuSpreadsheet: string;
  feishuLink: string;
  readResult: string;
  loadedSuccessfully: string;
  waiting: string;
  rows: string;
  uniqueHeaders: string;
  step2: string;
  columnRoles: string;
  column: string;
  sample: string;
  sheetsShort: string;
  role: string;
  sourceRole: string;
  targetRole: string;
  lockedRole: string;
  configureRolesFirst: string;
  step3: string;
  glossary: string;
  generating: string;
  generateGlossary: string;
  addTerm: string;
  enabled: string;
  remove: string;
  sourceTerm: string;
  category: string;
  note: string;
  noGlossary: string;
  step4: string;
  translate: string;
  generatePrompt: string;
  translating: string;
  translateAndWriteBack: string;
  fillEmptyOnly: string;
  overwriteUnlocked: string;
  prompt: string;
  sourceColumn: string;
  targetColumns: string;
  writeMode: string;
  result: string;
  summaryAndRollback: string;
  rollback: string;
  processedRows: string;
  skippedRows: string;
  failedRows: string;
  noSummary: string;
  recentRuns: string;
  noRuns: string;
  general: string;
  failedToLoadLocalConfig(error: string): string;
  workspaceExported: (path: string) => string;
  workspaceImported: (path: string) => string;
  fillFeishuFirst: string;
  fillOpenAiFirst: string;
  chooseSourceFirst: string;
  chooseTargetFirst: string;
  loadedSheets(count: number): string;
  glossaryGenerated: string;
  promptGenerated: string;
  translationDone: string;
  noSnapshot: string;
  rollbackDone: string;
  historyLine(mode: string, processed: number, failed: number): string;
};

const DEFAULT_LINK =
  "https://chillyroom.feishu.cn/sheets/NOLxsHcwyh0K8ktTculcc6WOnfd";
const APP_VERSION = __APP_VERSION__;
const CUSTOM_LLM_BASE_URL = "https://api.vectorengine.cn/v1";
const CUSTOM_LLM_MODEL = "gemini-3.1-pro-preview-thinking";

const DEFAULT_CONFIG: AppConfig = {
  feishuAppId: "",
  feishuAppSecret: "",
  aiProvider: "packy",
  openaiApiKey: "",
  openaiBaseUrl: "https://www.packyapi.ai/v1",
  openaiModel: "gemini-3.8-flash",
  uiLanguage: "zh-CN",
};

const PROVIDER_OPTIONS: Array<{ value: AiProvider; label: string }> = [
  { value: "packy", label: "PackyAPI（OpenAI 兼容）" },
  { value: "openai", label: "OpenAI" },
  { value: "gemini", label: "Gemini（推荐）" },
  { value: "claude", label: "Claude" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "doubao", label: "Doubao" },
  { value: "custom", label: "Custom LLM" },
];
const PROVIDER_DEFAULT_BASE_URL: Record<AiProvider, string> = {
  packy: "https://www.packyapi.ai/v1",
  openai: "https://api.openai.com/v1",
  gemini: "https://generativelanguage.googleapis.com/v1beta",
  claude: "https://api.anthropic.com/v1",
  deepseek: "https://api.deepseek.com/v1",
  doubao: "https://operator.las.cn-beijing.volces.com/api/v1",
  custom: CUSTOM_LLM_BASE_URL,
};
const ALL_SHEETS_OPTION = "__all_sheets__";
const MODEL_OPTIONS: Record<AiProvider, string[]> = {
  packy: ["gemini-3.8-flash"],
  openai: [ "gpt-4.1-mini", "gpt-4.1", "gpt-4o-mini", "gpt-4o"],
  gemini: [
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.0-flash",
  ],
  claude: ["claude-sonnet-4-6", "claude-opus-4-6", "claude-haiku-4-5"],
  deepseek: ["deepseek-chat", "deepseek-reasoner"],
  doubao: ["doubao-1-5-pro-32k-250115"],
  custom: [CUSTOM_LLM_MODEL],
};
const LANGUAGE_OPTIONS: Array<{ value: UiLanguage; label: string }> = [
  { value: "zh-CN", label: "中文" },
  { value: "en-US", label: "English" },
];

const TEXT: Record<UiLanguage, MessageSet> = {
  "zh-CN": {
    connection: "连接设置",
    currentTask: "当前任务",
    language: "界面语言",
    feishuAppId: "飞书 App ID",
    feishuAppSecret: "飞书 App Secret",
    openaiApiKey: "",
    openaiBaseUrl: "OpenAI Base URL",
    model: "模型",
    exportWorkspace: "保存工作区",
    importWorkspace: "加载工作区",
    workspaceConfig: "工作区配置",
    saving: "保存中...",
    sheets: "子表数",
    columns: "列数",
    targets: "目标列",
    source: "源列",
    sourceEmpty: "未选择",
    workflow: "流程",
    heroTitle: "读取全部子表，配置列角色，整理术语，然后批量翻译。",
    heroCopy: "列角色互斥：翻译源列只能有一个，目标列和锁定列可以有多个。",
    success: "成功",
    notLoaded: "未读取",
    readStatus: "读取状态",
    childSheets: "子表数量",
    step1: "步骤 1",
    readSpreadsheet: "读取表格",
    loading: "读取中...",
    readFeishuSpreadsheet: "读取飞书表格",
    feishuLink: "飞书链接",
    readResult: "读取结果",
    loadedSuccessfully: "读取成功",
    waiting: "等待中",
    rows: "行数",
    uniqueHeaders: "去重表头数",
    step2: "步骤 2",
    columnRoles: "列配置",
    column: "列名",
    sample: "示例",
    sheetsShort: "子表",
    role: "角色",
    sourceRole: "源列",
    targetRole: "目标列",
    lockedRole: "锁定列",
    configureRolesFirst: "请先读取表格，再配置列角色。",
    step3: "步骤 3",
    glossary: "术语表",
    generating: "生成中...",
    generateGlossary: "AI 整理术语",
    addTerm: "新增术语",
    enabled: "启用",
    remove: "删除",
    sourceTerm: "源术语",
    category: "分类",
    note: "备注",
    noGlossary: "还没有术语表，请先根据选中的源列生成。",
    step4: "步骤 4",
    translate: "执行翻译",
    generatePrompt: "根据源语言生成提示词",
    translating: "翻译中...",
    translateAndWriteBack: "翻译并回填",
    fillEmptyOnly: "仅填空白",
    overwriteUnlocked: "覆盖未锁定列",
    prompt: "提示词",
    sourceColumn: "翻译源列",
    targetColumns: "目标列",
    writeMode: "写入模式",
    result: "结果",
    summaryAndRollback: "结果汇总与回滚",
    rollback: "回滚本地快照",
    processedRows: "处理行数",
    skippedRows: "跳过行数",
    failedRows: "失败行数",
    noSummary: "先执行一次翻译任务，这里才会显示结果汇总。",
    recentRuns: "最近任务",
    noRuns: "还没有执行记录。",
    general: "通用",
    failedToLoadLocalConfig: (error) => `读取本地配置失败：${error}`,
    workspaceExported: (path) => `工作区已导出到：${path}`,
    workspaceImported: (path) => `工作区已导入：${path}`,
    fillFeishuFirst: "请先填写飞书 App ID 和 App Secret。",
    fillOpenAiFirst: "请先填写 OpenAI API Key 和模型。",
    chooseSourceFirst: "请先选择一个翻译源列。",
    chooseTargetFirst: "请至少选择一个目标列。",
    loadedSheets: (count) => `已成功读取 ${count} 个子表。`,
    glossaryGenerated: "术语候选已生成。",
    promptGenerated: "已根据源语言生成提示词。",
    translationDone: "翻译完成，结果已回写飞书。",
    noSnapshot: "当前没有可回滚的本地快照。",
    rollbackDone: "只恢复了本地界面快照，飞书远端数据不会自动回滚。",
    historyLine: (mode, processed, failed) =>
      `${mode} · 处理 ${processed} · 失败 ${failed}`,
  },
  "en-US": {
    connection: "Connection",
    currentTask: "Current Task",
    language: "UI Language",
    feishuAppId: "Feishu App ID",
    feishuAppSecret: "Feishu App Secret",
    openaiApiKey: "",
    openaiBaseUrl: "OpenAI Base URL",
    model: "Model",
    exportWorkspace: "Save Workspace",
    importWorkspace: "Load Workspace",
    workspaceConfig: "Workspace",
    saving: "Saving...",
    sheets: "Sheets",
    columns: "Columns",
    targets: "Targets",
    source: "Source",
    sourceEmpty: "Not Selected",
    workflow: "Workflow",
    heroTitle: "Read all child sheets, configure column roles, build a glossary, then translate.",
    heroCopy: "Column roles are exclusive: only one source column, plus any number of target and locked columns.",
    success: "Success",
    notLoaded: "Not Loaded",
    readStatus: "Read Status",
    childSheets: "Child Sheets",
    step1: "Step 1",
    readSpreadsheet: "Read Spreadsheet",
    loading: "Loading...",
    readFeishuSpreadsheet: "Read Feishu Spreadsheet",
    feishuLink: "Feishu Link",
    readResult: "Read Result",
    loadedSuccessfully: "Loaded Successfully",
    waiting: "Waiting",
    rows: "Rows",
    uniqueHeaders: "Unique Headers",
    step2: "Step 2",
    columnRoles: "Column Roles",
    column: "Column",
    sample: "Sample",
    sheetsShort: "Sheets",
    role: "Role",
    sourceRole: "Source",
    targetRole: "Target",
    lockedRole: "Locked",
    configureRolesFirst: "Read the spreadsheet first to configure column roles.",
    step3: "Step 3",
    glossary: "Glossary",
    generating: "Generating...",
    generateGlossary: "Generate Glossary With AI",
    addTerm: "Add Term",
    enabled: "Enabled",
    remove: "Remove",
    sourceTerm: "Source Term",
    category: "Category",
    note: "Note",
    noGlossary: "No glossary yet. Generate one from the selected source column first.",
    step4: "Step 4",
    translate: "Translate",
    generatePrompt: "Generate Prompt From Source",
    translating: "Translating...",
    translateAndWriteBack: "Translate And Write Back",
    fillEmptyOnly: "Fill Empty Only",
    overwriteUnlocked: "Overwrite Unlocked",
    prompt: "Prompt",
    sourceColumn: "Source Column",
    targetColumns: "Target Columns",
    writeMode: "Write Mode",
    result: "Result",
    summaryAndRollback: "Summary And Rollback",
    rollback: "Roll Back Local Snapshot",
    processedRows: "Processed Rows",
    skippedRows: "Skipped Rows",
    failedRows: "Failed Rows",
    noSummary: "Run one translation job to see the summary here.",
    recentRuns: "Recent Runs",
    noRuns: "No runs yet.",
    general: "General",
    failedToLoadLocalConfig: (error) => `Failed to load local config: ${error}`,
    workspaceExported: (path) => `Workspace exported to: ${path}`,
    workspaceImported: (path) => `Workspace imported from: ${path}`,
    fillFeishuFirst: "Please fill Feishu App ID and App Secret first.",
    fillOpenAiFirst: "Please fill OpenAI API Key and model first.",
    chooseSourceFirst: "Please select one source column.",
    chooseTargetFirst: "Please choose at least one target column.",
    loadedSheets: (count) => `Loaded ${count} sheets successfully.`,
    glossaryGenerated: "Glossary candidates generated.",
    promptGenerated: "Prompt generated from the source language.",
    translationDone: "Translation finished and written back to Feishu.",
    noSnapshot: "No local snapshot to roll back.",
    rollbackDone: "Only the local UI snapshot was restored. Remote Feishu data was not reverted.",
    historyLine: (mode, processed, failed) =>
      `${mode} · processed ${processed} · failed ${failed}`,
  },
};

function pickDefaultSourceColumn(columns: Column[]) {
  const preferred = columns.find(
    (column) => /^(chinese|zh|cn)$/i.test(column.id) || /中文|chinese/i.test(column.name)
  );
  return preferred?.id ?? columns[0]?.id ?? "";
}

function getDefaultColumnRoles(columns: Column[]) {
  const languageColumns = columns.filter(isLanguageColumn);
  const sourceId = languageColumns[0]?.id ?? pickDefaultSourceColumn(columns);
  const targetIds = languageColumns
    .map((column) => column.id)
    .filter((columnId) => columnId !== sourceId);

  return { sourceId, targetIds };
}

function createGlossaryId(seed: string) {
  const normalized = seed.trim().replace(/[^a-zA-Z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return `glossary-${crypto.randomUUID()}${normalized ? `-${normalized}` : ""}`;
}

function isLanguageColumn(column: Column) {
  const splitWords = (value: string) =>
    value
      .replace(/([a-z])([A-Z])/g, "$1 $2")
      .replace(/([A-Z])([A-Z][a-z])/g, "$1 $2")
      .replace(/[_\-/()]+/g, " ")
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean);

  const words = [...splitWords(column.id), ...splitWords(column.name)];
  const joined = words.join(" ");
  const blockedWords = new Set([
    "type",
    "season",
    "desc",
    "description",
    "character",
    "limit",
    "notes",
    "note",
    "remark",
    "comment",
    "status",
    "key",
    "id",
  ]);
  const blockedPatterns = [
    /character\s+limit/i,
    /desc\s*(en|zh|jp|ja|kr|ko|fr|de|es|pt|ru|th|vi|id|it|tr|ar|pl)\b/i,
    /description\s*(en|zh|jp|ja|kr|ko|fr|de|es|pt|ru|th|vi|id|it|tr|ar|pl)\b/i,
    /(描述|说明|备注|注释|类型|赛季|季节|限制|字数|角色限制|状态)/,
  ];
  if (words.some((word) => blockedWords.has(word)) || blockedPatterns.some((pattern) => pattern.test(joined) || pattern.test(column.name))) {
    return false;
  }

  const languageTokens = new Set([
    "chinese",
    "english",
    "japanese",
    "korean",
    "french",
    "german",
    "spanish",
    "portuguese",
    "russian",
    "thai",
    "vietnamese",
    "indonesian",
    "italian",
    "turkish",
    "arabic",
    "polish",
    "simplified",
    "traditional",
    "zh",
    "cn",
    "en",
    "jp",
    "ja",
    "kr",
    "ko",
    "fr",
    "de",
    "es",
    "pt",
    "ru",
    "th",
    "vi",
    "id",
    "it",
    "tr",
    "ar",
    "pl",
    "简中",
    "繁中",
    "中文",
    "英文",
    "日文",
    "日语",
    "韩文",
    "韩语",
    "法文",
    "法语",
    "德文",
    "德语",
    "西语",
    "西班牙",
    "葡语",
    "葡萄牙",
    "俄文",
    "俄语",
    "泰文",
    "泰语",
    "越南",
    "越语",
    "印尼",
    "意大利",
    "土耳其",
    "阿拉伯",
    "波兰",
  ]);
  return words.some((word) => languageTokens.has(word)) || /(中文|英文|日文|日语|韩文|韩语|法文|法语|德文|德语|西班牙|葡萄牙|俄文|俄语|泰文|泰语|越南|印尼|意大利|土耳其|阿拉伯|波兰|简中|繁中)/.test(column.name);
}

function App() {
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [sheetLink, setSheetLink] = useState(DEFAULT_LINK);
  const [sheetState, setSheetState] = useState<SheetPreview | null>(null);
  const [lockedColumns, setLockedColumns] = useState<string[]>([]);
  const [targetColumns, setTargetColumns] = useState<string[]>([]);
  const [sourceColumnId, setSourceColumnId] = useState("");
  const [writeMode, setWriteMode] = useState<WriteMode>("fill_empty");
  const [translationTokenBudget, setTranslationTokenBudget] = useState(0);
  const [glossary, setGlossary] = useState<GlossaryTerm[]>([]);
  const [prompt, setPrompt] = useState("");
  const [readSuccess, setReadSuccess] = useState(false);
  const [busy, setBusy] = useState<BusyState>("idle");
  const [summary, setSummary] = useState<TranslationSummary | null>(null);
  const [history, setHistory] = useState<RunHistory[]>([]);
  const [snapshotRows, setSnapshotRows] = useState<Row[] | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [availableModels, setAvailableModels] = useState<AvailableModelsResponse | null>(null);
  const [aiUsage, setAiUsage] = useState<AiUsageSnapshot | null>(null);
  const [streamTask, setStreamTask] = useState("");
  const [streamMessage, setStreamMessage] = useState("");
  const [streamOutput, setStreamOutput] = useState("");
  const [streamPending, setStreamPending] = useState("");
  const [streamProgress, setStreamProgress] = useState({ current: 0, total: 0 });
  const [lastDebugPrompt, setLastDebugPrompt] = useState("");
  const [lastDebugMessage, setLastDebugMessage] = useState("");
  const [lastUsageDelta, setLastUsageDelta] = useState({
    promptTokens: 0,
    completionTokens: 0,
    totalTokens: 0,
  });
  const [usageLog, setUsageLog] = useState<UsageLogEntry[]>([]);
  const [glossarySheet, setGlossarySheet] = useState(ALL_SHEETS_OPTION);
  const [glossaryColumnWidths, setGlossaryColumnWidths] = useState<Record<string, number>>({
    enabled: 90,
    sourceSheet: 140,
    term: 180,
    category: 120,
    note: 180,
    translation: 160,
  });
  const [pendingWorkflow, setPendingWorkflow] = useState<WorkflowState | null>(null);
  const [glossaryLocalOnlyMode, setGlossaryLocalOnlyMode] = useState(false);
  const [collapsedPanels, setCollapsedPanels] = useState<Record<string, boolean>>({
    stream: false,
    step1: false,
    step2: false,
    step3: false,
    step4: false,
    step5: false,
    result: false,
  });
  const glossarySyncTimersRef = useRef<Map<string, number>>(new Map());
  const glossaryCompositionFieldsRef = useRef<Set<string>>(new Set());
  const streamOutputRef = useRef<HTMLPreElement | null>(null);
  const workflowSaveTimerRef = useRef<number | null>(null);
  const autoLoadTriggeredRef = useRef(false);
  const [glossarySyncingIds, setGlossarySyncingIds] = useState<string[]>([]);
  const [streamAutoFollow, setStreamAutoFollow] = useState(true);
  const configSaveChain = useRef<Promise<unknown>>(Promise.resolve());
  const configCanSave = useRef(false);
  const providerDrafts = useRef<Partial<Record<AiProvider, AppConfig>>>({});
  const [configHydrated, setConfigHydrated] = useState(false);
  const [workflowHydrated, setWorkflowHydrated] = useState(false);

  const uiLanguage = config.uiLanguage ?? "zh-CN";
  const text = TEXT[uiLanguage];

  function getLockedColumnIds() {
    return Array.from(new Set([
      ...lockedColumns,
      ...(sheetState?.columns ?? [])
        .filter((column) => column.id !== sourceColumnId && !targetColumns.includes(column.id))
        .map((column) => column.id),
    ]));
  }

  function buildWorkflowStateSnapshot(
    overrides: Partial<WorkflowState> = {}
  ): WorkflowState {
    return {
      sheetLink,
      lockedColumns: getLockedColumnIds(),
      targetColumns,
      sourceColumnId,
      writeMode,
      translationTokenBudget,
      glossary,
      prompt,
      glossarySheet,
      glossaryColumnWidths,
      glossaryLocalOnlyMode,
      sheetState,
      readSuccess,
      summary,
      history: history.slice(0, 20),
      collapsedPanels,
      ...overrides,
    };
  }

  function applyWorkflowState(workflow: Partial<WorkflowState>) {
    const nextSheetLink = workflow.sheetLink ?? DEFAULT_LINK;
    setPendingWorkflow({
      sheetLink: nextSheetLink,
      lockedColumns: workflow.lockedColumns ?? [],
      targetColumns: workflow.targetColumns ?? [],
      sourceColumnId: workflow.sourceColumnId ?? "",
      writeMode: workflow.writeMode ?? "fill_empty",
      translationTokenBudget: workflow.translationTokenBudget ?? 0,
      glossary: workflow.glossary ?? [],
      prompt: workflow.prompt ?? "",
      glossarySheet: workflow.glossarySheet ?? ALL_SHEETS_OPTION,
      glossaryColumnWidths: workflow.glossaryColumnWidths ?? {},
      glossaryLocalOnlyMode: workflow.glossaryLocalOnlyMode ?? false,
    });
    setSheetLink(nextSheetLink);
    setWriteMode(workflow.writeMode ?? "fill_empty");
    setTranslationTokenBudget(workflow.translationTokenBudget ?? 0);
    setGlossary(workflow.glossary ?? []);
    setPrompt(workflow.prompt ?? "");
    setGlossarySheet(workflow.glossarySheet ?? ALL_SHEETS_OPTION);
    setGlossaryLocalOnlyMode(workflow.glossaryLocalOnlyMode ?? false);
    setSheetState(workflow.sheetState ?? null);
    setReadSuccess(workflow.readSuccess ?? false);
    setSummary(workflow.summary ?? null);
    setHistory(workflow.history ?? []);
    setCollapsedPanels((existing) => ({
      ...existing,
      ...(workflow.collapsedPanels ?? {}),
    }));
    setGlossaryColumnWidths((existing) => ({
      ...existing,
      ...(workflow.glossaryColumnWidths ?? {}),
    }));
    if (workflow.sheetState) {
      const availableIds = new Set(workflow.sheetState.columns.map((column) => column.id));
      const defaults = getDefaultColumnRoles(workflow.sheetState.columns);
      const restoredTargets = (workflow.targetColumns ?? []).filter((id) => availableIds.has(id));
      setLockedColumns((workflow.lockedColumns ?? []).filter((id) => availableIds.has(id)));
      setTargetColumns(workflow.targetColumns ? restoredTargets : defaults.targetIds);
      setSourceColumnId(
        workflow.sourceColumnId && availableIds.has(workflow.sourceColumnId)
          ? workflow.sourceColumnId
          : defaults.sourceId
      );
      setPendingWorkflow(null);
    }
  }

  useEffect(() => {
    invoke<AppConfig>("load_local_config")
      .then((storedConfig) => {
        configCanSave.current = true;
        setConfig({ ...DEFAULT_CONFIG, ...storedConfig });
      })
      .catch((error) => {
        setToast({
          tone: "warning",
          message: TEXT["zh-CN"].failedToLoadLocalConfig(String(error)),
        });
      })
      .finally(() => {
        setConfigHydrated(true);
      });
  }, []);

  useEffect(() => {
    if (!configHydrated || !configCanSave.current) return;
    const timer = window.setTimeout(() => {
      configSaveChain.current = configSaveChain.current.catch(() => undefined)
        .then(() => invoke("save_local_config", { config }))
        .catch(() => setToast({ tone: "error", message: config.uiLanguage === "zh-CN" ? "连接配置保存失败。" : "Failed to save connection settings." }));
    }, 400);
    return () => window.clearTimeout(timer);
  }, [config, configHydrated]);

  useEffect(() => {
    invoke<WorkflowState | null>("load_local_workflow")
      .then((workflow) => {
        if (workflow) {
          applyWorkflowState(workflow);
        }
      })
      .catch(() => {
        // Ignore malformed or missing local workflow snapshots.
      })
      .finally(() => {
        setWorkflowHydrated(true);
      });
  }, []);

  useEffect(() => {
    if (!toast) {
      return;
    }
    const timer = window.setTimeout(() => setToast(null), 2800);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    listen<LlmStreamEvent>("llm-stream", (event) => {
      if (disposed) {
        return;
      }
      const payload = event.payload;
      setStreamTask(payload.task);
      setStreamProgress({ current: payload.current, total: payload.total });
      if (payload.phase === "start") {
        setStreamMessage(payload.message);
        setStreamOutput(payload.message ? `[${payload.message}]\n` : "");
        setStreamPending("");
        setStreamAutoFollow(true);
        setUsageLog([]);
        return;
      }
      if (payload.phase === "status") {
        setStreamMessage(payload.message);
        setStreamOutput((existing) =>
          payload.message ? `${existing}\n[${payload.message}]\n` : existing
        );
        return;
      }
      if (payload.phase === "delta") {
        setStreamPending((existing) => `${existing}${payload.delta}`);
        return;
      }
      if (payload.phase === "debug") {
        setLastDebugMessage(payload.message);
        setLastDebugPrompt(payload.debugPrompt);
        setStreamOutput((existing) =>
          payload.message ? `${existing}\n[Debug] ${payload.message}\n` : existing
        );
        return;
      }
      if (payload.phase === "usage") {
        setLastUsageDelta({
          promptTokens: payload.promptTokens,
          completionTokens: payload.completionTokens,
          totalTokens: payload.totalTokens,
        });
        setUsageLog((existing) => [
          ...existing,
          {
            id: crypto.randomUUID(),
            task: payload.task,
            current: payload.current,
            total: payload.total,
            promptTokens: payload.promptTokens,
            completionTokens: payload.completionTokens,
            totalTokens: payload.totalTokens,
          },
        ]);
        setStreamOutput(
          (existing) =>
            `${existing}\n[Usage] prompt=${payload.promptTokens}, completion=${payload.completionTokens}, total=${payload.totalTokens}\n`
        );
        return;
      }
      if (payload.phase === "complete") {
        setStreamMessage(payload.message);
        setStreamOutput((existing) =>
          payload.message ? `${existing}\n\n[${payload.message}]` : existing
        );
        return;
      }
      if (payload.phase === "error") {
        setStreamMessage(payload.message);
        setStreamOutput((existing) =>
          payload.message ? `${existing}\n\n[Error] ${payload.message}` : existing
        );
      }
    }).then((fn) => {
      unlisteners.push(fn);
    });

    listen<GlossaryPatchEvent>("glossary-patch", (event) => {
      if (disposed) {
        return;
      }
      const payload = event.payload;
      if (payload.terms.length) {
        setGlossary((existing) =>
          existing.map((item) => {
            const patched = payload.terms.find(
              (candidate) => candidate.glossaryId && candidate.glossaryId === item.glossaryId
            );
            return patched ?? item;
          })
        );
      }
      if (payload.removedGlossaryIds.length) {
        setGlossary((existing) =>
          existing.filter((item) => !payload.removedGlossaryIds.includes(item.glossaryId ?? ""))
        );
      }
      if (payload.rows.length) {
        mergeSheetRows(payload.rows);
      }
      if (payload.terms.length) {
        const changedIds = payload.terms
          .map((term) => term.glossaryId)
          .filter((value): value is string => Boolean(value));
        if (changedIds.length) {
          setGlossarySyncingIds((existing) =>
            existing.filter((value) => !changedIds.includes(value))
          );
        }
      }
    }).then((fn) => {
      unlisteners.push(fn);
    });

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!streamPending) {
      return;
    }
    const timer = window.setInterval(() => {
      setStreamPending((existing) => {
        if (!existing) {
          return existing;
        }
        const chunk = existing.slice(0, 6);
        setStreamOutput((currentOutput) => `${currentOutput}${chunk}`);
        return existing.slice(6);
      });
    }, 16);
    return () => window.clearInterval(timer);
  }, [streamPending]);

  useEffect(() => {
    if (!streamAutoFollow || !streamOutputRef.current) {
      return;
    }
    const element = streamOutputRef.current;
    element.scrollTop = element.scrollHeight;
  }, [streamOutput, streamAutoFollow]);

  function handleStreamOutputScroll() {
    const element = streamOutputRef.current;
    if (!element) {
      return;
    }
    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;
    setStreamAutoFollow(distanceFromBottom <= 24);
  }

  useEffect(() => {
    const loadUsage = () => {
      invoke<AiUsageSnapshot>("get_ai_usage")
        .then((usage) => setAiUsage(usage))
        .catch(() => undefined);
    };
    loadUsage();
    const timer = window.setInterval(loadUsage, 2500);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    return () => {
      glossarySyncTimersRef.current.forEach((timer) => window.clearTimeout(timer));
      glossarySyncTimersRef.current.clear();
      if (workflowSaveTimerRef.current) {
        window.clearTimeout(workflowSaveTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    setAvailableModels((existing) => {
      if (!existing) {
        return existing;
      }
      if (
        existing.aiProvider !== config.aiProvider ||
        existing.baseUrl !== config.openaiBaseUrl.trim()
      ) {
        return null;
      }
      return existing;
    });
  }, [config.aiProvider, config.openaiBaseUrl, config.openaiApiKey]);

  const sourceColumn = useMemo(
    () => sheetState?.columns.find((column) => column.id === sourceColumnId) ?? null,
    [sheetState, sourceColumnId]
  );

  const visibleColumns = useMemo(() => {
    if (!sheetState) {
      return [];
    }
    return sheetState.columns.filter(isLanguageColumn);
  }, [sheetState]);

  const translatedColumns = useMemo(() => {
    if (!sheetState) {
      return [];
    }
    return sheetState.columns.filter((column) => targetColumns.includes(column.id));
  }, [sheetState, targetColumns]);
  const fillEmptyPreviewRows = useMemo<FillEmptyPreviewItem[]>(() => {
    if (!sheetState || !sourceColumnId || writeMode !== "fill_empty" || !translatedColumns.length) {
      return [];
    }
    return sheetState.rows
      .map((row) => {
        const source = row.values[sourceColumnId]?.trim() ?? "";
        if (!source) {
          return null;
        }
        const missingTargets = translatedColumns
          .filter((column) => row.availableColumnIds.includes(column.id))
          .filter((column) => !(row.values[column.id] ?? "").trim())
          .map((column) => column.name);
        if (!missingTargets.length) {
          return null;
        }
        const key = row.values.key?.trim() || row.rowId;
        return {
          id: row.rowId,
          key,
          sheetName: row.sheetName,
          rowNumber: row.rowNumber,
          source,
          missingTargets,
        };
      })
      .filter((item): item is FillEmptyPreviewItem => Boolean(item));
  }, [sheetState, sourceColumnId, translatedColumns, writeMode]);

  const sheetOptions = useMemo(() => {
    if (!sheetState) {
      return [];
    }
    return Array.from(new Set(sheetState.rows.map((row) => row.sheetName))).sort();
  }, [sheetState]);

  const scopedGlossaryRows = useMemo(() => {
    if (!sheetState) {
      return [];
    }
    if (glossarySheet === ALL_SHEETS_OPTION) {
      return sheetState.rows;
    }
    return sheetState.rows.filter((row) => row.sheetName === glossarySheet);
  }, [sheetState, glossarySheet]);

  const visibleGlossary = useMemo(() => {
    return glossary.map((term, index) => ({ term, index }));
  }, [glossary]);

  function mergeSheetRows(updatedRows: Row[]) {
    if (!updatedRows.length) {
      return;
    }
    setSheetState((existing) => {
      if (!existing) {
        return existing;
      }
      const nextRows = [...existing.rows];
      for (const row of updatedRows) {
        const index = nextRows.findIndex((candidate) => candidate.rowId === row.rowId);
        if (index >= 0) {
          nextRows[index] = row;
        } else {
          nextRows.push(row);
        }
      }
      nextRows.sort((left, right) =>
        left.sheetName === right.sheetName
          ? left.rowNumber - right.rowNumber
          : left.sheetName.localeCompare(right.sheetName)
      );
      return {
        ...existing,
        rows: nextRows,
      };
    });
  }

  function preserveLocalGlossaryState(
    incomingTerm: GlossaryTerm,
    existingTerm?: GlossaryTerm
  ): GlossaryTerm {
    if (!existingTerm?.localTargetClears?.length) {
      return incomingTerm;
    }
    return {
      ...incomingTerm,
      localTargetClears: [...existingTerm.localTargetClears],
    };
  }

  function buildGlossaryTermForSync(nextTerm: GlossaryTerm) {
    if (!sheetState || !nextTerm.localTargetClears?.length) {
      return nextTerm;
    }
    const boundRow = nextTerm.rowId
      ? sheetState.rows.find((row) => row.rowId === nextTerm.rowId)
      : undefined;
    if (!boundRow) {
      return nextTerm;
    }
    const preservedTranslations = { ...nextTerm.translations };
    for (const columnId of nextTerm.localTargetClears) {
      preservedTranslations[columnId] = boundRow.values[columnId] ?? preservedTranslations[columnId] ?? "";
    }
    return {
      ...nextTerm,
      translations: preservedTranslations,
    };
  }

  function upsertGlossaryTermLocal(nextTerm: GlossaryTerm) {
    const targetId = nextTerm.glossaryId;
    setGlossary((existing) => {
      const index = targetId
        ? existing.findIndex((item) => item.glossaryId === targetId)
        : -1;
      if (index >= 0) {
        return existing.map((item, itemIndex) =>
          itemIndex === index ? preserveLocalGlossaryState(nextTerm, item) : item
        );
      }
      return [...existing, nextTerm];
    });
  }

  function removeGlossaryTermLocal(glossaryId?: string) {
    if (!glossaryId) {
      return;
    }
    setGlossary((existing) => existing.filter((item) => item.glossaryId !== glossaryId));
  }

  useEffect(() => {
    const workflow = buildWorkflowStateSnapshot();
    if (workflowSaveTimerRef.current) {
      window.clearTimeout(workflowSaveTimerRef.current);
    }
    workflowSaveTimerRef.current = window.setTimeout(() => {
      void invoke("save_local_workflow", { workflow }).catch(() => undefined);
    }, 400);
  }, [
    sheetLink,
    lockedColumns,
    targetColumns,
    sourceColumnId,
    writeMode,
    translationTokenBudget,
    glossary,
    prompt,
    glossarySheet,
    glossaryColumnWidths,
    glossaryLocalOnlyMode,
    sheetState,
    readSuccess,
    summary,
    history,
    collapsedPanels,
  ]);

  useEffect(() => {
    if (!sheetState || !pendingWorkflow || pendingWorkflow.sheetLink !== sheetLink) {
      return;
    }
    const availableIds = new Set(sheetState.columns.map((column) => column.id));
    const defaults = getDefaultColumnRoles(sheetState.columns);
    const restoredTargets = pendingWorkflow.targetColumns.filter((id) => availableIds.has(id));
    setLockedColumns(pendingWorkflow.lockedColumns.filter((id) => availableIds.has(id)));
    setTargetColumns(restoredTargets);
    setSourceColumnId(
      availableIds.has(pendingWorkflow.sourceColumnId)
        ? pendingWorkflow.sourceColumnId
        : defaults.sourceId
    );
    setPendingWorkflow(null);
  }, [pendingWorkflow, sheetLink, sheetState]);

  const providerModels = useMemo(() => {
    const dynamicModels =
      availableModels &&
      availableModels.aiProvider === config.aiProvider &&
      availableModels.baseUrl === config.openaiBaseUrl.trim()
        ? availableModels.models
        : [];
    const defaults = config.aiProvider === "custom" ? [] : MODEL_OPTIONS[config.aiProvider] ?? [];
    const baseModels = dynamicModels.length ? dynamicModels : defaults;
    if (!config.openaiModel.trim()) {
      return baseModels;
    }
    return baseModels.includes(config.openaiModel)
      ? baseModels
      : [config.openaiModel, ...baseModels];
  }, [availableModels, config.aiProvider, config.openaiBaseUrl, config.openaiModel]);
  const usingFetchedModels =
    !!availableModels &&
    availableModels.aiProvider === config.aiProvider &&
    availableModels.baseUrl === config.openaiBaseUrl.trim() &&
    availableModels.models.length > 0;
  const detectionProgressPercent =
    streamProgress.total > 0
      ? (Math.min(streamProgress.current, streamProgress.total) / streamProgress.total) * 100
      : 0;
  const recentUsageLog = usageLog.slice(0, 8);
  const currentApiModel = (aiUsage?.model || config.openaiModel || "-").trim() || "-";

  function updateConfig<K extends keyof AppConfig>(key: K, value: AppConfig[K]) {
    setConfig((existing) => ({ ...existing, [key]: value }));
  }

  function providerDisplayName(provider: AiProvider) {
    if (provider === "packy") return "PackyAPI";
    if (provider === "gemini") {
      return "Gemini";
    }
    if (provider === "claude") {
      return "Claude";
    }
    if (provider === "deepseek") {
      return "DeepSeek";
    }
    if (provider === "doubao") {
      return uiLanguage === "zh-CN" ? "豆包" : "Doubao";
    }
    if (provider === "custom") {
      return "Custom LLM";
    }
    return "OpenAI";
  }

  function providerFieldLabel() {
    return uiLanguage === "zh-CN" ? "LLM 供应商" : "LLM Provider";
  }

  function apiKeyFieldLabel() {
    return `${providerDisplayName(config.aiProvider)} API Key`;
  }

  function baseUrlFieldLabel() {
    return `${providerDisplayName(config.aiProvider)} Base URL`;
  }

  function modelFieldLabel() {
    return uiLanguage === "zh-CN" ? "AI 模型" : "AI Model";
  }

  function detectApiButtonLabel() {
    if (busy === "detecting") {
      return uiLanguage === "zh-CN" ? "识别中..." : "Detecting...";
    }
    return uiLanguage === "zh-CN" ? "自动识别 API" : "Detect API";
  }

  function fetchModelsButtonLabel() {
    if (busy === "models") {
      return uiLanguage === "zh-CN" ? "读取中..." : "Loading Models...";
    }
    return uiLanguage === "zh-CN" ? "读取可用模型" : "Load Models";
  }

  function detectedApiMessage(provider: AiProvider, model: string) {
    return uiLanguage === "zh-CN"
      ? `已识别为 ${providerDisplayName(provider)}，模型：${model}`
      : `Detected ${providerDisplayName(provider)} with model ${model}`;
  }

  function missingAiConfigMessage() {
    return uiLanguage === "zh-CN"
      ? `请先填写 ${providerDisplayName(config.aiProvider)} API Key。`
      : `Please fill ${providerDisplayName(config.aiProvider)} API Key first.`;
  }

  function handleProviderChange(provider: AiProvider) {
    providerDrafts.current[config.aiProvider] = { ...config };
    const saved = providerDrafts.current[provider];
    setConfig((existing) => ({
      ...existing,
      aiProvider: provider,
      openaiBaseUrl: saved?.openaiBaseUrl || PROVIDER_DEFAULT_BASE_URL[provider],
      openaiModel: saved?.openaiModel || MODEL_OPTIONS[provider][0],
      openaiApiKey: saved?.openaiApiKey || "",
    }));
    setAvailableModels(null);
  }

  function modeLabel(mode: WriteMode) {
    return mode === "fill_empty" ? text.fillEmptyOnly : text.overwriteUnlocked;
  }

  function showError(error: unknown) {
    const message = String(error);
    if (message.includes("TRANSLATION_CANCELLED")) {
      setToast({
        tone: "warning",
        message:
          uiLanguage === "zh-CN"
            ? "翻译已停止，本次结果未回写飞书。"
            : "Translation stopped. No write-back was performed.",
      });
      return;
    }
    setToast({ tone: "error", message });
  }

  async function handleExportWorkspace() {
    try {
      setBusy("saving");
      const path = await invoke<string>("export_workspace", {
        appConfig: config,
        workflow: buildWorkflowStateSnapshot(),
      });
      setToast({ tone: "success", message: text.workspaceExported(path) });
    } catch (error) {
      const message = String(error);
      if (message.includes("WORKSPACE_DIALOG_CANCELLED")) {
        setBusy("idle");
        return;
      }
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleImportWorkspace() {
    try {
      setBusy("loading");
      const imported = await invoke<WorkspaceImportResponse>("import_workspace");
      setConfig({ ...DEFAULT_CONFIG, ...imported.appConfig });
      applyWorkflowState(imported.workflow);
      await invoke("save_local_workflow", { workflow: buildWorkflowStateSnapshot(imported.workflow) });
      await invoke("save_local_config", { config: { ...DEFAULT_CONFIG, ...imported.appConfig } });
      setToast({ tone: "success", message: text.workspaceImported(imported.path) });
    } catch (error) {
      const message = String(error);
      if (message.includes("WORKSPACE_DIALOG_CANCELLED")) {
        setBusy("idle");
        return;
      }
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleDetectAiProvider() {
    if (!config.openaiApiKey.trim()) {
      setToast({ tone: "error", message: missingAiConfigMessage() });
      return;
    }

    try {
      setBusy("detecting");
      const detected = await invoke<DetectAiProviderResponse>("detect_ai_provider", { config });
      const nextConfig: AppConfig = {
        ...config,
        aiProvider: detected.aiProvider,
        openaiBaseUrl: detected.baseUrl,
        openaiModel: detected.suggestedModel || config.openaiModel,
      };
      setConfig((existing) => ({
        ...existing,
        aiProvider: detected.aiProvider,
        openaiBaseUrl: detected.baseUrl,
        openaiModel: detected.suggestedModel || existing.openaiModel,
      }));
      try {
        const available = await invoke<AvailableModelsResponse>("fetch_available_models", {
          config: nextConfig,
        });
        setAvailableModels(available);
        setConfig((existing) => ({
          ...existing,
          openaiModel: available.selectedModel || existing.openaiModel,
        }));
      } catch {
        setAvailableModels(null);
      }
      setToast({
        tone: "success",
        message: detectedApiMessage(detected.aiProvider, detected.suggestedModel),
      });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleFetchAvailableModels() {
    if (!config.openaiApiKey.trim()) {
      setToast({ tone: "error", message: missingAiConfigMessage() });
      return;
    }
    try {
      setBusy("models");
      const available = await invoke<AvailableModelsResponse>("fetch_available_models", { config });
      setAvailableModels(available);
      setConfig((existing) => ({
        ...existing,
        aiProvider: available.aiProvider,
        openaiBaseUrl: available.baseUrl,
        openaiModel: available.selectedModel || existing.openaiModel,
      }));
      setToast({
        tone: "success",
        message:
          uiLanguage === "zh-CN"
            ? `已读取 ${available.models.length} 个可用模型`
            : `Loaded ${available.models.length} available models`,
      });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  function ensureConfigForLoad() {
    if (!config.feishuAppId.trim() || !config.feishuAppSecret.trim()) {
      setToast({ tone: "error", message: text.fillFeishuFirst });
      return false;
    }
    return true;
  }

  function ensureConfigForAi() {
    if (!ensureConfigForLoad()) {
      return false;
    }
    if (!config.openaiApiKey.trim() || !config.openaiModel.trim()) {
      setToast({ tone: "error", message: missingAiConfigMessage() });
      return false;
    }
    return true;
  }

  function ensureSourceColumn() {
    if (!sourceColumnId) {
      setToast({ tone: "warning", message: text.chooseSourceFirst });
      return false;
    }
    return true;
  }

  async function handleLoadSheet() {
    if (!ensureConfigForLoad()) {
      return;
    }

    try {
      setBusy("loading");
      const response = await invoke<LoadSheetResponse>("load_sheet_preview", {
        link: sheetLink.trim(),
        config,
      });
      const defaults = getDefaultColumnRoles(response.preview.columns);
      const availableIds = new Set(response.preview.columns.map((column) => column.id));
      const restoredLocked = lockedColumns.filter((id) => availableIds.has(id));
      const restoredTargets = targetColumns.filter((id) => availableIds.has(id));
      const restoredSource =
        sourceColumnId && availableIds.has(sourceColumnId) ? sourceColumnId : defaults.sourceId;

      setSheetState(response.preview);
      setReadSuccess(response.readSuccess);
      setLockedColumns(restoredLocked);
      setTargetColumns(sheetState ? restoredTargets : defaults.targetIds);
      setSourceColumnId(restoredSource);
      setSummary(null);
      setSnapshotRows(null);
      setToast({
        tone: "success",
        message: text.loadedSheets(response.preview.sheetNames.length),
      });
    } catch (error) {
      setReadSuccess(false);
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  useEffect(() => {
    if (!configHydrated || !workflowHydrated || autoLoadTriggeredRef.current) {
      return;
    }
    if (busy !== "idle") {
      return;
    }
    if (!sheetLink.trim() || !config.feishuAppId.trim() || !config.feishuAppSecret.trim()) {
      return;
    }
    autoLoadTriggeredRef.current = true;
    void handleLoadSheet();
  }, [
    busy,
    config.feishuAppId,
    config.feishuAppSecret,
    configHydrated,
    sheetLink,
    workflowHydrated,
  ]);

  async function handleGenerateGlossary() {
    if (!sheetState || !ensureSourceColumn()) {
      return;
    }
    if (!ensureConfigForAi()) {
      return;
    }

    try {
      setBusy("glossary");
      const response = await invoke<GlossaryResponse>("generate_glossary", {
        request: {
          rows: scopedGlossaryRows,
          sourceColumnId,
          targetColumns: translatedColumns,
          prompt,
        },
        config,
      });
      setGlossary((existing) => [
        ...existing,
        ...response.terms.map((term) => ({
          ...term,
          glossaryId: term.glossaryId ?? createGlossaryId(term.term),
          sourceSheet: glossarySheet === ALL_SHEETS_OPTION ? "" : glossarySheet,
        })),
      ]);
      if (response.rows?.length) {
        mergeSheetRows(response.rows);
      }
      setToast({ tone: "success", message: text.glossaryGenerated });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleGeneratePrompt() {
    if (!sheetState || !ensureSourceColumn()) {
      return;
    }
    if (!ensureConfigForAi()) {
      return;
    }
    if (!targetColumns.length) {
      setToast({ tone: "warning", message: text.chooseTargetFirst });
      return;
    }

    try {
      setBusy("prompt");
      const response = await invoke<PromptSuggestionResponse>("generate_translation_prompt", {
        request: {
          sourceColumnName: sourceColumn?.name ?? sourceColumnId,
          sourceColumnId,
          rows: sheetState.rows,
          targetColumns: translatedColumns,
        },
        config,
      });
      setPrompt(response.prompt);
      setToast({ tone: "success", message: text.promptGenerated });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleTranslate(retryOnly = false) {
    if (!sheetState || !ensureSourceColumn()) {
      return;
    }
    if (!targetColumns.length) {
      setToast({ tone: "warning", message: text.chooseTargetFirst });
      return;
    }
    if (!ensureConfigForAi()) {
      return;
    }

    try {
      setBusy("translating");
      setSnapshotRows(structuredClone(sheetState.rows));
      const failedIds = new Set(summary?.failedRowIds ?? []);
      const requestedRows = retryOnly ? sheetState.rows.filter((row) => failedIds.has(row.rowId)) : sheetState.rows;
      if (!requestedRows.length) return;
      const response = await invoke<TranslationResponse>("run_translation", {
        request: {
          spreadsheetId: sheetState.spreadsheetId,
          columns: sheetState.columns,
          rows: requestedRows,
          lockedColumnIds: getLockedColumnIds(),
          targetColumnIds: targetColumns,
          sourceColumnId,
          glossary,
          prompt,
          writeMode,
          translationTokenBudget,
        },
        config,
      });

      const returned = new Map(response.rows.map((row) => [row.rowId, row]));
      setSheetState((existing) => existing ? { ...existing, rows: existing.rows.map((row) => returned.get(row.rowId) ?? row) } : existing);
      setSummary(response.summary);
      setHistory((existing) => [
        {
          id: crypto.randomUUID(),
          timestamp: new Date().toLocaleString(uiLanguage),
          mode: writeMode,
          processedRows: response.summary.processedRows,
          failedRows: response.summary.failedRows.length,
        },
        ...existing,
      ].slice(0, 20));
      const state = response.summary.status;
      setToast({
        tone: state === "failed" ? "error" : state === "partial" || state === "unchanged" ? "warning" : "success",
        message: state === "failed"
          ? (uiLanguage === "zh-CN" ? "全部批次失败，未写回飞书。可重试失败行。" : "All batches failed. Nothing was written. Retry failed rows.")
          : state === "partial"
          ? (uiLanguage === "zh-CN" ? "部分翻译失败；成功部分已写回并核验，可重试失败行。" : "Partial failure. Successful changes were written and verified.")
          : state === "unchanged"
          ? (uiLanguage === "zh-CN" ? "没有需要写回的变更。" : "No changes to write back.")
          : (uiLanguage === "zh-CN" ? "翻译完成，飞书写回已读回核验。" : "Translation written and verified."),
      });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  async function handleStopTranslation() {
    try {
      await invoke("stop_translation");
      setToast({
        tone: "warning",
        message:
          uiLanguage === "zh-CN"
            ? "已发送停止请求，正在安全中断翻译。"
            : "Stop requested. Safely stopping translation...",
      });
    } catch (error) {
      showError(error);
    }
  }

  function handleRollback() {
    if (!sheetState || !snapshotRows) {
      setToast({ tone: "warning", message: text.noSnapshot });
      return;
    }

    setBusy("rollback");
    setSheetState({ ...sheetState, rows: snapshotRows });
    setSummary(null);
    setBusy("idle");
    setToast({ tone: "warning", message: text.rollbackDone });
  }

  function setColumnRole(columnId: string, role: ColumnRole) {
    if (role === "source") {
      setSourceColumnId(columnId);
      setLockedColumns((existing) => existing.filter((value) => value !== columnId));
      setTargetColumns((existing) => existing.filter((value) => value !== columnId));
      return;
    }

    if (role === "target") {
      setTargetColumns((existing) =>
        existing.includes(columnId) ? existing : [...existing, columnId]
      );
      setLockedColumns((existing) => existing.filter((value) => value !== columnId));
      if (sourceColumnId === columnId) {
        setSourceColumnId("");
      }
      return;
    }

    if (role === "locked") {
      setLockedColumns((existing) =>
        existing.includes(columnId) ? existing : [...existing, columnId]
      );
      setTargetColumns((existing) => existing.filter((value) => value !== columnId));
      if (sourceColumnId === columnId) {
        setSourceColumnId("");
      }
      return;
    }
  }

  function getColumnRole(columnId: string): ColumnRole {
    if (sourceColumnId === columnId) {
      return "source";
    }
    if (targetColumns.includes(columnId)) {
      return "target";
    }
    return "locked";
  }

  async function syncGlossaryTermToFeishu(nextTerm: GlossaryTerm, remove = false) {
    if (!sheetState || !sourceColumnId) {
      return;
    }
    const glossaryId = nextTerm.glossaryId ?? createGlossaryId(nextTerm.term || "term");
    const syncPayload = buildGlossaryTermForSync(nextTerm);
    const termToSync = {
      ...syncPayload,
      glossaryId,
    };
    setGlossarySyncingIds((existing) =>
      existing.includes(glossaryId) ? existing : [...existing, glossaryId]
    );
    try {
      const response = await invoke<GlossarySyncResponse>("sync_glossary_term", {
        request: {
          spreadsheetId: sheetState.spreadsheetId,
          columns: sheetState.columns,
          rows: sheetState.rows,
          sourceColumnId,
          targetColumnIds: targetColumns,
          glossaryTerm: termToSync,
          remove,
        },
        config,
      });
      if (response.permissionDenied) {
        setGlossaryLocalOnlyMode(true);
        setToast({
          tone: "warning",
          message:
            uiLanguage === "zh-CN"
              ? "飞书术语表实时同步被拒绝，已切换为仅本地编辑。可稍后统一回写。"
              : "Glossary live sync was denied. Switched to local-only mode. You can write back later.",
        });
      }
      if (response.row) {
        mergeSheetRows([response.row]);
      }
      if (response.removed) {
        removeGlossaryTermLocal(glossaryId);
        return;
      }
      if (response.term) {
        upsertGlossaryTermLocal(response.term);
      }
    } catch (error) {
      showError(error);
    } finally {
      glossarySyncTimersRef.current.delete(glossaryId);
      setGlossarySyncingIds((existing) => existing.filter((value) => value !== glossaryId));
    }
  }

  function queueGlossarySync(nextTerm: GlossaryTerm) {
    if (!sheetState || !sourceColumnId || glossaryLocalOnlyMode) {
      return;
    }
    const glossaryId = nextTerm.glossaryId ?? createGlossaryId(nextTerm.term || "term");
    const termToQueue = {
      ...nextTerm,
      glossaryId,
    };
    const existingTimer = glossarySyncTimersRef.current.get(glossaryId);
    if (existingTimer) {
      window.clearTimeout(existingTimer);
    }
    setGlossarySyncingIds((existing) =>
      existing.includes(glossaryId) ? existing : [...existing, glossaryId]
    );
    const timer = window.setTimeout(() => {
      void syncGlossaryTermToFeishu(termToQueue);
    }, 420);
    glossarySyncTimersRef.current.set(glossaryId, timer);
  }

  function glossaryFieldComposeKey(glossaryId: string, field: string) {
    return `${glossaryId}:${field}`;
  }

  function handleGlossaryCompositionStart(glossaryId: string, field: string) {
    glossaryCompositionFieldsRef.current.add(glossaryFieldComposeKey(glossaryId, field));
  }

  function handleGlossaryCompositionEnd(
    index: number,
    nextTerm: GlossaryTerm,
    field: string
  ) {
    const glossaryId =
      nextTerm.glossaryId ?? glossary[index]?.glossaryId ?? createGlossaryId(nextTerm.term || `row-${index + 1}`);
    glossaryCompositionFieldsRef.current.delete(glossaryFieldComposeKey(glossaryId, field));
    updateGlossaryTerm(index, { ...nextTerm, glossaryId }, true);
  }

  function updateGlossaryField(
    index: number,
    nextTerm: GlossaryTerm,
    field: string,
    shouldSync = true
  ) {
    const glossaryId =
      nextTerm.glossaryId ?? glossary[index]?.glossaryId ?? createGlossaryId(nextTerm.term || `row-${index + 1}`);
    const isComposing = glossaryCompositionFieldsRef.current.has(
      glossaryFieldComposeKey(glossaryId, field)
    );
    updateGlossaryTerm(index, { ...nextTerm, glossaryId }, shouldSync && !isComposing);
  }

  function updateGlossaryTerm(index: number, nextTerm: GlossaryTerm, shouldSync = true) {
    const normalizedTerm = {
      ...nextTerm,
      glossaryId:
        nextTerm.glossaryId ??
        glossary[index]?.glossaryId ??
        createGlossaryId(nextTerm.term || `row-${index + 1}`),
    };
    setGlossary((existing) =>
      existing.map((item, itemIndex) => (itemIndex === index ? normalizedTerm : item))
    );
    if (shouldSync) {
      queueGlossarySync(normalizedTerm);
    }
  }

  function addGlossaryTerm() {
    const currentScope = glossarySheet === ALL_SHEETS_OPTION ? "" : glossarySheet;
    setGlossary((existing) => [
      ...existing,
      {
        glossaryId: createGlossaryId(currentScope || "new"),
        term: "",
        category: text.general,
        note: "",
        enabled: true,
        sourceSheet: currentScope,
        rowId: "",
        rowNumber: 0,
        sheetId: "",
        translations: Object.fromEntries(translatedColumns.map((column) => [column.id, ""])),
      },
    ]);
  }

  function removeGlossaryTerm(index: number) {
    const targetTerm = glossary[index];
    if (!targetTerm) {
      return;
    }
    const glossaryId = targetTerm.glossaryId;
    const existingTimer = glossaryId ? glossarySyncTimersRef.current.get(glossaryId) : undefined;
    if (existingTimer) {
      window.clearTimeout(existingTimer);
      if (glossaryId) {
        glossarySyncTimersRef.current.delete(glossaryId);
      }
    }
    if (glossaryId) {
      setGlossarySyncingIds((existing) => existing.filter((value) => value !== glossaryId));
    }
    setGlossary((existing) => existing.filter((_, itemIndex) => itemIndex !== index));
  }

  function clearGlossaryTargetTranslations(index: number) {
    const targetTerm = glossary[index];
    if (!targetTerm) {
      return;
    }
    const glossaryId = targetTerm.glossaryId;
    const existingTimer = glossaryId ? glossarySyncTimersRef.current.get(glossaryId) : undefined;
    if (existingTimer) {
      window.clearTimeout(existingTimer);
      if (glossaryId) {
        glossarySyncTimersRef.current.delete(glossaryId);
      }
    }
    if (glossaryId) {
      setGlossarySyncingIds((existing) => existing.filter((value) => value !== glossaryId));
    }
    updateGlossaryTerm(
      index,
      {
        ...targetTerm,
        translations: Object.fromEntries(translatedColumns.map((column) => [column.id, ""])),
        localTargetClears: translatedColumns.map((column) => column.id),
      },
      false
    );
  }

  function clearGlossary() {
    for (const timer of glossarySyncTimersRef.current.values()) {
      window.clearTimeout(timer);
    }
    glossarySyncTimersRef.current.clear();
    setGlossarySyncingIds([]);
    setGlossary([]);
  }

  function updateGlossaryColumnWidth(columnKey: string, nextValue: number) {
    setGlossaryColumnWidths((existing) => ({
      ...existing,
      [columnKey]: Math.max(80, nextValue),
    }));
  }

  function getGlossaryColumnWidth(columnKey: string, fallback = 120) {
    return glossaryColumnWidths[columnKey] ?? fallback;
  }

  function streamTaskLabel(task: string) {
    if (task === "prompt") {
      return uiLanguage === "zh-CN" ? "AI填充提示词" : "AI Prompt Generation";
    }
    if (task === "glossary") {
      return uiLanguage === "zh-CN" ? "AI整理术语" : "AI Glossary";
    }
    if (task === "fillGlossary") {
      return uiLanguage === "zh-CN" ? "AI填充翻译" : "AI Fill Translation";
    }
    if (task === "translation") {
      return uiLanguage === "zh-CN" ? "执行翻译" : "Translation";
    }
    return task || (uiLanguage === "zh-CN" ? "空闲" : "Idle");
  }

  function togglePanel(panelKey: string) {
    setCollapsedPanels((existing) => ({
      ...existing,
      [panelKey]: !existing[panelKey],
    }));
  }

  function startGlossaryColumnResize(
    columnKey: string,
    startEvent: React.MouseEvent<HTMLDivElement>
  ) {
    startEvent.preventDefault();
    const startX = startEvent.clientX;
    const startWidth = glossaryColumnWidths[columnKey] ?? 120;
    const handleMove = (moveEvent: MouseEvent) => {
      updateGlossaryColumnWidth(columnKey, startWidth + (moveEvent.clientX - startX));
    };
    const handleUp = () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };
    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
  }

  function addGlossaryTermsFromCurrentSheet() {
    if (!sheetState || !sourceColumnId) {
      return;
    }

    const scopeKey = glossarySheet === ALL_SHEETS_OPTION ? "" : glossarySheet;
    const seen = new Set(glossary.map((term) => `${term.sourceSheet || ""}::${term.term.trim()}`));
    const additions = scopedGlossaryRows
      .map((row) => {
        const term = row.values[sourceColumnId]?.trim() ?? "";
        return {
          term,
          row,
          scopeKey: glossarySheet === ALL_SHEETS_OPTION ? row.sheetName : scopeKey,
        };
      })
      .filter((item) => item.term.length > 0)
      .filter(
        (item, index, items) =>
          index ===
          items.findIndex(
            (candidate) =>
              candidate.term === item.term && candidate.scopeKey === item.scopeKey
          )
      )
      .filter((item) => !seen.has(`${item.scopeKey}::${item.term}`))
      .map<GlossaryTerm>(({ term, row, scopeKey: nextScopeKey }) => ({
        glossaryId: createGlossaryId(`${row.rowId}-${term}`),
        term,
        category: text.general,
        note: "",
        enabled: true,
        sourceSheet: nextScopeKey,
        rowId: row.rowId,
        rowNumber: row.rowNumber,
        sheetId: row.sheetId,
        translations: Object.fromEntries(
          translatedColumns.map((column) => [column.id, row.values[column.id] ?? ""])
        ),
      }));

    if (!additions.length) {
      return;
    }
    setGlossary((existing) => [...existing, ...additions]);
  }

  async function handleFillGlossaryTranslations() {
    if (!sheetState || !translatedColumns.length || !ensureConfigForAi()) {
      return;
    }
    try {
      setBusy("glossary");
      const response = await invoke<GlossaryResponse>("fill_glossary_translations", {
        request: {
          spreadsheetId: sheetState.spreadsheetId,
          columns: sheetState.columns,
          rows: sheetState.rows,
          sourceColumnId,
          glossary,
          targetColumns: translatedColumns,
          prompt,
        },
        config,
      });
      setGlossary(response.terms);
      if (response.rows?.length) {
        mergeSheetRows(response.rows);
      }
      if (response.liveSyncDenied) {
        setGlossaryLocalOnlyMode(true);
        setToast({
          tone: "warning",
          message:
            uiLanguage === "zh-CN"
              ? "飞书术语表实时同步被拒绝，已切换为仅本地编辑。可稍后统一回写。"
              : "Glossary live sync was denied. Switched to local-only mode. You can write back later.",
        });
        return;
      }
      setGlossaryLocalOnlyMode(false);
      setToast({
        tone: "success",
        message:
          uiLanguage === "zh-CN" ? "术语表翻译空缺已由 AI 补全。" : "AI filled glossary translations.",
      });
    } catch (error) {
      showError(error);
    } finally {
      setBusy("idle");
    }
  }

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-badge">AN</span>
          <div>
            <h1>Feishu Loc</h1>
            <div className="brand-version">
              <span>{uiLanguage === "zh-CN" ? "版本" : "Version"}</span>
              <strong>v{APP_VERSION}</strong>
            </div>
          </div>
        </div>

        <div className="sidebar-panel">
          <h2>{text.connection}</h2>
          <div className="compact-grid">
            <label className="field tight">
              <span>{text.language}</span>
              <select
                value={uiLanguage}
                onChange={(event) =>
                  updateConfig("uiLanguage", event.currentTarget.value as UiLanguage)
                }
              >
                {LANGUAGE_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>

            <label className="field tight">
              <span>{text.feishuAppId}</span>
              <input
                value={config.feishuAppId}
                onChange={(event) => updateConfig("feishuAppId", event.currentTarget.value)}
              />
            </label>

            <label className="field tight">
              <span>{text.feishuAppSecret}</span>
              <input
                type="password"
                value={config.feishuAppSecret}
                onChange={(event) => updateConfig("feishuAppSecret", event.currentTarget.value)}
              />
            </label>

            <label className="field tight">
              <span>{providerFieldLabel()}</span>
              <select
                value={config.aiProvider}
                onChange={(event) => handleProviderChange(event.currentTarget.value as AiProvider)}
              >
                {PROVIDER_OPTIONS.map((provider) => (
                  <option key={provider.value} value={provider.value}>
                    {provider.label}
                  </option>
                ))}
              </select>
            </label>

            <label className="field tight">
              <span>{apiKeyFieldLabel()}</span>
              <input
                type="password"
                value={config.openaiApiKey}
                onChange={(event) => updateConfig("openaiApiKey", event.currentTarget.value)}
              />
            </label>

            <button
              type="button"
              className="primary"
              onClick={handleDetectAiProvider}
              disabled={busy !== "idle"}
            >
              {detectApiButtonLabel()}
            </button>

            <label className="field tight">
              <span>{baseUrlFieldLabel()}</span>
              <input
                value={config.openaiBaseUrl}
                onChange={(event) => updateConfig("openaiBaseUrl", event.currentTarget.value)}
              />
            </label>

            <label className="field tight">
              <span>{modelFieldLabel()}</span>
              {(config.aiProvider === "openai" || config.aiProvider === "packy" || config.aiProvider === "custom") && config.openaiModel === "gemini-3.8-flash" && (
                <small>{uiLanguage === "zh-CN" ? "思考深度：最高（high）" : "Thinking: highest (high)"}</small>
              )}
              {config.aiProvider === "custom" && !usingFetchedModels ? (
                <input
                  value={config.openaiModel}
                  onChange={(event) => updateConfig("openaiModel", event.currentTarget.value)}
                />
              ) : (
                <select
                  value={config.openaiModel}
                  onChange={(event) => updateConfig("openaiModel", event.currentTarget.value)}
                >
                  {providerModels.map((model) => (
                    <option key={model} value={model}>
                      {model}
                    </option>
                  ))}
                </select>
              )}
            </label>

            <button
              type="button"
              className="primary"
              onClick={handleFetchAvailableModels}
              disabled={busy !== "idle" && busy !== "models"}
            >
              {fetchModelsButtonLabel()}
            </button>
          </div>

          <div className="workspace-config-actions">
            <h3>{text.workspaceConfig}</h3>
            <div className="workspace-config-buttons">
              <button
                type="button"
                className="primary"
                onClick={handleImportWorkspace}
                disabled={busy !== "idle" && busy !== "loading"}
              >
                {text.importWorkspace}
              </button>

              <button
                type="button"
                className="primary"
                onClick={handleExportWorkspace}
                disabled={busy !== "idle" && busy !== "saving"}
              >
                {text.exportWorkspace}
              </button>
            </div>
          </div>
        </div>

        <div className="sidebar-panel">
          <h2>{text.currentTask}</h2>
          <dl>
            <div>
              <dt>{text.sheets}</dt>
              <dd>{sheetState?.sheetNames.length ?? 0}</dd>
            </div>
            <div>
              <dt>{text.columns}</dt>
              <dd>{sheetState?.columns.length ?? 0}</dd>
            </div>
            <div>
              <dt>{text.targets}</dt>
              <dd>{targetColumns.length}</dd>
            </div>
            <div>
              <dt>{text.source}</dt>
              <dd>{sourceColumn?.name || text.sourceEmpty}</dd>
            </div>
          </dl>
        </div>

        <div className="sidebar-panel">
          <h2>{uiLanguage === "zh-CN" ? "LLM 用量" : "LLM Usage"}</h2>
          <dl>
            <div>
              <dt>{uiLanguage === "zh-CN" ? "请求数" : "Requests"}</dt>
              <dd>{aiUsage?.requestCount ?? 0}</dd>
            </div>
            <div>
              <dt>{uiLanguage === "zh-CN" ? "输入 Tokens" : "Prompt Tokens"}</dt>
              <dd>{aiUsage?.promptTokens ?? 0}</dd>
            </div>
            <div>
              <dt>{uiLanguage === "zh-CN" ? "输出 Tokens" : "Completion Tokens"}</dt>
              <dd>{aiUsage?.completionTokens ?? 0}</dd>
            </div>
            <div>
              <dt>{uiLanguage === "zh-CN" ? "总 Tokens" : "Total Tokens"}</dt>
              <dd>{aiUsage?.totalTokens ?? 0}</dd>
            </div>
          </dl>
        </div>
      </aside>

      <main className="workspace">
        <section className="hero">
          <div>
            <p className="eyebrow">{text.workflow}</p>
            <h2>{text.heroTitle}</h2>
          </div>
          <div className="hero-metrics">
            <div>
              <strong>{readSuccess ? text.success : text.notLoaded}</strong>
              <span>{text.readStatus}</span>
            </div>
            <div>
              <strong>{sheetState?.sheetNames.length ?? 0}</strong>
              <span>{text.childSheets}</span>
            </div>
            <div>
              <strong>{sheetState?.rows.length ?? 0}</strong>
              <span>{text.rows}</span>
            </div>
            <div>
              <strong>{targetColumns.length}</strong>
              <span>{text.targets}</span>
            </div>
          </div>
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{uiLanguage === "zh-CN" ? "检测面板" : "Diagnostics"}</p>
              <h3>{streamTaskLabel(streamTask)}</h3>
            </div>
            <button
              type="button"
              className="panel-toggle"
              onClick={() => togglePanel("stream")}
            >
              {collapsedPanels.stream ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
            </button>
          </div>
          {!collapsedPanels.stream && (
            <>
              <div className="diagnostic-summary-grid">
                <div className="mini-card diagnostic-mini-card">
                  <span>{uiLanguage === "zh-CN" ? "当前状态" : "Current Status"}</span>
                  <strong>
                    {streamMessage || (uiLanguage === "zh-CN" ? "等待任务开始" : "Waiting for a task")}
                  </strong>
                </div>
                <div className="mini-card diagnostic-mini-card">
                  <span>{uiLanguage === "zh-CN" ? "批次进度" : "Batch Progress"}</span>
                  <strong>
                    {streamProgress.total > 0
                      ? `${Math.min(streamProgress.current, streamProgress.total)}/${streamProgress.total}`
                      : "0/0"}
                  </strong>
                </div>
                <div className="mini-card diagnostic-mini-card">
                  <span>{uiLanguage === "zh-CN" ? "最近一次 Token" : "Last Token Delta"}</span>
                  <strong>Total {lastUsageDelta.totalTokens}</strong>
                </div>
                <div className="mini-card diagnostic-mini-card">
                  <span>{uiLanguage === "zh-CN" ? "已记录请求" : "Logged Requests"}</span>
                  <strong>{usageLog.length}</strong>
                </div>
                <div className="mini-card diagnostic-mini-card">
                  <span>{uiLanguage === "zh-CN" ? "当前模型" : "Current Model"}</span>
                  <strong>{currentApiModel}</strong>
                </div>
              </div>

              <div className="stream-progress-bar">
                <div
                  className="stream-progress-fill"
                  style={{ width: `${detectionProgressPercent}%` }}
                />
              </div>

              <div className="diagnostic-layout">
                <div className="diagnostic-left">
                  <div className="debug-card prompt-output-card">
                    <span>{uiLanguage === "zh-CN" ? "当前输入提示词" : "Current Prompt"}</span>
                    <pre>{prompt || (uiLanguage === "zh-CN" ? "当前为空。" : "Empty.")}</pre>
                  </div>
                  <div className="debug-card">
                    <span>{uiLanguage === "zh-CN" ? "最近一次完整请求 Prompt" : "Last Full Prompt Sent To Model"}</span>
                    <strong>
                      {lastDebugMessage ||
                        (uiLanguage === "zh-CN" ? "尚未发起模型请求" : "No model request yet")}
                    </strong>
                    <pre className="diagnostic-output-box">
                      {lastDebugPrompt ||
                        (uiLanguage === "zh-CN"
                          ? "这里会显示系统提示词和用户内容的最终拼装结果。"
                          : "The final combined system and user prompt will appear here.")}
                    </pre>
                  </div>
                  <div className="debug-card">
                    <span>{uiLanguage === "zh-CN" ? "批次 Token 明细" : "Batch Token Details"}</span>
                    <div className="debug-metrics">
                      <strong>
                        {uiLanguage === "zh-CN" ? "输入" : "Prompt"}: {lastUsageDelta.promptTokens}
                      </strong>
                      <strong>
                        {uiLanguage === "zh-CN" ? "输出" : "Completion"}: {lastUsageDelta.completionTokens}
                      </strong>
                      <strong>Total: {lastUsageDelta.totalTokens}</strong>
                    </div>
                    <div className="usage-log">
                      {recentUsageLog.length ? (
                        recentUsageLog.map((entry) => (
                          <div key={entry.id} className="usage-log-row">
                            <strong>
                              {entry.task} {entry.current}/{entry.total}
                            </strong>
                            <span>
                              p={entry.promptTokens} / c={entry.completionTokens} / t={entry.totalTokens}
                            </span>
                          </div>
                        ))
                      ) : (
                        <span className="usage-log-empty">
                          {uiLanguage === "zh-CN"
                            ? "还没有批次 Token 记录。"
                            : "No batch token records yet."}
                        </span>
                      )}
                    </div>
                    <p className="debug-note">
                      {uiLanguage === "zh-CN"
                        ? "如果这里跳动很频繁，通常说明当前任务仍然被拆成了较多批次。"
                        : "If this updates too often, the current task is still being split into too many batches."}
                    </p>
                  </div>
                </div>
                <div className="diagnostic-right">
                  <div className="debug-card stream-card">
                    <span>{uiLanguage === "zh-CN" ? "实时输出" : "Live Output"}</span>
                    <pre
                      ref={streamOutputRef}
                      className="stream-output diagnostic-output-box"
                      onScroll={handleStreamOutputScroll}
                    >
                      {streamOutput ||
                        (uiLanguage === "zh-CN"
                          ? "这里会实时显示模型输出和阶段状态。"
                          : "Live model output and stage status will appear here.")}
                    </pre>
                  </div>
                </div>
              </div>
            </>
          )}
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{text.step1}</p>
              <h3>{text.readSpreadsheet}</h3>
            </div>
            <div className="button-group">
              <button
                type="button"
                className="primary action-button"
                onClick={handleLoadSheet}
                disabled={busy !== "idle"}
              >
                {busy === "loading" ? text.loading : text.readFeishuSpreadsheet}
              </button>
              <button
                type="button"
                className="panel-toggle"
                onClick={() => togglePanel("step1")}
              >
                {collapsedPanels.step1 ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
              </button>
            </div>
          </div>

          {!collapsedPanels.step1 && (
            <>
          <p className="section-tip">
            {uiLanguage === "zh-CN"
              ? "先将“本地化工具”添加至相应飞书表格的文档应用中。"
              : 'Add "Localization Tool" to the target Feishu sheet as a document app first.'}
          </p>
          <label className="field">
            <span>{text.feishuLink}</span>
            <input
              value={sheetLink}
              onChange={(event) => setSheetLink(event.currentTarget.value)}
            />
          </label>

          <div className="summary-grid">
            <div className="mini-card">
              <span>{text.readResult}</span>
              <strong>{readSuccess ? text.loadedSuccessfully : text.waiting}</strong>
            </div>
            <div className="mini-card">
              <span>{text.childSheets}</span>
              <strong>{sheetState?.sheetNames.length ?? 0}</strong>
            </div>
            <div className="mini-card">
              <span>{text.rows}</span>
              <strong>{sheetState?.rows.length ?? 0}</strong>
            </div>
            <div className="mini-card">
              <span>{text.uniqueHeaders}</span>
              <strong>{sheetState?.columns.length ?? 0}</strong>
            </div>
          </div>
            </>
          )}
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{text.step2}</p>
              <h3>{text.columnRoles}</h3>
            </div>
            <button
              type="button"
              className="panel-toggle"
              onClick={() => togglePanel("step2")}
            >
              {collapsedPanels.step2 ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
            </button>
          </div>
          {!collapsedPanels.step2 && (
            <>
          <p className="section-tip">{text.heroCopy}</p>
          <details className="step-details">
            <summary>
              {uiLanguage === "zh-CN" ? "查看语言列表头筛选与 Key 识别逻辑" : "View Language Filtering And Key Rules"}
            </summary>
            <div className="step-details-body">
              <p>
                {uiLanguage === "zh-CN"
                  ? "系统会优先识别纯语言列，例如 Chinese、English、Japanese、中文、英文、日语、简中、繁中。"
                  : "The app prioritizes pure language columns such as Chinese, English, Japanese, 中文, 英文, 日语, 简中, and 繁中."}
              </p>
              <p>
                {uiLanguage === "zh-CN"
                  ? "像 Type、Season、Character Limit、DescEN、DescZH、备注、说明、状态 这类业务字段会主动剔除，不再出现在列配置里，但翻译时会作为辅助上下文参与理解。"
                  : "Business fields like Type, Season, Character Limit, DescEN, DescZH, notes, descriptions, and status are excluded from column roles, but still sent as auxiliary context during translation."}
              </p>
              <p>
                {uiLanguage === "zh-CN"
                  ? "Key 列不会作为语言列展示，但会优先作为行级唯一标识；只有在 Key 为空或冲突时，才回退到 子表 + 行号。"
                  : "The Key column is hidden from language-role selection, but preferred as the row-level unique identifier. The app falls back to sheet plus row number only when Key is empty or duplicated."}
              </p>
            </div>
          </details>

          {sheetState ? (
            <div className="column-compact-shell">
              <table className="compact-table">
                <thead>
                  <tr>
                    <th>{text.column}</th>
                    <th>{text.sample}</th>
                    <th>{text.sheetsShort}</th>
                    <th>{text.role}</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleColumns.map((column) => {
                    const role = getColumnRole(column.id);
                    return (
                      <tr key={column.id}>
                        <td>{column.name}</td>
                        <td>{column.sample || "-"}</td>
                        <td>{column.sheetCount}</td>
                        <td>
                          <div className="role-group">
                            <button
                              type="button"
                              className={role === "source" ? "role-chip active" : "role-chip"}
                              onClick={() => setColumnRole(column.id, "source")}
                            >
                              {text.sourceRole}
                            </button>
                            <button
                              type="button"
                              className={role === "target" ? "role-chip active" : "role-chip"}
                              onClick={() => setColumnRole(column.id, "target")}
                            >
                              {text.targetRole}
                            </button>
                            <button
                              type="button"
                              className={role === "locked" ? "role-chip active" : "role-chip"}
                              onClick={() => setColumnRole(column.id, "locked")}
                            >
                              {text.lockedRole}
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="empty-state">{text.configureRolesFirst}</div>
          )}
            </>
          )}
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{uiLanguage === "zh-CN" ? "步骤 3" : "Step 3"}</p>
              <h3>{text.prompt}</h3>
            </div>
            <div className="button-group">
              <button
                type="button"
                className="secondary action-button"
                onClick={handleGeneratePrompt}
                disabled={!sheetState || busy !== "idle"}
              >
                {busy === "prompt" ? text.generating : text.generatePrompt}
              </button>
              <button
                type="button"
                className="panel-toggle"
                onClick={() => togglePanel("step3")}
              >
                {collapsedPanels.step3 ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
              </button>
            </div>
          </div>

          {!collapsedPanels.step3 && (
            <>
          <label className="field">
            <span>{text.prompt}</span>
            <textarea
              value={prompt}
              onChange={(event) => setPrompt(event.currentTarget.value)}
              rows={6}
            />
          </label>

          <div className="summary-grid">
            <div className="mini-card">
              <span>{text.sourceColumn}</span>
              <strong>{sourceColumn?.name || "-"}</strong>
            </div>
            <div className="mini-card">
              <span>{text.targetColumns}</span>
              <strong>{translatedColumns.map((column) => column.name).join(", ") || "-"}</strong>
            </div>
          </div>
          <details className="step-details">
            <summary>
              {uiLanguage === "zh-CN" ? "查看提示词优化逻辑" : "View Prompt Optimization Logic"}
            </summary>
            <div className="step-details-body">
              <p>
                {uiLanguage === "zh-CN"
                  ? "生成提示词时会抽样分析源语言文本，识别句长分布、占位符、富文本标签、换行和数字格式。"
                  : "Prompt generation samples source text and analyzes line length, placeholders, rich-text tags, line breaks, and numeric formats."}
              </p>
              <p>
                {uiLanguage === "zh-CN"
                  ? "执行翻译时会把你的提示词和固定翻译规则合并，并尽量只携带当前目标语言真正相关的术语上下文。"
                  : "During translation, your prompt is merged with fixed localization rules and only the glossary context relevant to the current targets is sent."}
              </p>
              <p>
                {uiLanguage === "zh-CN"
                  ? "检测面板里的完整 Prompt 和批次 Token 明细可以帮助定位：是提示词过长，还是批次拆分过细导致消耗偏高。"
                  : "The diagnostics panel shows the full prompt and per-batch token details so you can tell whether usage comes from oversized prompts or over-splitting."}
              </p>
            </div>
          </details>
            </>
          )}
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{uiLanguage === "zh-CN" ? "步骤 4" : "Step 4"}</p>
              <h3>{text.glossary}</h3>
            </div>
            <div className="button-group wrap">
              <button
                type="button"
                className="secondary action-button"
                onClick={handleGenerateGlossary}
                disabled={!sheetState || busy !== "idle"}
              >
                {busy === "glossary" ? text.generating : text.generateGlossary}
              </button>
              <button
                type="button"
                className="secondary action-button"
                onClick={handleFillGlossaryTranslations}
                disabled={!glossary.length || busy !== "idle"}
              >
                {uiLanguage === "zh-CN" ? "AI 填充翻译" : "AI Fill Translation"}
              </button>
              <button
                type="button"
                className="secondary action-button"
                onClick={addGlossaryTermsFromCurrentSheet}
                disabled={!sheetState || !sourceColumnId}
              >
                {uiLanguage === "zh-CN" ? "按子表批量添加" : "Batch Add From Sheet"}
              </button>
              <label className="field inline-field">
                <span>{uiLanguage === "zh-CN" ? "子表范围" : "Sheet Scope"}</span>
                <select
                  value={glossarySheet}
                  onChange={(event) => setGlossarySheet(event.currentTarget.value)}
                >
                  <option value={ALL_SHEETS_OPTION}>
                    {uiLanguage === "zh-CN" ? "全部子表" : "All Sheets"}
                  </option>
                  {sheetOptions.map((sheetName) => (
                    <option key={sheetName} value={sheetName}>
                      {sheetName}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                className="primary action-button"
                onClick={addGlossaryTerm}
                disabled={!sheetState}
              >
                {text.addTerm}
              </button>
              <button
                type="button"
                className="ghost action-button"
                onClick={() => void clearGlossary()}
                disabled={busy !== "idle"}
              >
                {uiLanguage === "zh-CN" ? "清空术语表" : "Clear Glossary"}
              </button>
              <button
                type="button"
                className="panel-toggle"
                onClick={() => togglePanel("step4")}
              >
                {collapsedPanels.step4 ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
              </button>
            </div>
          </div>
          {!collapsedPanels.step4 && (
            <>
          <div className="glossary-help">
            <p>
              <strong>{uiLanguage === "zh-CN" ? "同步模式" : "Sync Mode"}</strong>
              {uiLanguage === "zh-CN"
                ? glossaryLocalOnlyMode
                  ? "：当前为仅本地编辑。术语修改不会实时写飞书。"
                  : "：当前为实时同步。术语修改会自动写回飞书。"
                : glossaryLocalOnlyMode
                  ? ": Local-only mode. Glossary changes stay local."
                  : ": Live sync mode. Glossary changes are written back to Feishu automatically."}
            </p>
            <p>
              <strong>{uiLanguage === "zh-CN" ? "AI整理术语" : "Generate Glossary With AI"}</strong>
              {uiLanguage === "zh-CN"
                ? "：根据当前子表范围内的源语言内容，自动抽取可复用术语并新增到术语表。"
                : ": Extract reusable terms from the current sheet scope and add them into the glossary."}
            </p>
            <p>
              <strong>{uiLanguage === "zh-CN" ? "AI填充翻译" : "AI Fill Translation"}</strong>
              {uiLanguage === "zh-CN"
                ? "：只补全术语表里目标翻译列为空的单元格，不覆盖已有翻译，并参考当前提示词。"
                : ": Fill only empty target translation cells in the glossary, keep existing translations, and reference the current prompt."}
            </p>
          </div>

          <div className="glossary-table-shell">
              <table className="compact-table glossary-table">
                <thead>
                  <tr>
                    <th style={{ width: glossaryColumnWidths.enabled }}>
                      <div className="column-resize-head">
                        <span>{text.enabled}</span>
                        <div
                          className="resize-handle"
                          onMouseDown={(event) => startGlossaryColumnResize("enabled", event)}
                        />
                      </div>
                    </th>
                    <th style={{ width: glossaryColumnWidths.sourceSheet }}>
                      <div className="column-resize-head">
                        <span>{uiLanguage === "zh-CN" ? "来源子表" : "Source Sheet"}</span>
                        <div
                          className="resize-handle"
                          onMouseDown={(event) => startGlossaryColumnResize("sourceSheet", event)}
                        />
                      </div>
                    </th>
                    <th style={{ width: glossaryColumnWidths.term }}>
                      <div className="column-resize-head">
                        <span>{text.sourceTerm}</span>
                        <div
                          className="resize-handle"
                          onMouseDown={(event) => startGlossaryColumnResize("term", event)}
                        />
                      </div>
                    </th>
                    <th style={{ width: glossaryColumnWidths.category }}>
                      <div className="column-resize-head">
                        <span>{text.category}</span>
                        <div
                          className="resize-handle"
                          onMouseDown={(event) => startGlossaryColumnResize("category", event)}
                        />
                      </div>
                    </th>
                    <th style={{ width: glossaryColumnWidths.note }}>
                      <div className="column-resize-head">
                        <span>{text.note}</span>
                        <div
                          className="resize-handle"
                          onMouseDown={(event) => startGlossaryColumnResize("note", event)}
                        />
                      </div>
                    </th>
                    {translatedColumns.map((column) => (
                      <th
                        key={column.id}
                        style={{ width: getGlossaryColumnWidth(`translation:${column.id}`, glossaryColumnWidths.translation) }}
                      >
                        <div className="column-resize-head">
                          <span>{column.name}</span>
                          <div
                            className="resize-handle"
                            onMouseDown={(event) =>
                              startGlossaryColumnResize(`translation:${column.id}`, event)
                            }
                          />
                        </div>
                      </th>
                    ))}
                    <th>{uiLanguage === "zh-CN" ? "操作" : "Actions"}</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleGlossary.length ? (
                    visibleGlossary.map(({ term, index }) => (
                      <tr
                        key={term.glossaryId ?? `${term.term}-${index}`}
                        className={
                          glossarySyncingIds.includes(term.glossaryId ?? "")
                            ? "glossary-row syncing"
                            : "glossary-row"
                        }
                      >
                        <td>
                          <input
                            type="checkbox"
                            checked={term.enabled}
                            onChange={(event) =>
                              updateGlossaryTerm(index, {
                                ...term,
                                enabled: event.currentTarget.checked,
                              })
                            }
                          />
                        </td>
                        <td>
                          <input
                            value={term.sourceSheet ?? ""}
                            onChange={(event) =>
                              updateGlossaryField(index, {
                                ...term,
                                sourceSheet: event.currentTarget.value,
                              }, "sourceSheet")
                            }
                          />
                        </td>
                        <td>
                          <input
                            value={term.term}
                            onChange={(event) =>
                              updateGlossaryField(index, {
                                ...term,
                                term: event.currentTarget.value,
                              }, "term")
                            }
                          />
                        </td>
                        <td>
                          <input
                            value={term.category}
                            onChange={(event) =>
                              updateGlossaryField(index, {
                                ...term,
                                category: event.currentTarget.value,
                              }, "category")
                            }
                          />
                        </td>
                        <td>
                          <input
                            value={term.note}
                            onCompositionStart={() =>
                              handleGlossaryCompositionStart(term.glossaryId ?? createGlossaryId(term.term || `row-${index + 1}`), "note")
                            }
                            onCompositionEnd={(event) =>
                              handleGlossaryCompositionEnd(
                                index,
                                {
                                  ...term,
                                  note: event.currentTarget.value,
                                },
                                "note"
                              )
                            }
                            onChange={(event) =>
                              updateGlossaryField(index, {
                                ...term,
                                note: event.currentTarget.value,
                              }, "note")
                            }
                          />
                        </td>
                        {translatedColumns.map((column) => (
                          <td key={`${term.glossaryId ?? term.term}-${column.id}`}>
                            <input
                              value={term.translations[column.id] ?? ""}
                              onChange={(event) =>
                                updateGlossaryField(index, {
                                  ...term,
                                  localTargetClears: (term.localTargetClears ?? []).filter(
                                    (value) => value !== column.id
                                  ),
                                  translations: {
                                    ...term.translations,
                                    [column.id]: event.currentTarget.value,
                                  },
                                }, `translation:${column.id}`)
                              }
                            />
                          </td>
                        ))}
                        <td>
                          <div className="button-group wrap glossary-row-actions">
                            <button
                              type="button"
                              className="secondary action-button"
                              onClick={() => clearGlossaryTargetTranslations(index)}
                              disabled={busy !== "idle"}
                            >
                              {uiLanguage === "zh-CN" ? "清除目标列" : "Clear Targets"}
                            </button>
                            <button
                              type="button"
                              className="ghost action-button"
                              onClick={() => removeGlossaryTerm(index)}
                              disabled={busy !== "idle"}
                            >
                              {text.remove}
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))
                  ) : (
                    <tr>
                      <td colSpan={6 + translatedColumns.length} className="empty-glossary-row">
                        {uiLanguage === "zh-CN"
                          ? "术语表为空，点击“新增术语”添加一条空白术语条目。"
                          : 'Glossary is empty. Click "Add Term" to insert a blank glossary row.'}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            </>
          )}
        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{uiLanguage === "zh-CN" ? "步骤 5" : "Step 5"}</p>
              <h3>{text.translate}</h3>
            </div>
            <div className="button-group">
              <button
                type="button"
                className="success action-button"
                onClick={() => void handleTranslate()}
                disabled={!sheetState || busy !== "idle"}
              >
                {busy === "translating" ? text.translating : text.translateAndWriteBack}
              </button>
              {!!summary?.failedRowIds?.length && (
                <button type="button" className="ghost action-button" disabled={!sheetState || busy !== "idle"}
                  onClick={() => void handleTranslate(true)}>
                  {uiLanguage === "zh-CN" ? "仅重试失败行" : "Retry failed rows"}
                </button>
              )}
              <button
                type="button"
                className="ghost action-button"
                onClick={handleStopTranslation}
                disabled={busy !== "translating"}
              >
                {uiLanguage === "zh-CN" ? "停止翻译" : "Stop Translation"}
              </button>
              <button
                type="button"
                className="panel-toggle"
                onClick={() => togglePanel("step5")}
              >
                {collapsedPanels.step5 ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
              </button>
            </div>
          </div>

          {!collapsedPanels.step5 && (
            <>
              <div className="mode-strip">
                <button
                  type="button"
                  className={writeMode === "fill_empty" ? "mode-chip active" : "mode-chip"}
                  onClick={() => setWriteMode("fill_empty")}
                >
                  {text.fillEmptyOnly}
                </button>
                <button
                  type="button"
                  className={writeMode === "overwrite_unlocked" ? "mode-chip active" : "mode-chip"}
                  onClick={() => setWriteMode("overwrite_unlocked")}
                >
                  {text.overwriteUnlocked}
                </button>
              </div>

              <details className="step-details">
                <summary>
                  {uiLanguage === "zh-CN" ? "动态分批" : "Dynamic Batching"}
                </summary>
                <div className="step-details-body">
                  <div className="dynamic-batching-panel">
                    <label className="field tight">
                      <span>
                        {uiLanguage === "zh-CN"
                          ? "估算 Token 上限/批"
                          : "Estimated Token Budget / Batch"}
                      </span>
                      <input
                        type="number"
                        min={0}
                        step={1000}
                        value={translationTokenBudget}
                        onChange={(event) =>
                          setTranslationTokenBudget(Math.max(0, Number(event.currentTarget.value || 0)))
                        }
                      />
                      <small className="field-hint">
                        {uiLanguage === "zh-CN"
                          ? "填 0 表示自动估算。"
                          : "Use 0 for automatic estimation."}
                      </small>
                    </label>
                  </div>
                  <p>
                    {uiLanguage === "zh-CN"
                      ? "这个功能用来控制每一批翻译请求允许装多少内容。数值越小，每批更短更稳，但批次数更多；数值越大，每批更大，请求次数更少，但单次更重，失败重试成本也更高。"
                      : "This controls how much content each translation batch may contain. Smaller values produce shorter, more stable batches but increase request count. Larger values reduce request count but make each batch heavier and more expensive to retry."}
                  </p>
                  <p>
                    {uiLanguage === "zh-CN"
                      ? "填 0 会让软件自动估算；你也可以手动指定上限。可以把它理解成“每次喂给 AI 的包有多大”。"
                      : "Use 0 for automatic estimation, or set a manual limit. You can think of it as the size of each package sent to the model."}
                  </p>
                  <p>
                    {uiLanguage === "zh-CN"
                      ? "建议范围：8000-12000 偏稳，适合目标语言较多或文本较长；12000-24000 比较均衡，适合大多数本地化表；24000-40000 偏激进，适合短 UI 文案。"
                      : "Suggested range: 8000-12000 is conservative and good for many targets or longer text; 12000-24000 is balanced for most localization sheets; 24000-40000 is more aggressive and better for short UI text."}
                  </p>
                </div>
              </details>

              {writeMode === "overwrite_unlocked" && (
                <p className="section-tip">
                  {uiLanguage === "zh-CN"
                    ? "覆盖未锁定列会直接根据源语言、术语表和辅助上下文重翻未锁定目标列，不再把旧目标译文发给 AI，以减少 token 消耗。"
                    : "Overwrite Unlocked regenerates unlocked target columns from the source text, glossary, and row context without sending old target translations, reducing token usage."}
                </p>
              )}

              {writeMode === "fill_empty" && (
                <div className="fill-empty-preview">
                  <div className="summary-grid">
                    <div className="mini-card">
                      <span>{uiLanguage === "zh-CN" ? "涉及条目行" : "Affected Rows"}</span>
                      <strong>{fillEmptyPreviewRows.length}</strong>
                    </div>
                    <div className="mini-card">
                      <span>{text.sourceColumn}</span>
                      <strong>{sourceColumn?.name || "-"}</strong>
                    </div>
                    <div className="mini-card">
                      <span>{text.targetColumns}</span>
                      <strong>{translatedColumns.map((column) => column.name).join(", ") || "-"}</strong>
                    </div>
                  </div>

                  {fillEmptyPreviewRows.length ? (
                    <div className="table-shell fill-empty-table-shell">
                      <table className="compact-table fill-empty-table">
                        <thead>
                          <tr>
                            <th>{uiLanguage === "zh-CN" ? "Key / 行标识" : "Key / Row ID"}</th>
                            <th>{uiLanguage === "zh-CN" ? "子表" : "Sheet"}</th>
                            <th>{uiLanguage === "zh-CN" ? "行号" : "Row"}</th>
                            <th>{text.sourceColumn}</th>
                            <th>{uiLanguage === "zh-CN" ? "待填目标列" : "Missing Targets"}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {fillEmptyPreviewRows.map((item) => (
                            <tr key={item.id}>
                              <td>{item.key}</td>
                              <td>{item.sheetName}</td>
                              <td>{item.rowNumber}</td>
                              <td>{item.source}</td>
                              <td>{item.missingTargets.join(", ")}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : (
                    <div className="empty-state">
                      {uiLanguage === "zh-CN"
                        ? "当前没有命中“仅填空白”的条目行。"
                        : 'No rows currently match the "Fill Empty Only" mode.'}
                    </div>
                  )}
                </div>
              )}
            </>
          )}

        </section>

        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{text.result}</p>
              <h3>{text.summaryAndRollback}</h3>
            </div>
            <div className="button-group">
              <button
                type="button"
                className="secondary action-button"
                onClick={handleRollback}
                disabled={!snapshotRows}
              >
                {text.rollback}
              </button>
              <button
                type="button"
                className="panel-toggle"
                onClick={() => togglePanel("result")}
              >
                {collapsedPanels.result ? (uiLanguage === "zh-CN" ? "展开" : "Expand") : (uiLanguage === "zh-CN" ? "折叠" : "Collapse")}
              </button>
            </div>
          </div>

          {!collapsedPanels.result && (
            <>
          {summary ? (
            <div className="summary-grid">
              <div className="mini-card">
                <span>{text.processedRows}</span>
                <strong>{summary.processedRows}</strong>
              </div>
              <div className="mini-card">
                <span>{text.skippedRows}</span>
                <strong>{summary.skippedRows}</strong>
              </div>
              <div className="mini-card">
                <span>{text.failedRows}</span>
                <strong>{summary.failedRows.length}</strong>
              </div>
            </div>
          ) : (
            <div className="empty-state">{text.noSummary}</div>
          )}

          <div className="history-panel">
            <h4>{text.recentRuns}</h4>
            {history.length ? (
              <ul>
                {history.map((item) => (
                  <li key={item.id}>
                    <strong>{item.timestamp}</strong>
                    <span>
                      {text.historyLine(
                        modeLabel(item.mode),
                        item.processedRows,
                        item.failedRows
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p>{text.noRuns}</p>
            )}
          </div>
            </>
          )}
        </section>

        {false && (
        <section className="panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">{uiLanguage === "zh-CN" ? "Prompt Debug" : "Prompt Debug"}</p>
              <h3>{uiLanguage === "zh-CN" ? "调试信息" : "Debug Info"}</h3>
            </div>
          </div>
          <div className="debug-grid">
            <div className="debug-card">
              <span>{uiLanguage === "zh-CN" ? "当前输入 Prompt" : "Current Input Prompt"}</span>
              <pre>{prompt || (uiLanguage === "zh-CN" ? "当前为空。" : "Empty.")}</pre>
            </div>
            <div className="debug-card">
              <span>{uiLanguage === "zh-CN" ? "最近一次实际发送给模型的完整 Prompt" : "Last Full Prompt Sent To Model"}</span>
              <strong>{lastDebugMessage || (uiLanguage === "zh-CN" ? "尚未发起模型请求" : "No model request yet")}</strong>
              <pre>{lastDebugPrompt || (uiLanguage === "zh-CN" ? "这里会显示系统提示词 + 用户内容的完整拼装结果。" : "The full system + user prompt will appear here.")}</pre>
            </div>
            <div className="debug-card">
              <span>{uiLanguage === "zh-CN" ? "最近一次请求 Token 增量" : "Last Request Token Delta"}</span>
              <div className="debug-metrics">
                <strong>{uiLanguage === "zh-CN" ? "输入" : "Prompt"}: {lastUsageDelta.promptTokens}</strong>
                <strong>{uiLanguage === "zh-CN" ? "输出" : "Completion"}: {lastUsageDelta.completionTokens}</strong>
                <strong>Total: {lastUsageDelta.totalTokens}</strong>
              </div>
              <p className="debug-note">
                {uiLanguage === "zh-CN"
                  ? "如果这里频繁跳很多次，通常说明当前任务被按目标语言列或批次拆成了多次请求。"
                  : "If this jumps many times, the task is likely being split into multiple target-column or chunked requests."}
              </p>
            </div>
          </div>
        </section>
        )}
      </main>

      {toast && <div className={`toast ${toast.tone}`}>{toast.message}</div>}
    </div>
  );
}

export default App;
