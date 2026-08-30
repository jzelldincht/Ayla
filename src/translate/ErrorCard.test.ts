// 六类错误卡片组件渲染与回调（design-spec 4.4：按钮动作 = 重试 或 打开设置）。
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import ErrorCard from './ErrorCard.svelte';
import { errorCardFor } from './lib/errors';

afterEach(cleanup);

describe('ErrorCard 渲染与回调', () => {
  it.each(['EX-1', 'EX-2', 'EX-3', 'EX-4', 'EX-5', 'EX-14'])(
    '%s：标题/说明/按钮文案逐字呈现',
    (ex) => {
      const card = errorCardFor(ex);
      const onRetry = vi.fn();
      const onOpenSettings = vi.fn();
      render(ErrorCard, { props: { exCode: ex, onRetry, onOpenSettings } });
      const alertEl = screen.getByRole('alert');
      expect(alertEl.textContent).toContain(card.title);
      expect(alertEl.textContent).toContain(card.description);
      const btn = screen.getByRole('button', { name: card.actionLabel });
      expect(btn).toBeTruthy();
    },
  );

  it('EX-1 重试：点击触发 onRetry（不触发 openSettings）', () => {
    const onRetry = vi.fn();
    const onOpenSettings = vi.fn();
    render(ErrorCard, { props: { exCode: 'EX-1', onRetry, onOpenSettings } });
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onOpenSettings).not.toHaveBeenCalled();
  });

  it('EX-2/EX-3 打开设置：点击触发 onOpenSettings（不触发重试）', () => {
    const onRetry = vi.fn();
    const onOpenSettings = vi.fn();
    render(ErrorCard, { props: { exCode: 'EX-2', onRetry, onOpenSettings } });
    fireEvent.click(screen.getByRole('button', { name: '打开设置' }));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
  });
});
