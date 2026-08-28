// 顶栏测试：监听开关镜像（INV-8：值只读镜像 store、写只经 IPC 命令、回流经 settings://changed 广播）、
// 场景分段切换、定向语言下拉（Q5 九种）、幽灵关闭。
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import TopBar from './TopBar.svelte';
import { applySettingsView, mergeSettingsPatch, resetSettingsMirror } from '../lib/stores';
import { DIRECTED_TARGET_LANGS, type SettingsView } from '../lib/types';

const mocks = vi.hoisted(() => ({
  setClipboardMonitor: vi.fn(() => Promise.resolve()),
  setSettings: vi.fn(() => Promise.resolve()),
  setPanelTargetLang: vi.fn(() => Promise.resolve()),
  startDragging: vi.fn(() => Promise.resolve()),
}));

vi.mock('../lib/ipc', () => ({
  setClipboardMonitor: mocks.setClipboardMonitor,
  setSettings: mocks.setSettings,
  setPanelTargetLang: mocks.setPanelTargetLang,
  getSettings: vi.fn(() => Promise.resolve(null)),
  onSettingsChanged: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ startDragging: mocks.startDragging }),
}));

afterEach(() => {
  cleanup();
  resetSettingsMirror();
});

beforeEach(() => {
  vi.clearAllMocks();
});

function fakeSettings(overrides: Partial<SettingsView> = {}): SettingsView {
  return {
    schemaVersion: 1,
    ai: { baseUrl: 'https://api.deepseek.com', model: 'deepseek-chat', hasKey: false },
    translation: {
      defaultTargetLang: '英文',
      presentation: 'stream_all',
      longTextThresholdChars: 1000,
    },
    panel: { hideOnBlur: true, widthLogical: 460, heightLogical: 340 },
    clipboardMonitor: false,
    scene: { active: 'mutual' },
    hotkeys: {
      selection: 'Alt+Q',
      input: 'Alt+E',
      ocrRecognize: 'Alt+S',
      ocrTranslate: 'Alt+D',
      toggleScene: 'Alt+W',
    },
    ocr: { language: 'auto' },
    general: { autostart: false, theme: 'system' },
    meta: { onboardingCompleted: false },
    ...overrides,
  };
}

function renderTopBar() {
  const onTargetChange = vi.fn();
  const onClose = vi.fn();
  render(TopBar, { props: { tempTarget: null, onTargetChange, onClose } });
  return { onTargetChange, onClose };
}

describe('顶栏监听开关（INV-8 三方同步镜像）', () => {
  it('镜像缺省（监视关闭）→ aria-checked=false；镜像更新为开 → 同步为开', async () => {
    renderTopBar();
    const sw = screen.getByRole('switch', { name: '剪贴板监听' });
    expect(sw.getAttribute('aria-checked')).toBe('false');

    applySettingsView(fakeSettings({ clipboardMonitor: true }));
    await tick();
    expect(sw.getAttribute('aria-checked')).toBe('true');
  });

  it('点击开关仅发 IPC 命令，不乐观改值；广播回流（settings://changed）后镜像更新', async () => {
    renderTopBar();
    applySettingsView(fakeSettings({ clipboardMonitor: true }));
    await tick();
    const sw = screen.getByRole('switch', { name: '剪贴板监听' });
    fireEvent.click(sw);
    expect(mocks.setClipboardMonitor).toHaveBeenCalledWith(false);

    // 后端广播回流（Invoke 结果经 settings://changed，INV-8 禁前端自改）
    mergeSettingsPatch({ clipboardMonitor: false });
    await tick();
    expect(sw.getAttribute('aria-checked')).toBe('false');
  });
});

describe('顶栏场景分段与定向语言', () => {
  it('互译/定向分段控件选中态随镜像；点击发 set_settings 场景补丁', async () => {
    renderTopBar();
    applySettingsView(fakeSettings());
    await tick();
    expect(screen.getByText('互译').getAttribute('aria-pressed')).toBe('true');

    fireEvent.click(screen.getByText('定向'));
    expect(mocks.setSettings).toHaveBeenCalledWith({ scene: { active: 'directed' } });

    // 广播回流（活动场景全局唯一，settings://changed 同步）
    mergeSettingsPatch({ scene: { active: 'directed' } });
    await tick();
    expect(screen.getByText('定向').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByText('互译').getAttribute('aria-pressed')).toBe('false');
  });

  it('定向时出现目标语言下拉（Q5 九种）；互译时不出现（Tab 不可达）', async () => {
    renderTopBar();
    applySettingsView(fakeSettings({ scene: { active: 'directed' } }));
    await tick();
    const select = screen.getByLabelText('目标语言') as HTMLSelectElement;
    expect(select).toBeTruthy();
    const options = Array.from(select.options).map((o) => o.value);
    expect(options).toEqual(DIRECTED_TARGET_LANGS.map((l) => l.label));
    expect(options.length).toBe(9);

    mergeSettingsPatch({ scene: { active: 'mutual' } });
    await tick();
    // 互译时下拉随 140ms 过渡收起（shown 类移除；aria-hidden 由 Svelte 布尔收敛不恒现，以 shown 断言）
    expect(select.closest('.lang-wrap')!.classList.contains('shown')).toBe(false);
  });

  it('语言下拉变更：易失临时槽命令 + 回调（不回写默认值，由命令语义保证）', async () => {
    const { onTargetChange } = renderTopBar();
    applySettingsView(fakeSettings({ scene: { active: 'directed' } }));
    await tick();
    const select = screen.getByLabelText('目标语言') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: '日文' } });
    expect(mocks.setPanelTargetLang).toHaveBeenCalledWith({ lang: '日文' });
    expect(onTargetChange).toHaveBeenCalledWith('日文');
  });

  it('幽灵关闭按钮触发 onClose（FR-9.2 可见关闭控件）', () => {
    const { onClose } = renderTopBar();
    fireEvent.click(screen.getByRole('button', { name: '关闭面板' }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe('顶栏拖拽（真机修复轮 2：显式 startDragging）', () => {
  it('顶栏空白处按下左键 → 调用 startDragging', async () => {
    renderTopBar();
    const header = screen.getByText('互译').closest('header')!;
    fireEvent.mouseDown(header, { button: 0 });
    await tick();
    expect(mocks.startDragging).toHaveBeenCalledTimes(1);
  });

  it('按在交互控件（按钮/下拉）上不触发拖拽；非左键不触发', async () => {
    renderTopBar();
    fireEvent.mouseDown(screen.getByRole('button', { name: '关闭面板' }), { button: 0 });
    fireEvent.mouseDown(screen.getByRole('switch', { name: '剪贴板监听' }), { button: 2 });
    await tick();
    expect(mocks.startDragging).not.toHaveBeenCalled();
  });
});
