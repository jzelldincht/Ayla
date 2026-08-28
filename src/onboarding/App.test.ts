// 首启引导页测试（T14）：步骤流转（欢迎→隐私→Key→完成）、隐私步唯一前进入口与跳过路径
// （FR-11.2/11.3）、验证连接成功/失败行内反馈与重试（FR-11.1(c)）、guide 模式直达
// （FR-11.4：隐私要点 + Key 两步，跳过即结束）、向导不支持 Esc（design-spec 6）。
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import App from './App.svelte';
import { resetSettingsMirror } from '../lib/stores';

const mocks = vi.hoisted(() => ({
  setSettings: vi.fn(() => Promise.resolve()),
  verifyConnection: vi.fn(() => Promise.resolve()),
  finishOnboarding: vi.fn(() => Promise.resolve()),
}));

vi.mock('../lib/ipc', () => ({
  setSettings: mocks.setSettings,
  verifyConnection: mocks.verifyConnection,
  finishOnboarding: mocks.finishOnboarding,
  getSettings: vi.fn(() => Promise.resolve(null)),
  onSettingsChanged: vi.fn(() => Promise.resolve(() => {})),
  onOnboardingBegin: vi.fn(() => Promise.resolve(() => {})),
}));

afterEach(() => {
  cleanup();
  resetSettingsMirror();
});

beforeEach(() => {
  vi.clearAllMocks();
});

/** 当前步名（顶部指示器 role=status；唯一 status 元素）。 */
function stepName(): string {
  return screen.getByRole('status').textContent ?? '';
}

describe('first 模式四步流转', () => {
  it('欢迎 → 隐私告知 → 配置 API Key → 完成，步名随流程更新', async () => {
    render(App);
    expect(screen.getByText('欢迎使用 Alya')).toBeTruthy();
    expect(stepName()).toContain('欢迎');

    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    expect(screen.getByRole('heading', { name: '隐私告知' })).toBeTruthy();
    expect(stepName()).toContain('隐私告知');

    fireEvent.click(screen.getByRole('button', { name: '我已了解，继续' }));
    await tick();
    expect(screen.getByRole('heading', { name: '配置 API Key（可跳过）' })).toBeTruthy();
    expect(stepName()).toContain('配置 API Key');

    fireEvent.click(screen.getByRole('button', { name: '继续' }));
    await tick();
    expect(screen.getByText('一切就绪')).toBeTruthy();
    expect(stepName()).toContain('完成');
  });

  it('完成步「进入 Alya」→ finish_onboarding', async () => {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '我已了解，继续' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '继续' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '进入 Alya' }));
    expect(mocks.finishOnboarding).toHaveBeenCalledTimes(1);
  });
});

describe('隐私告知步（FR-11.2 必经）', () => {
  it('完整句子 + 强调块平铺（无折叠标记）；「我已了解，继续」为唯一前进入口', async () => {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    expect(screen.getByText(/你选取的原文将被发送至你所配置的第三方云端翻译服务/)).toBeTruthy();
    expect(screen.getByText(/截图识别的文字提取在本机完成，截图本身不会上传/)).toBeTruthy();
    expect(screen.getByText('以上内容完整展示、不可折叠；阅读后才能继续。')).toBeTruthy();
    // 前进入口唯一：主按钮「我已了解，继续」；辅助链接为跳过
    expect(screen.getByRole('button', { name: '我已了解，继续' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '暂不使用' })).toBeTruthy();
  });

  it('跳过路径：暂不使用 → 完成步 → 进入 Alya（写 meta，FR-11.3）', async () => {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '暂不使用' }));
    await tick();
    expect(stepName()).toContain('完成');
    fireEvent.click(screen.getByRole('button', { name: '进入 Alya' }));
    expect(mocks.finishOnboarding).toHaveBeenCalledTimes(1);
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });

  it('Key 步跳过：「暂时跳过，稍后在设置中配置」→ 完成步，不发 set_settings', async () => {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '我已了解，继续' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '暂时跳过，稍后在设置中配置' }));
    await tick();
    expect(stepName()).toContain('完成');
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });
});

describe('验证连接行内反馈（FR-11.1(c)）', () => {
  async function gotoKeyStep(): Promise<void> {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    fireEvent.click(screen.getByRole('button', { name: '我已了解，继续' }));
    await tick();
  }

  it('成功：success 色行「验证成功」', async () => {
    await gotoKeyStep();
    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: 'sk-fake-test' } });
    fireEvent.click(screen.getByRole('button', { name: '验证连接' }));
    expect(mocks.verifyConnection).toHaveBeenCalledWith({ key: 'sk-fake-test' });
    expect(await screen.findByText('✓ 验证成功')).toBeTruthy();
  });

  it('失败：error 色行内原因，可重试（再次点击成功覆盖）', async () => {
    mocks.verifyConnection.mockRejectedValueOnce(new Error('鉴权失败: 401'));
    await gotoKeyStep();
    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: 'sk-fake-test' } });
    fireEvent.click(screen.getByRole('button', { name: '验证连接' }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('✕ 验证失败');
    expect(alert.textContent).toContain('鉴权失败: 401');

    // 重试：mock 恢复成功 → 验证成功覆盖失败原因
    mocks.verifyConnection.mockResolvedValueOnce();
    fireEvent.click(screen.getByRole('button', { name: '验证连接' }));
    expect(await screen.findByText('✓ 验证成功')).toBeTruthy();
  });

  it('验证失败不阻断：未验证也可「继续」（信任用户输入），保存录入的 Key 后进入完成步', async () => {
    mocks.verifyConnection.mockRejectedValueOnce(new Error('网络不可达'));
    await gotoKeyStep();
    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: 'sk-fake-test' } });
    fireEvent.click(screen.getByRole('button', { name: '继续' }));
    expect(mocks.setSettings).toHaveBeenCalledWith({ ai: { apiKey: 'sk-fake-test' } });
    await tick();
    expect(stepName()).toContain('完成');
  });
});

describe('guide 模式（FR-11.4 补配引导直达）', () => {
  it('直达隐私要点 + Key 两步（无欢迎/完成步）；补配后 finish_onboarding', async () => {
    render(App, { props: { mode: 'guide' } });
    expect(stepName()).toContain('隐私告知');
    expect(screen.queryByText('欢迎使用 Alya')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '我已了解，继续' }));
    await tick();
    expect(stepName()).toContain('配置 API Key');

    fireEvent.input(screen.getByLabelText('API Key'), { target: { value: 'sk-fake-guide' } });
    fireEvent.click(screen.getByRole('button', { name: '继续' }));
    expect(mocks.setSettings).toHaveBeenCalledWith({ ai: { apiKey: 'sk-fake-guide' } });
    await tick(); // 保存 Key 完成后才 finish（guide 直接结束）
    expect(mocks.finishOnboarding).toHaveBeenCalledTimes(1);
  });

  it('guide 跳过：「暂不使用」直接结束（不发 set_settings，回调 hasKey=false 由后端判定）', async () => {
    render(App, { props: { mode: 'guide' } });
    fireEvent.click(screen.getByRole('button', { name: '暂不使用' }));
    expect(mocks.finishOnboarding).toHaveBeenCalledTimes(1);
    expect(mocks.setSettings).not.toHaveBeenCalled();
  });
});

describe('向导不支持 Esc（design-spec 6）', () => {
  it('Esc 被拦截：步骤不变、不触发完成/保存', async () => {
    render(App);
    fireEvent.click(screen.getByRole('button', { name: '开始使用' }));
    await tick();
    fireEvent.keyDown(window, { key: 'Escape' });
    await tick();
    expect(stepName()).toContain('隐私告知');
    expect(screen.getByRole('button', { name: '我已了解，继续' })).toBeTruthy();
    expect(mocks.finishOnboarding).not.toHaveBeenCalled();
  });
});
