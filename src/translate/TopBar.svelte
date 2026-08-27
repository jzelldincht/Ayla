<script lang="ts">
  // 顶栏骨架（design-spec 3.2，四形态共用）：监听开关（左一，图标式）→ 互译/定向分段控件 →
  // （定向时）目标语言下拉 → 幽灵关闭按钮；中间弹性空白。高 44px，兼任窗口拖拽移动区
  // （data-tauri-drag-region，Tauri 2 原生拖拽区域属性）。
  //
  // 三方同步（INV-8）：开关/场景变更只发 IPC 命令（后端写 config + settings://changed 广播），
  // 本组件值一律读镜像 store（settings），由事件回流更新——不在前端乐观改值。

  import { settingsState, currentScene } from '../lib/stores';
  import { setClipboardMonitor, setSettings, setPanelTargetLang } from '../lib/ipc';
  import { DIRECTED_TARGET_LANGS } from '../lib/types';

  let {
    tempTarget,
    onTargetChange,
    onClose,
  }: {
    /** 面板临时目标语言（易失，App 层持有；未设=null→显示设置默认值）。 */
    tempTarget: string | null;
    onTargetChange: (label: string) => void;
    onClose: () => void;
  } = $props();

  const monitorOn = $derived(settingsState.current?.clipboardMonitor ?? false);
  const scene = $derived(currentScene());
  const targetValue = $derived(
    tempTarget ?? settingsState.current?.translation.defaultTargetLang ?? '英文',
  );

  async function toggleMonitor() {
    try {
      await setClipboardMonitor(!monitorOn);
    } catch (err) {
      // 命令失败由后端回滚并保持事实源一致；前端等待广播回流，仅记录（不静默吞）。
      console.warn('clipboard monitor 切换失败', err);
    }
  }

  async function switchScene(directed: boolean) {
    try {
      await setSettings({ scene: { active: directed ? 'directed' : 'mutual' } });
    } catch (err) {
      console.warn('场景切换失败', err);
    }
  }

  function onLangChange(e: Event) {
    const label = (e.currentTarget as HTMLSelectElement).value;
    onTargetChange(label);
    // FR-2.7：仅对下一次翻译生效（易失槽）；不回写默认值、不自动重译。
    void setPanelTargetLang({ lang: label }).catch((err) => console.warn('临时目标语言设置失败', err));
  }
</script>

<header class="topbar" data-tauri-drag-region>
  <div class="left-cluster">
    <!-- 监听开关：关=fg/muted 线性图标，开=accent 填充图标；tooltip 照设计文案 -->
    <button
      class="icon-btn monitor-toggle"
      type="button"
      role="switch"
      aria-checked={monitorOn}
      aria-label="剪贴板监听"
      title="开启后，自动翻译新复制的内容"
      onclick={toggleMonitor}
    >
      {#if monitorOn}
        <svg class="icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <rect x="5" y="5" width="14" height="16" rx="2" fill="var(--accent-primary)" stroke="none" />
          <rect x="5" y="5" width="14" height="14" rx="1" fill="var(--accent-primary)" stroke="none" />
          <rect x="8.5" y="3" width="7" height="4" rx="1.2" fill="var(--accent-primary)" stroke="none" />
          <rect x="8.5" y="3" width="7" height="4" rx="1.2" fill="var(--bg-base)" stroke="none" />
        </svg>
      {:else}
        <svg class="icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <rect x="5" y="6" width="14" height="14" rx="2" stroke="var(--fg-muted)" stroke-width="1.5" />
          <rect x="8.5" y="4" width="7" height="4" rx="1.2" stroke="var(--fg-muted)" stroke-width="1.5" />
        </svg>
      {/if}
    </button>

    <!-- 活动场景分段控件：两段等宽；选中段 accent 底 + on-primary 文字（Alt+W 触发时滑块同步平移） -->
    <div class="segmented" role="group" aria-label="活动场景">
      <button
        class="segment"
        type="button"
        class:selected={scene === 'mutual'}
        aria-pressed={scene === 'mutual'}
        onclick={() => switchScene(false)}
      >
        互译
      </button>
      <button
        class="segment"
        type="button"
        class:selected={scene === 'directed'}
        aria-pressed={scene === 'directed'}
        onclick={() => switchScene(true)}
      >
        定向
      </button>
      <span class="thumb" class:thumb-right={scene === 'directed'} aria-hidden="true"></span>
    </div>

    <!-- 目标语言下拉：仅定向形态出现（出现/消失 140ms 过渡，紧跟分段控件右侧 8px） -->
    <div class="lang-wrap" class:shown={scene === 'directed'} aria-hidden={scene !== 'directed'}>
      <select
        class="lang-select"
        aria-label="目标语言"
        value={targetValue}
        onchange={onLangChange}
        tabindex={scene === 'directed' ? 0 : -1}
      >
        {#each DIRECTED_TARGET_LANGS as lang (lang.label)}
          <option value={lang.label}>{lang.label}</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="right-cluster">
    <!-- 关闭：幽灵图标按钮 28×28，悬停 bg/hover，永远可见（FR-9.2） -->
    <button
      class="icon-btn close-btn"
      type="button"
      aria-label="关闭面板"
      title="关闭面板"
      onclick={onClose}
    >
      <svg class="icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path d="M7 7 L17 17 M17 7 L7 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    height: var(--topbar-height);
    padding: 0 8px 0 var(--space-2);
    flex: none;
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }
  .left-cluster,
  .right-cluster {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .right-cluster {
    margin-left: auto;
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--hit-size);
    height: var(--hit-size);
    padding: 0;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--bg-hover);
  }
  .icon-btn:active {
    background: var(--bg-active);
  }
  .icon {
    width: 20px;
    height: 20px;
    display: block;
  }
  /* 分段控件：两段等宽 + 选中滑块 120ms 平移（design-spec 2.5） */
  .segmented {
    position: relative;
    display: flex;
    height: var(--control-height-panel);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    overflow: hidden;
    background: var(--bg-subtle);
  }
  .segment {
    position: relative;
    z-index: 1;
    width: 64px;
    padding: 0 var(--space-2);
    border: none;
    background: transparent;
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
    cursor: pointer;
  }
  .segment[aria-pressed='true'] {
    color: var(--accent-on-primary);
  }
  .thumb {
    position: absolute;
    top: 1px;
    left: 1px;
    width: calc(50% - 1.5px);
    height: calc(100% - 2px);
    border-radius: calc(var(--radius-control) - 1px);
    background: var(--accent-primary);
    transition:
      transform var(--duration-segment) var(--ease-in),
      opacity 0.01ms;
  }
  .thumb-right {
    transform: translateX(100%);
  }
  /* 定向语言下拉：出现/消失 140ms（透明度 + translateY(-2px)→0） */
  .lang-wrap {
    display: inline-flex;
    opacity: 0;
    transform: translateY(-2px);
    pointer-events: none;
    transition:
      opacity var(--duration-dropdown) var(--ease-in),
      transform var(--duration-dropdown) var(--ease-in);
  }
  .lang-wrap.shown {
    opacity: 1;
    transform: translateY(0);
    pointer-events: auto;
  }
  .lang-select {
    height: var(--control-height-panel);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: var(--bg-base);
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .close-btn {
    color: var(--fg-muted);
  }
</style>
