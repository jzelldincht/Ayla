// 全局 stores（T12 落位）：配置镜像与主题联动（src/lib/stores/settings.svelte.ts）。
// 约定（INV-8）：store 只保存「前端展示态镜像」，真实状态以 Rust config 为单一事实源，
// 经 `settings://changed` 事件同步，禁止前端自行改状态。
export {
  settingsState,
  applySettingsView,
  initSettingsMirror,
  mergeSettingsPatch,
  resetSettingsMirror,
  watchSettingsChanged,
  watchSystemTheme,
  applyThemeAppearance,
  currentScene,
} from './settings.svelte';
