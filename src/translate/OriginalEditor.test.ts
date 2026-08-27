// 原文编辑区（四形态共用）：重译按钮浮现逻辑（A1 裁决）+ Enter 同效 + Shift+Enter 换行 + 空提交拦截。
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import OriginalEditor from './OriginalEditor.svelte';

afterEach(cleanup);

interface Props {
  value: string;
  submitted: string;
  oninput: (v: string) => void;
  onaction: () => void;
  buttonMode: 'always' | 'whenModified';
  buttonLabel: string;
  placeholder?: string;
  variant?: 'bare' | 'boxed' | 'subtle';
  autofocus?: boolean;
  ariaLabel: string;
}

function defaults(overrides: Partial<Props>): Props {
  return {
    value: '原文',
    submitted: '原文',
    oninput: () => {},
    onaction: () => {},
    buttonMode: 'whenModified',
    buttonLabel: '重译 ↵',
    ariaLabel: '原文',
    ...overrides,
  };
}

describe('形式 A（whenModified）：重译按钮浮现', () => {
  it('未修改（value===submitted）→ 不浮现', () => {
    render(OriginalEditor, { props: defaults({}) });
    expect(screen.queryByText('重译 ↵')).toBeNull();
  });

  it('内容被修改 → 浮现「重译 ↵」；点击触发 onaction', () => {
    const onaction = vi.fn();
    render(OriginalEditor, { props: defaults({ value: '原文改', onaction }) });
    fireEvent.click(screen.getByText('重译 ↵'));
    expect(onaction).toHaveBeenCalledTimes(1);
  });

  it('修改后 Enter 同效（焦点在原文区触发重译）；未修改 Enter 不触发', () => {
    const onaction = vi.fn();
    const { rerender } = render(OriginalEditor, { props: defaults({ onaction }) });
    const ta = screen.getByLabelText('原文') as HTMLTextAreaElement;
    // 未修改：Enter 不触发（原生换行语义，不抢占编辑）
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onaction).not.toHaveBeenCalled();
    // 修改后：Enter 触发重译
    void rerender({ value: '原文改' });
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onaction).toHaveBeenCalledTimes(1);
  });

  it('Shift+Enter 不触发重译（换行语义保留）', () => {
    const onaction = vi.fn();
    render(OriginalEditor, { props: defaults({ value: '原文改', onaction }) });
    const ta = screen.getByLabelText('原文') as HTMLTextAreaElement;
    fireEvent.keyDown(ta, { key: 'Enter', shiftKey: true });
    expect(onaction).not.toHaveBeenCalled();
  });
});

describe('形式 B（always）：翻译按钮常驻 + Enter 提交', () => {
  it('按钮常驻；Enter 提交；Shift+Enter 不提交', () => {
    const onaction = vi.fn();
    render(OriginalEditor, {
      props: defaults({ buttonMode: 'always', buttonLabel: '翻译 ↵', onaction }),
    });
    expect(screen.getByText('翻译 ↵')).toBeTruthy();
    const ta = screen.getByLabelText('原文') as HTMLTextAreaElement;
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onaction).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(ta, { key: 'Enter', shiftKey: true });
    expect(onaction).toHaveBeenCalledTimes(1);
  });

  it('空白内容：按钮隐藏且 Enter 不发起（FR-4.3 无错误拦截）', () => {
    const onaction = vi.fn();
    render(OriginalEditor, {
      props: defaults({ value: '   ', buttonMode: 'always', buttonLabel: '翻译 ↵', onaction }),
    });
    expect(screen.queryByText('翻译 ↵')).toBeNull();
    const ta = screen.getByLabelText('原文') as HTMLTextAreaElement;
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onaction).not.toHaveBeenCalled();
  });

  it('输入回调带上新值（受控流）', () => {
    const oninput = vi.fn();
    render(OriginalEditor, { props: defaults({ oninput }) });
    const ta = screen.getByLabelText('原文') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: '你好' } });
    expect(oninput).toHaveBeenCalledWith('你好');
  });
});
