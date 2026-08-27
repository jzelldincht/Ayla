// 译文区组件测试：流式追加不整绘（D9 节点同一性）、EX-5 部分结果保留+错误卡片、
// 整段模式徽标（预估/正在翻译）、整段完成淡入。
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import StreamOutput from './StreamOutput.svelte';

afterEach(cleanup);

type Status = 'idle' | 'waiting' | 'streaming' | 'block' | 'done' | 'error';

interface Props {
  status: Status;
  text: string;
  renderMode: 'stream' | 'block';
  etaSeconds: number | null;
  errorExCode: string | null;
  roundKey: number;
  onRetry?: () => void;
  onOpenSettings?: () => void;
}

function defaults(overrides: Partial<Props>): Props {
  return {
    status: 'streaming',
    text: '',
    renderMode: 'stream',
    etaSeconds: null,
    errorExCode: null,
    roundKey: 1,
    ...overrides,
  };
}

describe('流式增量追加（D9 不整绘）', () => {
  it('两次增量产生两个片段节点，首片节点引用跨追加不变（旧内容不被重绘）', () => {
    const { container, rerender } = render(StreamOutput, {
      props: defaults({ status: 'streaming', text: '你好', roundKey: 1 }),
    });
    const host = container.querySelector('.chunk-host')!;
    const firstSpan = host.querySelector('span.stream-chunk')!;
    expect(firstSpan.textContent).toBe('你好');

    void rerender({ text: '你好世界' });
    const spans = host.querySelectorAll('span.stream-chunk');
    expect(spans.length).toBe(2);
    expect(host.querySelector('span.stream-chunk')).toBe(firstSpan);
    expect(host.textContent).toBe('你好世界');
  });

  it('新轮（roundKey 变化）清空上下文，重新从头构建（EX-13 替换/重译）', () => {
    const { container, rerender } = render(StreamOutput, {
      props: defaults({ status: 'streaming', text: '老轮', roundKey: 1 }),
    });
    expect(container.querySelectorAll('.stream-chunk').length).toBe(1);
    void rerender({ roundKey: 2, status: 'waiting', text: '' });
    expect(container.querySelectorAll('.stream-chunk').length).toBe(0);
  });
});

describe('等待与整段模式', () => {
  it('流式首字前：waiting 状态呼吸点可见', () => {
    const { container } = render(StreamOutput, {
      props: defaults({ status: 'waiting', roundKey: 1 }),
    });
    expect(container.querySelector('.dots')).toBeTruthy();
  });

  it('整段等待徽标：速率足够 →「预估剩余约 N 秒」且"预估"独立成词', () => {
    const { container } = render(StreamOutput, {
      props: defaults({ status: 'block', renderMode: 'block', etaSeconds: 12, roundKey: 1 }),
    });
    const badge = container.querySelector('.eta-badge')!;
    expect(badge.textContent).toContain('预估');
    expect(badge.textContent).toContain('12');
    expect(badge.textContent).toContain('秒');
  });

  it('整段等待徽标：速率不足 → 仅「正在翻译…」，不出现时间数字', () => {
    const { container } = render(StreamOutput, {
      props: defaults({ status: 'block', renderMode: 'block', etaSeconds: null, roundKey: 1 }),
    });
    const badge = container.querySelector('.eta-badge')!;
    expect(badge.textContent).toContain('正在翻译…');
    expect(badge.textContent).not.toContain('预估');
  });

  it('整段完成：done（block）一次整段淡入渲染全文', () => {
    const { container } = render(StreamOutput, {
      props: defaults({
        status: 'done',
        renderMode: 'block',
        text: '整段译文',
        roundKey: 1,
      }),
    });
    const whole = container.querySelector('.whole')!;
    expect(whole.textContent).toBe('整段译文');
  });
});

describe('错误态（EX-5 部分结果保留，4.5）', () => {
  it('partial 正常样式保留 + 传输中断卡片紧随其后；重试回调触发', () => {
    const onRetry = vi.fn();
    const { container } = render(StreamOutput, {
      props: defaults({
        status: 'error',
        text: '已收到的部分译文',
        errorExCode: 'EX-5',
        roundKey: 1,
        onRetry,
      }),
    });
    expect(container.textContent).toContain('已收到的部分译文');
    expect(container.textContent).toContain('传输中断');
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('错误卡片存在时不再出现呼吸点/光标条（4.4：不得残留等待动效）', () => {
    const { container } = render(StreamOutput, {
      props: defaults({ status: 'error', text: '部分', errorExCode: 'EX-1', roundKey: 1 }),
    });
    expect(container.querySelector('.dots')).toBeNull();
    expect(container.querySelector('.caret')).toBeNull();
    expect(screen.getByRole('alert')).toBeTruthy();
  });
});
