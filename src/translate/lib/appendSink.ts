// 流式追加渲染器（D9/FR-10.4：增量追加、纯文本节点、禁止整段重绘）。
//
// 实现要点：译文区内容以「分片 span」逐个创建——每个 span 是独立**纯文本节点**，追加时
// `span.textContent = tail` 一次性写入；既有片段节点绝不被替换/重建（测试以节点同一性断言
// 不整绘）。新片 90ms 透明度淡入（design-spec 2.5 流式增量 ≤90ms/次）；整段模式完成由
// 上层 once 变量触发容器淡入（140ms）。
//
// 说明：不采用单一 Text 节点 `appendData` 的原因——增量需要独立淡入动画，单文本节点无法
// 对片段做动画；span 包裹仍保持**纯文本语义**（选中/复制/读屏得到连续文本，D9 要求的是
// "文本节点追加不整绘"，span 不引入任何布局结构变化）。

/** 一段已提交的译文片段（供测试断言片段节点同一性）。 */
export interface ChunkCell {
  el: HTMLSpanElement;
  text: string;
}

export interface AppendSinkOptions {
  /** 每个新片段的淡入时长（ms；design-spec ≤90）。默认 90。 */
  fadeMs?: number;
}

/**
 * 增量追加 sink：宿主元素内按增量创建片段 span，旧片段绝不重写。
 * - `sync(fullText)`：以完整累计文本同步（内部只追加新增尾巴；只允许文本单调增长，
 *   新一轮替换须先 `reset()`）。
 * - `reset()`：清空宿主并复位（新一轮等待态 / EX-13 替换）。
 */
export class AppendSink {
  private host: HTMLElement;
  private fadeMs: number;
  private bytes = 0;
  private chunks: ChunkCell[] = [];

  constructor(host: HTMLElement, options: AppendSinkOptions = {}) {
    this.host = host;
    this.fadeMs = options.fadeMs ?? 90;
  }

  /** 当前绑定的宿主元素（流式输出层据此校验宿主是否已被新一轮分支替换）。 */
  get hostElement(): HTMLElement {
    return this.host;
  }

  /** 追加到最新累计文本；返回本轮追加的尾巴（供测试/统计）。 */
  append(fullText: string): string {
    const tail = fullText.slice(this.bytes);
    if (tail.length === 0) return '';
    const el = document.createElement('span');
    el.className = 'stream-chunk';
    el.textContent = tail;
    el.style.opacity = '0';
    // 淡入 ≤90ms；系统减弱动效时 theme.css 全局降到 80ms（≤80ms 透明度过渡）。
    el.style.transition = `opacity ${this.fadeMs}ms ease-in`;
    this.host.appendChild(el);
    // 强制一次样式计算后转 1：保证透明度过渡对新片段生效（下一帧淡入）。
    // jsdom 等无 requestAnimationFrame 环境（vitest）降级为 0ms 定时器。
    const raf = typeof requestAnimationFrame === 'function' ? requestAnimationFrame.bind(window) : ((cb: () => void) => setTimeout(cb, 0));
    raf(() => {
      el.style.opacity = '1';
    });
    this.bytes += tail.length;
    this.chunks.push({ el, text: tail });
    return tail;
  }

  get chunkCount(): number {
    return this.chunks.length;
  }

  get lastChunk(): ChunkCell | null {
    return this.chunks.at(-1) ?? null;
  }

  /** 新一轮：清空全文（宿主子节点整体移除——仅在新轮开始时发生，非流式追加路径）。 */
  reset(): void {
    this.host.replaceChildren();
    this.bytes = 0;
    this.chunks = [];
  }
}
