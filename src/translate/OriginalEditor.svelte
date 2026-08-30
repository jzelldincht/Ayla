<script lang="ts">
  // 原文编辑区（design-spec 3.3 四形态共用；用户裁决 A1：所有原文区可编辑）。
  // - 形态 A/D：裸文本/浅底框，修改后右下角浮现「重译 ↵」按钮（未修改不显示）；Enter 同效。
  // - 形态 B：输入框（bg/subtle），轮 12 起无按钮（Enter 提交、Shift+Enter 换行）。
  // 高度（真机修复轮 13g/13i 用户裁决）：默认按窗口黄金分割（defaultHeight prop，App 计算）；
  // 用户拖动底部 resizer 后以用户高度为准（固定 px），防抖 600ms 经 set_settings 记忆，
  // 下次呼出自动恢复；不再用 flex 撑满（flex 会覆盖手动内联高度 → 拖不动的根因）。

  import { onMount } from 'svelte';
  import { retranslateVisible } from './lib/retranslate';
  import { setSettings } from '../lib/ipc';

  let {
    value,
    submitted,
    oninput,
    onaction,
    buttonMode,
    buttonLabel,
    placeholder = '',
    variant = 'bare',
    autofocus = false,
    ariaLabel,
    defaultHeight,
    savedHeight = null,
  }: {
    value: string;
    /** 已提交版本（重译浮现判据）。 */
    submitted: string;
    oninput: (v: string) => void;
    onaction: () => void;
    /** 'always'（按钮常驻）| 'whenModified'（修改后浮现）| 'never'（无按钮，仅 Enter 提交）。 */
    buttonMode: 'always' | 'whenModified' | 'never';
    buttonLabel: string;
    placeholder?: string;
    /** 'bare' 裸文本（A）| 'boxed' 输入框（B）| 'subtle' 浅底框（D/C）。 */
    variant?: 'bare' | 'boxed' | 'subtle';
    autofocus?: boolean;
    ariaLabel: string;
    /** 默认高度（黄金分割，CSS 值；App 按形态计算 chrome 偏移）。 */
    defaultHeight: string;
    /** 用户拖动记忆高（逻辑 px；null＝未拖过 → 黄金分割）。 */
    savedHeight?: number | null;
  } = $props();

  let textarea: HTMLTextAreaElement;
  /** 当前生效高度：null＝黄金分割默认（跟随窗口），数字＝用户拖动定值。
   *  初值由下方 $effect 自 savedHeight prop 同步（挂载即跑，含镜像到达后的回流）。 */
  let userHeight = $state<number | null>(null);
  let saveTimer: number | null = null;

  // 镜像回流同步（本窗保存后广播回来值一致；它窗/重开镜像时同步恢复）。
  $effect(() => {
    userHeight = savedHeight ?? null;
  });

  /** A1：重译按钮浮现 = 内容被修改（≠ 已提交）且非纯空白（FR-4.3 拦截空提交）。
      轮 12：'never' 模式无按钮（形态 B 去"翻译 ↵"，Enter 提交）。 */
  const showAction = $derived(
    buttonMode === 'never'
      ? false
      : buttonMode === 'always'
        ? value.trim().length > 0
        : retranslateVisible(submitted, value),
  );

  onMount(() => {
    if (autofocus) textarea?.focus();
    return () => {
      if (saveTimer !== null) window.clearTimeout(saveTimer);
    };
  });

  /** 拖动结束（mouseup/触摸松开）：浏览器 resizer 已写内联 height——采为用户定值并记忆。 */
  function onResizeEnd(): void {
    if (!textarea) return;
    const h = Math.round(textarea.getBoundingClientRect().height);
    if (h < 40) return; // resizer 拖到极限的异常读数不采信
    userHeight = h;
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      void setSettings({ panel: { editorHeight: h } }).catch((err) =>
        console.warn('editorHeight 保存失败', err),
      );
    }, 600);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key !== 'Enter' || e.shiftKey) return; // Shift+Enter 换行（原生行为）
    // 轮 12：never（无按钮纯 Enter 提交）与 always 同语义；空白回车不发起、无错误（FR-4.3）。
    if (buttonMode === 'never' || buttonMode === 'always') {
      if (value.trim().length === 0) return;
      e.preventDefault();
      onaction();
    } else if (showAction) {
      e.preventDefault();
      onaction();
    }
    // A/D 未修改时 Enter 走原生换行（内容不变时不允许以重译提交语义覆盖编辑行为）
  }
</script>

<div class="editor">
  <!-- eslint-disable-next-line svelte/no-static-interactive-events -- mouseup 仅作拖动结束采样 -->
  <textarea
    bind:this={textarea}
    class:boxed={variant === 'boxed'}
    class:subtle={variant === 'subtle'}
    value={value}
    placeholder={placeholder}
    aria-label={ariaLabel}
    style:height={userHeight !== null ? `${userHeight}px` : defaultHeight}
    oninput={(e) => oninput((e.currentTarget as HTMLTextAreaElement).value)}
    onkeydown={onKeydown}
    onmouseup={onResizeEnd}
  ></textarea>
  {#if showAction}
    <button class="action" type="button" onclick={onaction}>{buttonLabel}</button>
  {/if}
</div>

<style>
  .editor {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  textarea {
    width: 100%;
    resize: vertical; /* 四形态统一可拖（真机修复轮 13i 用户需求） */
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: transparent;
    padding: var(--space-2);
    font-size: var(--font-size-body);
    line-height: var(--line-height-body);
    overflow-y: auto;
  }
  textarea:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 1px;
  }
  textarea::placeholder {
    color: var(--fg-muted);
  }
  textarea.boxed {
    background: var(--bg-subtle);
    min-height: 56px;
  }
  textarea.subtle {
    background: var(--bg-subtle);
  }
  /* 主色小按钮（右下角浮现；形态 B 常驻）——样式同 4.4 描边动作的实心主色版 */
  .action {
    align-self: flex-end;
    margin: var(--space-2) 0 var(--space-1);
    height: var(--control-height-panel);
    padding: 0 var(--space-3);
    background: var(--accent-primary);
    border: none;
    border-radius: var(--radius-control);
    color: var(--accent-on-primary);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .action:hover {
    background: var(--accent-primary-hover);
  }
  .action:active {
    background: var(--accent-primary-active);
  }
</style>
