// 配置镜像 store（T12 落位：面板/设置/托盘三方同步的前端侧，INV-8）。
//
// 约定（占位文件既已声明）：本 store 只保存「前端展示态镜像」，真实状态以 Rust config 为
// 单一事实源，经 `settings://changed` 事件同步，禁止前端自行改状态（写动作只经 IPC 命令，
// 回读经事件广播）。未经注册命令期（T13 get_settings 之前）镜像为 null，各消费者按保守
// 缺省渲染（互译 / 关闭 / system），不声明此为配置默认值权威源（权威源在 src-tauri config）。
//
// 主题三态（FR-1.2）：general.theme 驱动 documentElement[data-theme]（theme.css 双套 Token）；
// system 跟随系统偏好并监听变化。设置页（T13）与面板共用本套逻辑。

import { getSettings, onSettingsChanged } from '../ipc';
import type { SceneMode, SettingsPatch, SettingsView, ThemeMode } from '../types';

/** 配置镜像（null = 尚未取得镜像；INV-8 只读展示态）。
 * Svelte 5 规则：模块导出的 $state 不得整体重赋值——以对象属性承载值（属性变更即响应）。 */
export const settingsState = $state<{ current: SettingsView | null }>({ current: null });

/** 恢复/替换镜像（测试辅助；settingsChanged 监听由各窗口自行挂取）。 */
export function resetSettingsMirror(): void {
  applySettingsView(null);
}

/** 以完整视图替换镜像（initSettingsMirror 与测试共用；主题随之生效）。 */
export function applySettingsView(view: SettingsView | null): void {
  // 属性级替换（导出 $state 对象禁止整体重赋值；patch 语义与 apply 语义均在此属性承载）。
  settingsState.current = view;
  if (view) applyThemeAppearance(view.general.theme);
}

/** 把 settings://changed 补丁并入镜像并同步主题。patch 字段缺省＝不变更（6.2 契约）。 */
export function mergeSettingsPatch(patch: SettingsPatch): void {
  const settings = settingsState.current;
  if (!settings) return;
  if (patch.clipboardMonitor !== undefined) settings.clipboardMonitor = patch.clipboardMonitor;
  if (patch.scene?.active !== undefined) settings.scene.active = patch.scene.active;
  if (patch.general) {
    if (patch.general.autostart !== undefined) settings.general.autostart = patch.general.autostart;
    if (patch.general.theme !== undefined) {
      settings.general.theme = patch.general.theme;
      applyThemeAppearance(patch.general.theme);
    }
  }
  if (patch.ai) {
    if (patch.ai.baseUrl !== undefined) settings.ai.baseUrl = patch.ai.baseUrl;
    if (patch.ai.model !== undefined) settings.ai.model = patch.ai.model;
    // apiKey 写入通道不进入广播（INV-1 剥离纪律由后端发射点保证）；hasKey 广播无键（T13 定义）
  }
  if (patch.translation) {
    if (patch.translation.defaultTargetLang !== undefined)
      settings.translation.defaultTargetLang = patch.translation.defaultTargetLang;
    if (patch.translation.presentation !== undefined)
      settings.translation.presentation = patch.translation.presentation;
    if (patch.translation.longTextThresholdChars !== undefined)
      settings.translation.longTextThresholdChars = patch.translation.longTextThresholdChars;
  }
  if (patch.panel) {
    if (patch.panel.hideOnBlur !== undefined) settings.panel.hideOnBlur = patch.panel.hideOnBlur;
    if (patch.panel.widthLogical !== undefined)
      settings.panel.widthLogical = patch.panel.widthLogical;
    if (patch.panel.heightLogical !== undefined)
      settings.panel.heightLogical = patch.panel.heightLogical;
  }
  if (patch.hotkeys) {
    if (patch.hotkeys.selection !== undefined) settings.hotkeys.selection = patch.hotkeys.selection;
    if (patch.hotkeys.input !== undefined) settings.hotkeys.input = patch.hotkeys.input;
    if (patch.hotkeys.ocrRecognize !== undefined)
      settings.hotkeys.ocrRecognize = patch.hotkeys.ocrRecognize;
    if (patch.hotkeys.ocrTranslate !== undefined)
      settings.hotkeys.ocrTranslate = patch.hotkeys.ocrTranslate;
    if (patch.hotkeys.toggleScene !== undefined)
      settings.hotkeys.toggleScene = patch.hotkeys.toggleScene;
  }
  if (patch.ocr && patch.ocr.language !== undefined) settings.ocr.language = patch.ocr.language;
  if (patch.meta && patch.meta.onboardingCompleted !== undefined)
    settings.meta.onboardingCompleted = patch.meta.onboardingCompleted;
}

/** 首次镜像拉取（get_settings；命令未注册期 T13 前宽容失败，镜像保持 null）。 */
export async function initSettingsMirror(): Promise<void> {
  try {
    applySettingsView(await getSettings());
  } catch {
    // 命令未注册（T13 之前）/运行时异常：保持 null，消费端按保守缺省渲染而非报错。
  }
}

/** 订阅 settings://changed 广播（INV-8 三方同步；返回退订函数）。 */
export async function watchSettingsChanged(): Promise<() => void> {
  return onSettingsChanged(mergeSettingsPatch);
}

function prefersDark(): boolean {
  // jsdom 未实现 matchMedia（vitest 环境）；缺省 false 即亮色，仅测试路径。
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
}

/** 主题生效：light/dark 直写；system 按系统偏好折算（切换时由调用方再触发）。 */
export function applyThemeAppearance(theme: ThemeMode): void {
  const dark = theme === 'dark' || (theme === 'system' && prefersDark());
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
}

/** 监听系统偏好（仅 theme=system 时生效）；返回退订函数（无 matchMedia 环境＝no-op）。 */
export function watchSystemTheme(): () => void {
  if (!window.matchMedia) return () => {};
  const mql = window.matchMedia('(prefers-color-scheme: dark)');
  const onChange = () => {
    const theme = settingsState.current?.general.theme ?? 'system';
    if (theme === 'system') applyThemeAppearance('system');
  };
  mql.addEventListener('change', onChange);
  return () => mql.removeEventListener('change', onChange);
}

/** 活动场景（读镜像；镜像缺失时保守默认互译——权威源在 Rust config）。 */
export function currentScene(): SceneMode {
  return settingsState.current?.scene.active ?? 'mutual';
}
