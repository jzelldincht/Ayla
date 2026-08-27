<script lang="ts">
  // 六类错误卡片（design-spec 4.4：置于译文区原位，非模态；左缘 3px error 竖条 +
  //  error-bg 底 + radius-card；文案逐条来自 src/translate/lib/errors.ts 唯一来源）。
  import { errorCardFor } from './lib/errors';

  let {
    exCode,
    onRetry,
    onOpenSettings,
  }: {
    exCode: string;
    onRetry?: () => void;
    onOpenSettings?: () => void;
  } = $props();

  const card = $derived(errorCardFor(exCode));
</script>

<div class="error-card" role="alert" data-ex-code={exCode}>
  <div class="error-bar" aria-hidden="true"></div>
  <div class="error-body">
    <div class="error-title">{card.title}</div>
    <div class="error-desc">{card.description}</div>
    <button
      class="error-action"
      type="button"
      onclick={card.action === 'retry' ? onRetry : onOpenSettings}
    >
      {card.actionLabel}
    </button>
  </div>
</div>

<style>
  .error-card {
    display: flex;
    align-items: stretch;
    gap: var(--space-3);
    margin: var(--space-2) 0;
    padding: var(--space-3) var(--space-4);
    background: var(--error-bg);
    border-radius: var(--radius-card);
    color: var(--fg-body);
    /* 错误卡片出现 140ms：透明度 + translateY(2px)→0（design-spec 2.5） */
    animation: error-in var(--duration-error) var(--ease-in);
  }
  @keyframes error-in {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .error-bar {
    width: 3px;
    flex: none;
    border-radius: var(--radius-pill);
    background: var(--error);
  }
  .error-body {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    min-width: 0;
  }
  .error-title {
    font-size: var(--font-size-body);
    line-height: var(--line-height-body);
    font-weight: var(--font-weight-medium);
    color: var(--fg-title);
  }
  .error-desc {
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
    color: var(--fg-muted);
  }
  /* 操作按钮：主色描边小按钮（design-spec 4.4） */
  .error-action {
    margin-top: var(--space-1);
    height: var(--control-height-panel);
    padding: 0 var(--space-3);
    background: transparent;
    border: 1px solid var(--accent-primary);
    border-radius: var(--radius-control);
    color: var(--accent-primary);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .error-action:hover {
    background: var(--bg-hover);
  }
  .error-action:active {
    background: var(--bg-active);
  }
</style>
