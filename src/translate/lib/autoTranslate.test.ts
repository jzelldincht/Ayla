import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AUTO_TRANSLATE_DELAY_MS, AutoTranslateScheduler } from './autoTranslate';

describe('AutoTranslateScheduler（原文变化自动翻译）', () => {
  let scheduler: AutoTranslateScheduler;

  beforeEach(() => {
    vi.useFakeTimers();
    // jsdom 的 document.hasFocus() 默认 false（窗口未聚焦），桩为聚焦态；
    // 失焦分支由「触发时已失焦」用例单独覆盖。
    vi.spyOn(document, 'hasFocus').mockReturnValue(true);
    scheduler = new AutoTranslateScheduler();
  });

  afterEach(() => {
    scheduler.cancel();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('内容变化且非空白：停顿满阈值后触发一次（差 1ms 不触发）', () => {
    const fire = vi.fn();
    scheduler.input('新内容', '旧内容', AUTO_TRANSLATE_DELAY_MS, fire);
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS - 1);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it('与已提交内容相同或纯空白：不安排任务', () => {
    const fire = vi.fn();
    scheduler.input('相同', '相同', AUTO_TRANSLATE_DELAY_MS, fire);
    expect(scheduler.isPending).toBe(false);
    scheduler.input('  \n ', '旧内容', AUTO_TRANSLATE_DELAY_MS, fire);
    expect(scheduler.isPending).toBe(false);
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS * 2);
    expect(fire).not.toHaveBeenCalled();
  });

  it('仅首尾空白变化（轮 11）：不触发自动翻译', () => {
    const fire = vi.fn();
    scheduler.input(' hello ', 'hello', AUTO_TRANSLATE_DELAY_MS, fire);
    scheduler.input('hello\n', 'hello', AUTO_TRANSLATE_DELAY_MS, fire);
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS * 2);
    expect(fire).not.toHaveBeenCalled();
  });

  it('延迟可配置（轮 12）：按传入 delayMs 计时', () => {
    const fire = vi.fn();
    scheduler.input('新内容', '旧内容', 1500, fire);
    vi.advanceTimersByTime(1499);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it('连续输入防抖：计时随输入重置，只在最后一次停顿后触发一次', () => {
    const fire = vi.fn();
    scheduler.input('a', '', AUTO_TRANSLATE_DELAY_MS, fire);
    vi.advanceTimersByTime(500);
    scheduler.input('ab', '', AUTO_TRANSLATE_DELAY_MS, fire);
    vi.advanceTimersByTime(500);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS - 500);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it('cancel 后不再触发（显式提交路径防双重请求）', () => {
    const fire = vi.fn();
    scheduler.input('x', '', AUTO_TRANSLATE_DELAY_MS, fire);
    scheduler.cancel();
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS * 2);
    expect(fire).not.toHaveBeenCalled();
    expect(scheduler.isPending).toBe(false);
  });

  it('触发时已失焦：放弃本次，不空耗请求', () => {
    const fire = vi.fn();
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    scheduler.input('x', '', AUTO_TRANSLATE_DELAY_MS, fire);
    vi.advanceTimersByTime(AUTO_TRANSLATE_DELAY_MS);
    expect(fire).not.toHaveBeenCalled();
  });
});
