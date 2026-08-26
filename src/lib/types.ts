// 契约类型：src-tauri/src/events.rs 的 TS 手工镜像（spec 6.1 命令契约 / 6.2 事件契约）。
// 与 events.rs 同步修改（无代码生成）；字段名即线缆 JSON 键名（snake_case 仅 ex_code 一处，
// 按 spec 6.2 字面保留下划线）。
//
// 安全纪律：
// - INV-1：本文件不存在任何承载 API Key 明文/密文的响应字段——`SettingsView.ai.hasKey`
//   是前端能取得的唯一 Key 状态；写入经 `set_settings` 的 `AiPatch.apiKey`（仅用户输入，进 Rust 即加密）。
// - INV-3：截图位图不经这些类型传递（只传 sessionId 与文本）。

// ---------------------------------------------------------------------------
// 事件名常量（唯一事件名源，与 events.rs 同值；订阅包装在 ipc.ts）
// ---------------------------------------------------------------------------

export const EVENTS = {
  panelShow: 'panel://show',
  translationBegin: 'translation://begin',
  translationChunk: 'translation://chunk',
  translationDone: 'translation://done',
  translationError: 'translation://error',
  ocrResult: 'ocr://result',
  settingsChanged: 'settings://changed',
  selectionClosed: 'selection://closed',
} as const;

// ---------------------------------------------------------------------------
// 枚举（序列化值）
// ---------------------------------------------------------------------------

/** 翻译源入口（6.1 translate.source）。 */
export type TranslateSource = 'selection' | 'input' | 'clipboard' | 'ocr';

/** 面板呈现形态（6.2 panel://show.mode）。 */
export type PanelMode = 'selection' | 'input' | 'clipboard' | 'ocr';

/** 呈现模式（C1 裁决：translation://begin.renderMode）。 */
export type RenderMode = 'stream' | 'block';

/** 可改热键功能（FR-14.2 五功能；与 6.3 hotkeys 键名同风格）。 */
export type HotkeyFunction =
  'selection' | 'input' | 'ocrRecognize' | 'ocrTranslate' | 'toggleScene';

/** 活动场景（6.3 scene.active）。 */
export type SceneMode = 'mutual' | 'directed';

/** 译文呈现（6.3 translation.presentation）。 */
export type PresentationMode = 'stream_all' | 'staged';

/** 主题（6.3 general.theme）。 */
export type ThemeMode = 'light' | 'dark' | 'system';

// ---------------------------------------------------------------------------
// 6.1 命令参数 / 返回
// ---------------------------------------------------------------------------

/** get_settings 返回（INV-1：ai 仅 baseUrl/model/hasKey，无任何 Key 值）。 */
export interface AiSettingsView {
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

export interface SettingsView {
  schemaVersion: number;
  ai: AiSettingsView;
  translation: TranslationSettings;
  panel: PanelSettings;
  clipboardMonitor: boolean;
  scene: SceneSettings;
  hotkeys: HotkeySettings;
  ocr: OcrSettings;
  general: GeneralSettings;
  meta: MetaSettings;
}

/** 与 config 模块 Settings 子树同构（camelCase 键）。 */
export interface TranslationSettings {
  defaultTargetLang: string;
  presentation: PresentationMode;
  longTextThresholdChars: number;
}

export interface PanelSettings {
  hideOnBlur: boolean;
  widthLogical: number;
  heightLogical: number;
}

export interface SceneSettings {
  active: SceneMode;
}

export interface HotkeySettings {
  selection: string;
  input: string;
  ocrRecognize: string;
  ocrTranslate: string;
  toggleScene: string;
}

export interface OcrSettings {
  language: string;
}

export interface GeneralSettings {
  autostart: boolean;
  theme: ThemeMode;
}

export interface MetaSettings {
  onboardingCompleted: boolean;
}

/** set_settings 补丁 / settings://changed 载荷（全部可选：缺省＝不变更）。 */
export interface SettingsPatch {
  ai?: AiPatch;
  translation?: TranslationPatch;
  panel?: PanelPatch;
  clipboardMonitor?: boolean;
  scene?: ScenePatch;
  hotkeys?: HotkeysPatch;
  ocr?: OcrPatch;
  general?: GeneralPatch;
  meta?: MetaPatch;
}

/** 仅写入通道可带 apiKey（用户输入明文，后端立即 DPAPI 加密；广播载荷必须剥离）。 */
export interface AiPatch {
  baseUrl?: string;
  model?: string;
  apiKey?: string;
}

export interface TranslationPatch {
  defaultTargetLang?: string;
  presentation?: PresentationMode;
  longTextThresholdChars?: number;
}

export interface PanelPatch {
  hideOnBlur?: boolean;
  widthLogical?: number;
  heightLogical?: number;
}

export interface ScenePatch {
  active?: SceneMode;
}

export interface HotkeysPatch {
  selection?: string;
  input?: string;
  ocrRecognize?: string;
  ocrTranslate?: string;
  toggleScene?: string;
}

export interface OcrPatch {
  language?: string;
}

export interface GeneralPatch {
  autostart?: boolean;
  theme?: ThemeMode;
}

export interface MetaPatch {
  onboardingCompleted?: boolean;
}

/** settings://changed 载荷类型（与 SettingsPatch 语义相同，命名显式区分广播）。 */
export type SettingsChangedPayload = SettingsPatch;

/** translate 参数（6.1：{text, source} → {roundId}）。 */
export interface TranslateCommand {
  text: string;
  source: TranslateSource;
}

/** translate / send_ocr_to_translate 返回。 */
export interface TranslateResult {
  roundId: string;
}

export interface CancelTranslationCommand {
  roundId: string;
}

export interface RebindHotkeyCommand {
  function: HotkeyFunction;
  combo: string;
}

export interface SetClipboardMonitorCommand {
  enabled: boolean;
}

/** 选区矩形（所在显示器局部物理像素，INV-4）。 */
export interface SelectionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CompleteSelectionCommand {
  sessionId: string;
  monitorIndex: number;
  rect: SelectionRect;
}

export interface CancelSelectionCommand {
  sessionId: string;
}

export interface ReocrCommand {
  sessionId: string;
  lang: string;
}

export interface ReocrResult {
  text: string;
}

export interface SendOcrToTranslateCommand {
  sessionId: string;
}

/** verify_connection 参数（缺省＝用已存值；key 为用户输入，日志不得输出）。 */
export interface VerifyConnectionCommand {
  base?: string;
  model?: string;
  key?: string;
}

export interface OcrLanguage {
  tag: string;
  label: string;
}

// ---------------------------------------------------------------------------
// 6.2 事件载荷（Rust→窗口）
// ---------------------------------------------------------------------------

export interface PanelShowPayload {
  mode: PanelMode;
  text?: string;
  sessionId?: string;
}

export interface TranslationBeginPayload {
  roundId: string;
  renderMode: RenderMode;
}

export interface TranslationChunkPayload {
  roundId: string;
  delta: string;
}

export interface TranslationDonePayload {
  roundId: string;
}

export interface TranslationErrorPayload {
  roundId: string;
  /** spec 6.2 字面保留下划线；值 "EX-1".."EX-14"（error.rs AppError::ex_code）。 */
  ex_code: string;
  message: string;
  retriable: boolean;
  /** EX-5 保留的已收增量。 */
  partial?: string;
}

export interface OcrResultPayload {
  sessionId: string;
  text: string;
}

export interface SelectionClosedPayload {
  sessionId: string;
}
