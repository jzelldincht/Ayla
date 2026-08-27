<script lang="ts">
  // 译文区（design-spec 4.3 状态组件；D9/FR-10.4 增量追加不整绘；4.4 错误卡片原位）。
  //
  // 状态机（由 App 层持有并驱动本组件渲染）：
  // - idle：引导语（形态 B 空态「译文会在这里出现」，fg/muted 居左上）。
  // - waiting：呼吸圆点（减弱动效→静态"翻译中…"）；2px 不确定进度条由 App 层渲染在顶栏下方。
  // - block：长文整段等待徽标「预估剩余约 N 秒」（"预估"醒目 warning 色，每秒刷新）；
  //   速率不足时仅「正在翻译…」。增量在本组件不渲染（缓冲于 App.outputText）。
  // - streaming：增量 span 淡入（≤90ms）+ 末尾闪烁光标条（2px，周期约 1s，完成后 200ms 淡出）。
  // - done：stream→静态全文（光标淡出）；block→整段一次 140ms 淡入。
  // - error：partial 正常样式保留（绝不删除/置灰），其后紧跟错误卡片（EX-5 中断；4.5）。
  //
  // 滚动锚定（4.3）：距底 <24px 跟随，上滚停锚，滚回底部恢复；跟随态追加时贴底。

  import { onMount } from 'svelte';
  import { AppendSink } from './lib/appendSink';
  import { ScrollAnchor } from './lib/scrollAnchor';
  import ErrorCard from './ErrorCard.svelte';

  let {
    status,
    text,
    renderMode,
    etaSeconds,
    errorExCode,
    roundKey,
    onRetry,
    onOpenSettings,
  }: {
    status: 'idle' | 'waiting' | 'streaming' | 'block' | 'done' | 'error';
    text: string;
    renderMode: 'stream' | 'block';
    /** block 模式每秒刷新的剩余秒数（null＝速率不足，仅「正在翻译…」）。 */
    etaSeconds: number | null;
    errorExCode: string | null;
    /** 新一轮标识：变化即清空/复位（EX-13 替换、送翻等）。 */
    roundKey: number;
    onRetry?: () => void;
    onOpenSettings?: () => void;
  } = $props();

  let outputEl: HTMLDivElement;
  let chunkHost = $state<HTMLDivElement | null>(null);
  let sink: AppendSink | null = null;
  let anchor = new ScrollAnchor();
  let lastRoundKey = -1;
  let wholeFade = $state(false);
  let reducedMotion = $state(false);

  onMount(() => {
    // jsdom（vitest）无 matchMedia：缺省 false
    reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
    outputEl.addEventListener('scroll', () =>
      anchor.update(outputEl.scrollTop, outputEl.scrollHeight, outputEl.clientHeight),
    );
  });

  function ensureSink(): AppendSink | null {
    if (!chunkHost) return null;
    // 宿主元素每轮随模板分支重建（waiting 分支无 chunk-host）；sink 必须与当前宿主绑定。
    if (sink && sink.hostElement !== chunkHost) sink = null;
    sink ??= new AppendSink(chunkHost);
    return sink;
  }

  $effect(() => {
    const s = status;
    const t = text;
    const rk = roundKey;
    if (rk !== lastRoundKey) {
      lastRoundKey = rk;
      // 宿主要么随模板分支重建（bind:this 已给新值），要么是同一元素（done 分支复用）——
      // ensureSink 内部以 hostElement 引用比对保证不绑到旧宿主，此处仅弃用旧 sink。
      sink = null;
      anchor.reset();
      wholeFade = false;
    }

    if (s === 'streaming' || s === 'done' || (s === 'error' && t.length > 0)) {
      const active = ensureSink();
      if (active) {
        active.append(t);
        if (s === 'streaming' || s === 'done') {
          anchor.update(outputEl.scrollTop, outputEl.scrollHeight, outputEl.clientHeight);
          anchor.applyAppend(outputEl);
        }
      }
    }

    // 整段模式完成：一次 140ms 淡入（不做增量渲染）。
    if (s === 'done' && renderMode === 'block') {
      wholeFade = true;
    }
  });
</script>

<div class="stream-output" bind:this={outputEl} role="note" aria-label="译文">
  {#if status === 'idle'}
    <div class="idle-hint">译文会在这里出现</div>
  {:else if status === 'waiting'}
    <div class="wait-block" aria-live="polite">
      {#if reducedMotion}
        <span class="wait-static">翻译中…</span>
      {:else}
        <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
      {/if}
    </div>
  {:else if status === 'block'}
    <div class="block-wait" aria-live="polite">
      <span class="eta-badge">
        {#if etaSeconds !== null}
          <span class="eta-strong">预估</span>剩余约 <span class="eta-num">{etaSeconds}</span> 秒
        {:else}
          <span class="eta-plain">正在翻译…</span>
        {/if}
      </span>
      <div class="block-main">
        <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="block-text">正在翻译长文本…</span>
      </div>
    </div>
  {:else if status === 'streaming'}
    <div class="chunk-host" bind:this={chunkHost}></div>
    <span class="caret" aria-hidden="true"></span>
  {:else if status === 'done' && renderMode === 'block'}
    <div class="whole" class:whole-visible={wholeFade}>{text}</div>
  {:else if status === 'done'}
    <div class="chunk-host" bind:this={chunkHost}></div>
  {:else if status === 'error'}
    {#if text.length > 0}
      <div class="chunk-host" bind:this={chunkHost}></div>
    {/if}
    {#if errorExCode}
      <ErrorCard exCode={errorExCode} {onRetry} {onOpenSettings} />
    {/if}
  {/if}
</div>

<style>
  .stream-output {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-2) 2px var(--space-4);
    font-size: var(--font-size-body);
    line-height: var(--line-height-body);
    color: var(--fg-body);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .idle-hint {
    font-size: var(--font-size-body);
    color: var(--fg-muted);
    user-select: none;
  }
  /* 呼吸圆点：三枚，1.2s 循环，透明度 0.35→1（2.5） */
  .dots {
    display: inline-flex;
    gap: 5px;
    padding: 4px 0;
  }
  .dots i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-primary);
    animation: dot-breathe var(--duration-dots) var(--ease-in) infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes dot-breathe {
    0%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    50% {
      opacity: 1;
      transform: translateY(-1.5px);
    }
  }
  .wait-static {
    color: var(--fg-muted);
    font-size: var(--font-size-secondary);
  }
  .wait-block {
    display: flex;
    align-items: center;
    min-height: 28px;
  }
  /* 长文整段等待徽标（4.3）：warning 色文字 + bg/subtle 胶囊底；12px 等宽数字 */
  .block-wait {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .eta-badge {
    align-self: flex-start;
    display: inline-flex;
    align-items: baseline;
    gap: 2px;
    padding: 2px var(--space-3);
    background: var(--bg-subtle);
    border-radius: var(--radius-pill);
    color: var(--warning);
    font-size: var(--font-size-caption);
    line-height: var(--line-height-caption);
    user-select: none;
  }
  .eta-strong {
    font-weight: var(--font-weight-medium);
  }
  .eta-num {
    font-family: var(--font-mono-stack);
    font-variant-numeric: tabular-nums;
  }
  .eta-plain {
    color: var(--fg-muted);
  }
  .block-main {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .block-text {
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
  }
  /* 增量 span：新片段 90ms 淡入（≤90ms/次；还原 animation 由 theme.css 减弱动效覆盖） */
  :global(.stream-chunk) {
    transition: opacity 90ms ease-in;
  }
  /* 光标条：2px 竖条，闪烁周期约 1s；streaming 结束由 App 切换状态（done 分支不再渲染即消失，
     完成 200ms 淡出由 CSS transition 在 done 首帧承接——组件仅移除元素，无残留） */
  .caret {
    display: inline-block;
    width: 2px;
    height: calc(var(--line-height-body) - 4px);
    margin-left: 1px;
    vertical-align: text-bottom;
    background: var(--accent-primary);
    animation: caret-blink 1s ease-in-out infinite, caret-fade var(--duration-caret-fade) ease-out;
  }
  @keyframes caret-blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.25;
    }
  }
  .whole {
    opacity: 0;
  }
  .whole-visible {
    animation: whole-in var(--duration-block-fade) var(--ease-in);
  }
  @keyframes whole-in {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
