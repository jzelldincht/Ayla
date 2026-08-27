<script lang="ts">
  // 快捷键录制控件（T13；design-spec 7.2 状态机：展示态 → 录制态 → 校验 → 展示（成功/回退））。
  //
  // 状态机语义：
  // - 展示态：当前组合键（键帽胶囊）+「修改」按钮；启动注册失败（degraded）时行尾常驻
  //   warning 图标（title「注册失败，点击修改」，点击直接进入录制态）。
  // - 录制态：「请按下新的快捷键…」；此时 Esc 或点击控件外取消，均不改值；
  // - 录得组合（含至少一个修饰键 + 主键）→ 调 props.onRebind（父层 invoke rebind_hotkey + 刷新镜像）：
  //   成功/失败均回展示态；成功键帽经镜像刷新更新（INV-8：前端不自改值），失败由父层行内 error。
  //
  // 组合构造（Windows/tarui global-shortcut 词法）：
  // - e.code 为主键物理按键（KeyQ→Q、Digit1→1、F1、方向键…），规避 Shift 下 e.key 大小写漂移；
  // - 修饰键固定序 Ctrl+Alt+Shift+Super；至少一个修饰键才提交（排除纯修饰键与裸字母）；
  // - 录制中的按键均在冒泡前拦截（防页面其他快捷键处理）。

  let {
    combo,
    degraded = false,
    onRebind,
  } = $props<{
    combo: string;
    degraded?: boolean;
    onRebind: (newCombo: string) => Promise<void>;
  }>();

  let phase = $state<'display' | 'recording'>('display');
  let hint = $state('');
  let root: HTMLDivElement;
  let busy = $state(false);

  /** 主键名称映射（按 e.code；不支持组合键则返回 null）。 */
  function keyName(code: string): string | null {
    if (code.startsWith('Key')) return code.slice(3);
    if (code.startsWith('Digit')) return code.slice(5);
    if (/^F\d{1,2}$/.test(code)) return code;
    const named: Record<string, string> = {
      ArrowUp: 'Up',
      ArrowDown: 'Down',
      ArrowLeft: 'Left',
      ArrowRight: 'Right',
      Space: 'Space',
      Enter: 'Enter',
      Tab: 'Tab',
      Backspace: 'Backspace',
      Home: 'Home',
      End: 'End',
      PageUp: 'PageUp',
      PageDown: 'PageDown',
      Insert: 'Insert',
      Delete: 'Delete',
    };
    return named[code] ?? null;
  }

  /** 由键盘事件构造组合串；无修饰键/纯修饰键/不支持主键 → null（保持录制态）。 */
  function comboFromEvent(e: KeyboardEvent): string | null {
    const main = keyName(e.code);
    if (!main) return null;
    const parts: string[] = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super');
    // 至少一个修饰键（FR-14.2 录音式；裸键注册全局热键风险高，后端同样拒绝）
    if (parts.length === 0) return null;
    parts.push(main);
    return parts.join('+');
  }

  function begin(): void {
    phase = 'recording';
    hint = '请按下新的快捷键…（Esc 或点击其他位置取消）';
    // 点击控件外取消（design-spec 7.2）：mousedown 阶段拦截，防点击先夺焦
    window.addEventListener('mousedown', onGlobalMouseDown, true);
    window.addEventListener('keydown', onKeyDown, true);
  }

  function end(): void {
    phase = 'display';
    hint = '';
    window.removeEventListener('mousedown', onGlobalMouseDown, true);
    window.removeEventListener('keydown', onKeyDown, true);
  }

  function cancel(): void {
    end();
  }

  function onGlobalMouseDown(e: MouseEvent): void {
    if (phase !== 'recording') return;
    if (root && !root.contains(e.target as Node)) cancel();
  }

  async function onKeyDown(e: KeyboardEvent): Promise<void> {
    if (phase !== 'recording') return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      cancel(); // Esc 取消不改值
      return;
    }
    const comboStr = comboFromEvent(e);
    if (!comboStr) return; // 纯修饰键/不支持键等待主键
    if (comboStr === combo) {
      cancel(); // 同键：无实际变更（后端 rebind Noop 语义）
      return;
    }
    busy = true;
    try {
      await onRebind(comboStr);
    } finally {
      busy = false;
      end(); // 成功/失败均回展示态（失败的行内 error 由父层呈现，键帽保持旧值——后端回退语义）
    }
  }

  function onDestroy(): void {
    window.removeEventListener('mousedown', onGlobalMouseDown, true);
    window.removeEventListener('keydown', onKeyDown, true);
  }
  $effect(() => {
    return onDestroy;
  });
</script>

<div class="recorder" bind:this={root}>
  {#if phase === 'display'}
    <kbd class="keycap" title="当前快捷键">{combo}</kbd>
    {#if degraded}
      <button
        class="warn-icon"
        type="button"
        title="注册失败，点击修改"
        aria-label="注册失败，点击修改"
        onclick={begin}
      >
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path
            d="M12 3a1.2 1.2 0 0 1 1.05.62l8.3 15.2A1.1 1.1 0 0 1 20.4 20.6H3.6a1.1 1.1 0 0 1-.95-1.78l8.3-15.2A1.2 1.2 0 0 1 12 3Zm0 5.6a1.1 1.1 0 0 0-1.1 1.1v3a1.1 1.1 0 0 0 2.2 0v-3A1.1 1.1 0 0 0 12 8.6Zm0 7.4a1.3 1.3 0 1 0 0 2.6 1.3 1.3 0 0 0 0-2.6Z"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    {/if}
    <button class="btn" type="button" onclick={begin} disabled={busy}>修改</button>
  {:else}
    <span class="record-hint" role="status">{hint}</span>
    <button class="btn btn-plain" type="button" onclick={cancel}>取消</button>
  {/if}
</div>

<style>
  .recorder {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
  }
  .keycap {
    display: inline-flex;
    align-items: center;
    padding: 2px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    background: var(--bg-subtle);
    color: var(--fg-title);
    font-family: var(--font-mono-stack);
    font-size: var(--font-size-caption);
    line-height: var(--line-height-caption);
    font-variant-numeric: tabular-nums;
  }
  .warn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--warning);
    cursor: pointer;
  }
  .warn-icon:hover {
    background: var(--bg-hover);
  }
  .record-hint {
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
  }
  .btn {
    height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--bg-base);
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .btn:hover {
    background: var(--bg-hover);
  }
  .btn:disabled {
    color: var(--fg-disabled);
    border-color: var(--border-subtle);
    cursor: default;
  }
  .btn-plain {
    border: none;
    background: transparent;
    color: var(--fg-muted);
  }
</style>
