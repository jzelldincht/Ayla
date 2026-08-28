// 设置页测试（T13）：六分组渲染与默认值（FR-13 全表 ✓ 出厂默认自 get_settings 镜像渲染）、
// 即时保存 patch（7.4：行尾「已保存」1.5s 淡出 / 失败行内 error）、快捷键录制状态机
// （成功/冲突回退/取消三分支 + 启动冲突 warning 图标，design-spec 7.2）、API Key 掩码修改流
// （7.3/INV-1：固定 8 位掩码、录入清空、可见性切换、空值=未配置）、隐私提示块（J3）、主题切换。
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import App from './App.svelte';
import { mergeSettingsPatch, resetSettingsMirror } from '../lib/stores';
import {
  DIRECTED_TARGET_LANGS,
  type HotkeyFunction,
  type OcrLanguage,
  type SettingsView,
} from '../lib/types';

const mocks = vi.hoisted(() => ({
  setSettings: vi.fn(() => Promise.resolve()),
  setClipboardMonitor: vi.fn(() => Promise.resolve()),
  rebindHotkey: vi.fn(() => Promise.resolve()),
  getSettings: vi.fn(() => Promise.resolve(null as SettingsView | null)),
  listOcrLanguages: vi.fn(() => Promise.resolve([] as OcrLanguage[])),
  hotkeyRegistrationFailures: vi.fn(() => Promise.resolve([] as HotkeyFunction[])),
}));

vi.mock('../lib/ipc', () => ({
  setSettings: mocks.setSettings,
  setClipboardMonitor: mocks.setClipboardMonitor,
  rebindHotkey: mocks.rebindHotkey,
  getSettings: mocks.getSettings,
  listOcrLanguages: mocks.listOcrLanguages,
  hotkeyRegistrationFailures: mocks.hotkeyRegistrationFailures,
  onSettingsChanged: vi.fn(() => Promise.resolve(() => {})),
}));

afterEach(() => {
  cleanup();
  resetSettingsMirror();
  vi.useRealTimers();
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSettings.mockResolvedValue(null);
  mocks.listOcrLanguages.mockResolvedValue([]);
  mocks.hotkeyRegistrationFailures.mockResolvedValue([]);
});

function fakeSettings(overrides: Partial<SettingsView> = {}): SettingsView {
  return {
    schemaVersion: 1,
    ai: { baseUrl: 'https://api.deepseek.com', model: 'deepseek-chat', hasKey: false },
    translation: {
      defaultTargetLang: '英文',
      presentation: 'stream_all',
      longTextThresholdChars: 1000,
      autoTranslate: { enabled: false, delayMs: 700 },
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

/**
 * 渲染并让镜像经 get_settings（后端权威）落位——与 onMount 的 initSettingsMirror 同源，
 * 避免手动 applySettingsView 与异步镜像拉取竞态（镜像以 mock 返回值为准）。
 */
async function renderApp(view: SettingsView = fakeSettings()): Promise<void> {
  mocks.getSettings.mockResolvedValue(view);
  render(App);
  await tick();
}

/** 点击左侧分组导航，并切到指定组。 */
async function gotoGroup(name: string): Promise<void> {
  fireEvent.click(screen.getByRole('button', { name }));
  await tick();
}

describe('六分组渲染与出厂默认值（FR-13 全表来自 get_settings 镜像）', () => {
  it('导航含六分组；AI 服务默认组渲染三项与隐私提示块', async () => {
    await renderApp();
    for (const label of ['AI 服务', '翻译', '面板', '快捷键', '截图', '通用']) {
      expect(screen.getByRole('button', { name: label })).toBeTruthy();
    }
    const baseUrl = screen.getByLabelText('接口地址') as HTMLInputElement;
    expect(baseUrl.value).toBe('https://api.deepseek.com');
    expect((screen.getByLabelText('模型名') as HTMLInputElement).value).toBe('deepseek-chat');
    // API Key：未配置文案 + 配置（非修改）
    expect(screen.getByText('未配置')).toBeTruthy();
    expect(screen.getByRole('button', { name: '配置' })).toBeTruthy();
    // J3 常驻隐私提示块（完整句子）
    expect(
      screen.getByText(
        '使用翻译功能时，原文将被发送至所配置的第三方云端服务；截图识别在本机完成。',
      ),
    ).toBeTruthy();
  });

  it('翻译组：五项（Q5 九语种 / 呈现两档 / 阈值 / 自动翻译开关+延迟）按镜像渲染', async () => {
    await renderApp();
    await gotoGroup('翻译');
    const lang = screen.getByLabelText('定向目标语言') as HTMLSelectElement;
    expect(lang.value).toBe('英文');
    expect(Array.from(lang.options).map((o) => o.value)).toEqual(
      DIRECTED_TARGET_LANGS.map((l) => l.label),
    );
    expect((screen.getByLabelText('译文呈现') as HTMLSelectElement).value).toBe('stream_all');
    expect((screen.getByLabelText('长文整段阈值') as HTMLInputElement).value).toBe('1000');
    // 轮 12：自动翻译默认关（回车翻译），延迟档默认 700ms 且未开启时禁用
    expect(screen.getByRole('switch', { name: '自动翻译' }).getAttribute('aria-checked')).toBe(
      'false',
    );
    const delay = screen.getByLabelText('自动翻译延迟') as HTMLSelectElement;
    expect(delay.value).toBe('700');
    expect(delay.disabled).toBe(true);
  });

  it('面板组：失焦即隐开 / 剪贴板监听关；截图组：OCR 自动；通用组：自启关、主题跟随系统', async () => {
    await renderApp();
    await gotoGroup('面板');
    expect(screen.getByRole('switch', { name: '失焦即隐' }).getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(screen.getByRole('switch', { name: '剪贴板监听' }).getAttribute('aria-checked')).toBe(
      'false',
    );

    await gotoGroup('截图');
    const ocr = screen.getByLabelText('OCR 语种') as HTMLSelectElement;
    expect(ocr.value).toBe('auto');
    expect(ocr.options[0].text).toContain('自动');

    await gotoGroup('通用');
    expect(screen.getByRole('switch', { name: '开机自启' }).getAttribute('aria-checked')).toBe(
      'false',
    );
    expect(screen.getByRole('button', { name: '跟随系统' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
  });

  it('快捷键组：五项键帽按镜像渲染（FR-14.1 出厂值）', async () => {
    await renderApp();
    await gotoGroup('快捷键');
    for (const combo of ['Alt+Q', 'Alt+E', 'Alt+S', 'Alt+D', 'Alt+W']) {
      expect(screen.getByText(combo)).toBeTruthy();
    }
    expect(screen.getAllByRole('button', { name: '修改' })).toHaveLength(5);
  });
});

describe('即时保存（design-spec 7.4）', () => {
  it('控件变更 → set_settings 补丁；行尾「已保存」1.5s 后淡出移除', async () => {
    vi.useFakeTimers();
    await renderApp();
    const input = screen.getByLabelText('接口地址');
    fireEvent.change(input, { target: { value: 'https://new.example.com/' } });
    expect(mocks.setSettings).toHaveBeenCalledWith({
      ai: { baseUrl: 'https://new.example.com/' },
    });
    const saved = await screen.findByText('已保存');
    expect(saved.classList.contains('fading')).toBe(false);

    vi.advanceTimersByTime(1500);
    await tick();
    expect(saved.classList.contains('fading')).toBe(true);

    vi.advanceTimersByTime(300);
    await tick();
    expect(screen.queryByText('已保存')).toBeNull();
  });

  it('保存失败：行内 error 常驻（不出现「已保存」）', async () => {
    mocks.setSettings.mockRejectedValueOnce(new Error('配置写入失败'));
    await renderApp();
    fireEvent.change(screen.getByLabelText('模型名'), { target: { value: 'x' } });
    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(screen.queryByText('已保存')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('配置写入失败');
  });

  it('剪贴板监听开关走 set_clipboard_monitor 专用通道（非 set_settings）', async () => {
    await renderApp();
    await gotoGroup('面板');
    fireEvent.click(screen.getByRole('switch', { name: '剪贴板监听' }));
    expect(mocks.setClipboardMonitor).toHaveBeenCalledWith(true);
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });

  it('主题变更即时预览：patch + 广播回流 → data-theme 切换', async () => {
    await renderApp();
    await gotoGroup('通用');
    fireEvent.click(screen.getByRole('button', { name: '暗色' }));
    expect(mocks.setSettings).toHaveBeenCalledWith({ general: { theme: 'dark' } });
    // INV-8：后端广播回流后镜像 + 主题生效
    mergeSettingsPatch({ general: { theme: 'dark' } });
    await tick();
    expect(document.documentElement.dataset.theme).toBe('dark');
  });
});

describe('快捷键录制控件状态机（design-spec 7.2 三分支）', () => {
  async function startRecording(): Promise<void> {
    await renderApp();
    await gotoGroup('快捷键');
    fireEvent.click(screen.getAllByRole('button', { name: '修改' })[0]);
    await tick();
    expect(screen.getByText('请按下新的快捷键…（Esc 或点击其他位置取消）')).toBeTruthy();
  }

  it('取消分支：Esc 不改值不调用 rebind', async () => {
    await startRecording();
    fireEvent.keyDown(window, { key: 'Escape' });
    await tick();
    expect(screen.getByText('Alt+Q')).toBeTruthy();
    expect(mocks.rebindHotkey).not.toHaveBeenCalled();
  });

  it('取消分支：点击控件外取消不改值', async () => {
    await startRecording();
    fireEvent.mouseDown(document.body);
    await tick();
    expect(screen.getAllByRole('button', { name: '修改' })).toHaveLength(5);
    expect(mocks.rebindHotkey).not.toHaveBeenCalled();
  });

  it('成功分支：录得组合 → rebind_hotkey；权威刷新后键帽更新 + 「已保存」', async () => {
    await startRecording();
    // 权威刷新（get_settings）返回新组合
    mocks.getSettings.mockResolvedValueOnce(
      fakeSettings({ hotkeys: { ...fakeSettings().hotkeys, selection: 'Ctrl+Shift+K' } }),
    );
    fireEvent.keyDown(window, { key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true });
    expect(mocks.rebindHotkey).toHaveBeenCalledWith({
      function: 'selection',
      combo: 'Ctrl+Shift+K',
    });
    await screen.findByText('Ctrl+Shift+K');
    expect(screen.getByText('已保存')).toBeTruthy();
  });
  it('冲突回退分支：rebind 失败 → 行内 error 固定文案 + 键帽保持旧值', async () => {
    mocks.rebindHotkey.mockRejectedValueOnce(
      new Error(
        '快捷键「Ctrl+Shift+X」绑定失败：划词翻译（注册失败（可能已被系统或其他程序占用），已回退为原快捷键），更多信息见...',
      ),
    );
    await startRecording();
    fireEvent.keyDown(window, { key: 'X', code: 'KeyX', ctrlKey: true, shiftKey: true });
    expect(mocks.rebindHotkey).toHaveBeenCalledWith({
      function: 'selection',
      combo: 'Ctrl+Shift+X',
    });
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('该快捷键已被其他程序占用，已恢复原快捷键');
    await tick(); // 等 recorder 回展示态（onRebind 失败链 flush）
    expect(await screen.findByText('Alt+Q')).toBeTruthy(); // 回退：键帽未变
    expect(screen.queryByText('已保存')).toBeNull();
  });

  it('启动冲突常驻 warning 图标：降级清单命中 → 行尾图标（title=注册失败，点击修改）', async () => {
    mocks.hotkeyRegistrationFailures.mockResolvedValue(['selection']);
    await renderApp();
    await gotoGroup('快捷键');
    const icon = screen.getByRole('button', { name: '注册失败，点击修改' });
    expect(icon.getAttribute('title')).toContain('注册失败');
    fireEvent.click(icon);
    await tick();
    expect(screen.getByText(/请按下新的快捷键/)).toBeTruthy();
  });
});

describe('API Key 掩码修改流（7.3 / INV-1）', () => {
  const withKeyView = (hasKey: boolean): SettingsView =>
    fakeSettings({
      ai: { baseUrl: 'https://api.deepseek.com', model: 'deepseek-chat', hasKey },
    });

  it('已存 Key 固定 8 位掩码；修改流清空录入 + 可见性切换 + 保存后回掩码', async () => {
    await renderApp(withKeyView(true));
    expect(screen.getByText('••••••••')).toBeTruthy();
    expect(screen.getByRole('button', { name: '修改' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '修改' }));
    await tick();
    const input = screen.getByLabelText('API Key') as HTMLInputElement;
    expect(input.value).toBe('');
    expect(input.type).toBe('password');

    fireEvent.input(input, { target: { value: 'sk-fake-test' } });
    fireEvent.click(screen.getByRole('button', { name: '显示' }));
    await tick();
    expect((screen.getByLabelText('API Key') as HTMLInputElement).type).toBe('text');
    fireEvent.click(screen.getByRole('button', { name: '隐藏' }));
    await tick();
    expect((screen.getByLabelText('API Key') as HTMLInputElement).type).toBe('password');

    // 保存：写入通道带 apiKey（进 Rust 即加密）；成功后权威刷新回掩码
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(mocks.setSettings).toHaveBeenCalledWith({ ai: { apiKey: 'sk-fake-test' } });
    await screen.findByText('••••••••');
    expect(screen.getByText('已保存')).toBeTruthy();
  });

  it('允许保存空值（未配置）：清空录入保存 → 掩码空态「未配置」', async () => {
    await renderApp(withKeyView(true));
    fireEvent.click(screen.getByRole('button', { name: '修改' }));
    await tick();
    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: '' } });
    // 权威刷新回未配置态（hasKey=false）
    mocks.getSettings.mockResolvedValueOnce(withKeyView(false));
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(mocks.setSettings).toHaveBeenCalledWith({ ai: { apiKey: '' } });
    await screen.findByText('未配置');
  });

  it('取消放弃修改：不发 set_settings，仍显示掩码', async () => {
    await renderApp(withKeyView(true));
    fireEvent.click(screen.getByRole('button', { name: '修改' }));
    await tick();
    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: 'sk-x' } });
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    await tick();
    expect(screen.getByText('••••••••')).toBeTruthy();
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });
});

describe('OCR 语种下拉（5.7.6 已装清单数据源）', () => {
  it('自动首项 + list_ocr_languages 已装清单；选中值保存 patch', async () => {
    mocks.listOcrLanguages.mockResolvedValue([
      { tag: 'en-US', label: 'English (United States)' },
      { tag: 'ja', label: 'Japanese' },
    ]);
    await renderApp();
    await gotoGroup('截图');
    const select = screen.getByLabelText('OCR 语种') as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['auto', 'en-US', 'ja']);
    fireEvent.change(select, { target: { value: 'ja' } });
    expect(mocks.setSettings).toHaveBeenCalledWith({ ocr: { language: 'ja' } });
  });
});
