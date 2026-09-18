import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// 类型化 IPC 包装（T3）：命令名与参数映射严格对照 spec 6.1；事件订阅对照 6.2。
// 任意后端命令/事件的类型与线缆键名以 src-tauri/src/events.rs 为唯一权威（本文件仅调用）。
import { EVENTS } from './types';
import type {
  CancelSelectionCommand,
  CancelTranslationCommand,
  CompleteSelectionCommand,
  GetMaskSnapshotCommand,
  HotkeyFunction,
  MaskBeginPayload,
  MaskSnapshotResult,
  OnboardingBeginPayload,
  OcrLanguage,
  PanelShowPayload,
  RebindHotkeyCommand,
  ReocrCommand,
  ReocrResult,
  SelectionClosedPayload,
  SendOcrToTranslateCommand,
  SetPanelTargetLangCommand,
  SettingsChangedPayload,
  SettingsPatch,
  SettingsView,
  TranslateCommand,
  TranslateResult,
  TranslationBeginPayload,
  TranslationChunkPayload,
  TranslationDonePayload,
  TranslationErrorPayload,
  OcrResultPayload,
  VerifyConnectionCommand,
} from './types';

// ---------------------------------------------------------------------------
// 6.1 命令（前端→Rust）
// ---------------------------------------------------------------------------

/** get_settings → SettingsView（INV-1：无 Key 明文/密文）。 */
export function getSettings(): Promise<SettingsView> {
  return invoke<SettingsView>('get_settings');
}

/** set_settings：通用设置即时保存生效（FR-13）；热键变更不走此通道（6.1）。 */
export function setSettings(patch: SettingsPatch): Promise<void> {
  return invoke('set_settings', { patch });
}

/** rebind_hotkey：改键专用（回退语义在 Rust 侧，5.7.1）。 */
export function rebindHotkey(command: RebindHotkeyCommand): Promise<void> {
  return invoke('rebind_hotkey', { function: command.function, combo: command.combo });
}

/** set_clipboard_monitor：三方开关统一入口（FR-5.1，INV-8）。 */
export function setClipboardMonitor(enabled: boolean): Promise<void> {
  return invoke('set_clipboard_monitor', { enabled });
}

/**
 * set_panel_target_lang（T12 追加命令）：面板定向语言下拉临时切换（FR-2.7）——
 * 仅写后端易失槽位，不回写设置默认值；lang 缺省＝清除回落设置页默认值。
 */
export function setPanelTargetLang(command: SetPanelTargetLangCommand): Promise<void> {
  return invoke('set_panel_target_lang', { lang: command.lang ?? null });
}

/** translate：发起翻译（流式结果经 translation://* 事件返回）。 */
export function translate(command: TranslateCommand): Promise<TranslateResult> {
  return invoke('translate', { text: command.text, source: command.source });
}

/** cancel_translation：显式取消。 */
export function cancelTranslation(command: CancelTranslationCommand): Promise<void> {
  return invoke('cancel_translation', { roundId: command.roundId });
}

/** open_settings：EX-2 入口；单例前置（窗口复用）。 */
export function openSettings(): Promise<void> {
  return invoke('open_settings');
}

/** complete_selection：框选确认（rect 为该屏局部物理像素）。 */
export function completeSelection(command: CompleteSelectionCommand): Promise<void> {
  return invoke('complete_selection', {
    sessionId: command.sessionId,
    monitorIndex: command.monitorIndex,
    rect: command.rect,
  });
}

/** cancel_selection：Esc 取消。 */
export function cancelSelection(command: CancelSelectionCommand): Promise<void> {
  return invoke('cancel_selection', { sessionId: command.sessionId });
}

/**
 * get_mask_snapshot（T9 追加命令）：本屏冻结帧 → data: URL 内存字符串（INV-3 例外说明见
 * events.rs 注释：不落盘、无 HTTP，绘制后即弃）。monitorIndex 由遮罩页 URL 查询参数提供。
 */
export function getMaskSnapshot(command: GetMaskSnapshotCommand): Promise<MaskSnapshotResult> {
  return invoke('get_mask_snapshot', { monitorIndex: command.monitorIndex });
}

/** reocr：对保留裁剪块换语种重识别（FR-6.2）。 */
export function reocr(command: ReocrCommand): Promise<ReocrResult> {
  return invoke('reocr', { sessionId: command.sessionId, lang: command.lang });
}

/** send_ocr_to_translate：一键送翻译（FR-6.5）。 */
export function sendOcrToTranslate(command: SendOcrToTranslateCommand): Promise<TranslateResult> {
  return invoke('send_ocr_to_translate', { sessionId: command.sessionId });
}

/** list_ocr_languages：已装语言包（设置下拉数据源，防 EX-6）。 */
export function listOcrLanguages(): Promise<OcrLanguage[]> {
  return invoke('list_ocr_languages');
}

/**
 * hotkey_registration_failures（T13 新增契约）：启动注册失败（=降级态）功能清单——
 * 设置页快捷键行尾常驻 warning 图标数据源（design-spec 7.2）；空数组＝全部注册成功。
 */
export function hotkeyRegistrationFailures(): Promise<HotkeyFunction[]> {
  return invoke('hotkey_registration_failures');
}

/**
 * open_config_guide（T14 新增契约）：FR-11.4 配置引导——guide 模式复用引导窗体，
 * 直达隐私要点 + Key 步骤；补配完成后由后端回调通知（T16 action_router 接线）。
 */
export function openConfigGuide(): Promise<void> {
  return invoke('open_config_guide');
}

/** finish_onboarding（T14）：完成/跳过——写 meta.onboardingCompleted 并隐藏窗体（FR-11.3/11.5）。 */
export function finishOnboarding(): Promise<void> {
  return invoke('finish_onboarding');
}

/** verify_connection：连接验证（D6：先 GET {base}/models，404/405 回退最小 chat）。 */
export function verifyConnection(command: VerifyConnectionCommand): Promise<void> {
  return invoke('verify_connection', {
    base: command.base ?? null,
    model: command.model ?? null,
    key: command.key ?? null,
  });
}

// ---------------------------------------------------------------------------
// 6.2 事件订阅（Rust→窗口）
// ---------------------------------------------------------------------------

/** 通用订阅（事件名与载荷类型一一对应）。 */
export async function subscribeEvent<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (eventData) => handler(eventData.payload));
}

/** panel://show：面板显示指令（mode/text/sessionId）。 */
export function onPanelShow(handler: (payload: PanelShowPayload) => void): Promise<UnlistenFn> {
  return subscribeEvent<PanelShowPayload>(EVENTS.panelShow, handler);
}

/** translation://begin：新轮开始（renderMode 由 Rust 分档判定，C1）。 */
export function onTranslationBegin(
  handler: (payload: TranslationBeginPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<TranslationBeginPayload>(EVENTS.translationBegin, handler);
}

/** translation://chunk：流式增量。 */
export function onTranslationChunk(
  handler: (payload: TranslationChunkPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<TranslationChunkPayload>(EVENTS.translationChunk, handler);
}

/** translation://done：本轮终态。 */
export function onTranslationDone(
  handler: (payload: TranslationDonePayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<TranslationDonePayload>(EVENTS.translationDone, handler);
}

/** translation://error：错误态（ex_code/retriable/partial 依 5.8 呈现）。 */
export function onTranslationError(
  handler: (payload: TranslationErrorPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<TranslationErrorPayload>(EVENTS.translationError, handler);
}

/** ocr://result：识别结果。 */
export function onOcrResult(handler: (payload: OcrResultPayload) => void): Promise<UnlistenFn> {
  return subscribeEvent<OcrResultPayload>(EVENTS.ocrResult, handler);
}

/** settings://changed：配置变更镜像（INV-8 三方同步）。 */
export function onSettingsChanged(
  handler: (payload: SettingsChangedPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<SettingsChangedPayload>(EVENTS.settingsChanged, handler);
}

/** selection://closed：截图会话关闭（面板释放会话态）。 */
export function onSelectionClosed(
  handler: (payload: SelectionClosedPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<SelectionClosedPayload>(EVENTS.selectionClosed, handler);
}

/** mask://begin（T9 追加）：新框选会话开始——遮罩页据此重新拉取冻结帧。 */
export function onMaskBegin(handler: (payload: MaskBeginPayload) => void): Promise<UnlistenFn> {
  return subscribeEvent<MaskBeginPayload>(EVENTS.maskBegin, handler);
}

/** onboarding://begin（T14 追加）：引导窗复位指令（复用窗残留状态规避；模式驱动步骤定位）。 */
export function onOnboardingBegin(
  handler: (payload: OnboardingBeginPayload) => void,
): Promise<UnlistenFn> {
  return subscribeEvent<OnboardingBeginPayload>(EVENTS.onboardingBegin, handler);
}

/** 面板隐藏（FR-1.3 关闭=隐藏）。真机修复轮 2：窗口插件 API（window:hide）被 ACL 拒绝，改走自定义命令。 */
export function panelHide(): Promise<void> {
  return invoke('panel_hide');
}

/** 顶栏拖拽（真机修复轮 2）：window:start_dragging 同被 ACL 拒绝，改走自定义命令。 */
export function panelStartDrag(): Promise<void> {
  return invoke('panel_start_drag');
}

/**
 * 译文内容自适应高度（真机修复轮 13）：传入"译文不滚动所需的窗口总高"（CSS 逻辑像素，
 * 即 innerHeight − 译文可视高 + 译文内容自然高）。后端按 max(用户偏好, 内容所需) 钳上限应用，
 * 不持久化（临时生长不覆盖用户拖拽的偏好尺寸）。
 */
export function panelResizeForContent(contentHeight: number): Promise<void> {
  return invoke('panel_resize_for_content', { contentHeight });
}

/** 恢复全部设置为出厂默认值（清单 P3-11；后端保留 API Key/引导状态/界面语言）。 */
export function restoreDefaultSettings(): Promise<void> {
  return invoke('restore_default_settings');
}
