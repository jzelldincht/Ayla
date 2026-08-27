// AppendSink 单测（D9/FR-10.4：流式增量"纯文本节点追加，不整段重绘"）。
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { AppendSink } from './appendSink';

describe('AppendSink 流式增量追加', () => {
  let host: HTMLElement;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.appendChild(host);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    host.remove();
  });

  it('每次追加生成新片段节点，t 全文单调累加（增量 tail 切片）', () => {
    const sink = new AppendSink(host);
    const first = sink.append('敏捷的棕色狐狸');
    expect(first).toBe('敏捷的棕色狐狸');
    const second = sink.append('敏捷的棕色狐狸跳过');
    expect(second).toBe('跳过');
    expect(host.textContent).toBe('敏捷的棕色狐狸跳过');
    expect(sink.chunkCount).toBe(2);
  });

  it('既有片段节点不被重建（不整绘：同一 DOM 引用跨追加保持不变）', () => {
    const sink = new AppendSink(host);
    sink.append('你');
    const firstSpan = host.querySelector('span.stream-chunk')!;
    sink.append('你好世界');
    const spans = host.querySelectorAll('span.stream-chunk');
    expect(spans.length).toBe(2);
    // D9 核心：旧片段节点引用不被替换/重绘
    expect(host.querySelector('span.stream-chunk')).toBe(firstSpan);
    // 追加路径只新增节点，宿主自身不重建
    expect(spans[0].textContent).toBe('你');
    expect(spans[1].textContent).toBe('好世界');
  });

  it('新片段带 90ms 淡入过渡（≤90ms/次，design-spec 2.5）', () => {
    const sink = new AppendSink(host);
    sink.append('你好');
    const span = host.querySelector('span.stream-chunk')! as HTMLElement;
    expect(span.style.transition).toContain('90ms');
    expect(span.style.opacity).toBe('0');
  });

  it('reset 清空并复位（新一轮/EX-13 替换；后续追加从头构建）', () => {
    const sink = new AppendSink(host);
    sink.append('第一轮');
    sink.reset();
    expect(host.childElementCount).toBe(0);
    expect(sink.chunkCount).toBe(0);
    sink.append('第二轮');
    expect(host.textContent).toBe('第二轮');
  });

  it('无新增内容时 append 不产生空节点', () => {
    const sink = new AppendSink(host);
    sink.append('你好');
    const tail = sink.append('你好');
    expect(tail).toBe('');
    expect(sink.chunkCount).toBe(1);
  });
});
