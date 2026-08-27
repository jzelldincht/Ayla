<script lang="ts">
  // 原文编辑区（design-spec 3.3 四形态共用；用户裁决 A1：所有原文区可编辑）。
  // - 形态 A/D：裸文本/浅底框，修改后右下角浮现「重译 ↵」按钮（未修改不显示）；Enter 同效。
  // - 形态 B：输入框（bg/subtle），「翻译 ↵」按钮常驻；Enter 提交、Shift+Enter 换行。
  // 高度自适应：随内容增高，上限约面板 40%（A/B）或 35%（D，由外层 class 控制），超出内部滚动。

  import { onMount } from 'svelte';
  import { retranslateVisible } from './lib/retranslate';

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
  }: {
    value: string;
    /** 已提交版本（重译浮现判据）。 */
    submitted: string;
    oninput: (v: string) => void;
    onaction: () => void;
    /** 'always'（形态 B 按钮常驻）| 'whenModified'（形态 A/D 修改后浮现）。 */
    buttonMode: 'always' | 'whenModified';
    buttonLabel: string;
    placeholder?: string;
    /** 'bare' 裸文本（A）| 'boxed' 输入框（B）| 'subtle' 浅底框（D）。 */
    variant?: 'bare' | 'boxed' | 'subtle';
    autofocus?: boolean;
    ariaLabel: string;
  } = $props();

  let textarea: HTMLTextAreaElement;

  /** A1：重译按钮浮现 = 内容被修改（≠ 已提交）且非纯空白（FR-4.3 拦截空提交）。 */
  const showAction = $derived(
    buttonMode === 'always' ? value.trim().length > 0 : retranslateVisible(submitted, value),
  );

  // 高度自适应（≤40%（A/B 由 .limit-40 外层）/≤35%（D .limit-35），超出滚动）。
  function autosize() {
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 9999)}px`;
  }

  $effect(() => {
    autosize();
  });

  onMount(() => {
    autosize();
    if (autofocus) textarea?.focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.shiftKey) return; // Shift+Enter 换行（原生行为）
    if (buttonMode === 'always') {
      if (value.trim().length === 0) return; // FR-4.3：空白回车不发起、无错误
      e.preventDefault();
      onaction();
    } else if (showAction) {
      e.preventDefault();
      onaction();
    }
    // A/D 未修改时 Enter 走原生换行（内容不变时不允许以重译提交语义覆盖编辑行为）
  }
</script>

<div class="editor" class:limit-40={variant !== 'subtle'} class:limit-35={variant === 'subtle'}>
  <textarea
    bind:this={textarea}
    class:boxed={variant === 'boxed'}
    class:subtle={variant === 'subtle'}
    value={value}
    placeholder={placeholder}
    aria-label={ariaLabel}
    oninput={(e) => oninput((e.currentTarget as HTMLTextAreaElement).value)}
    onkeydown={onKeydown}
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
  /* 面板高度 40%/35% 上限（Webview 铺满窗口 ⇒ 100vh = 窗口高度；
     % 高在 auto 高度父容器中不成立，故用 vh——与"面板高度"语义一致） */
  .limit-40 {
    max-height: 40vh;
  }
  .limit-35 {
    max-height: 35vh;
  }
  textarea {
    width: 100%;
    min-height: 2.6em;
    max-height: 100%;
    resize: none;
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
