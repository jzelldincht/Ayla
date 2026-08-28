<script lang="ts">
  // 首启引导页（label=onboarding，T14 实现；design-spec 6 + FR-11 全部）。
  //
  // 模式：`mode` prop（缺省读 URL `?mode=guide`，Rust 建窗时写入）——
  // - `first`：四步向导（欢迎 → 隐私告知 → 配置 API Key → 完成）；
  // - `guide`（FR-11.4 补配引导）：直达隐私要点 + Key 步骤（两步），完成/跳过即隐藏并回调
  //   （Rust 侧 GuideFinishedHandler，T16 action_router 接线恢复挂起翻译）。
  //
  // 规则（design-spec 6 / FR-11.2/11.3）：
  // - 步骤指示器：圆点 + 当前步名；向导不支持 Esc（本页忽略 Esc，不隐藏不跳步）；
  // - 隐私告知步：完整句子 + 强调底色块平铺（无折叠、无内部滚动），「我已了解，继续」唯一前进入口，
  //   「暂不使用」= 跳过整个引导（FR-11.3 路径）；
  // - Key 步：输入框 + 录入期可见性切换 +「验证连接」（verify_connection 行内反馈，可重试可跳过）
  //   +「暂时跳过，稍后在设置中配置」；未验证也可继续；
  // - 完成/跳过 → `finish_onboarding`（写 meta.onboardingCompleted）。
  //
  // INV-1：录入中的 Key 仅存在于本页内存变量，保存经 set_settings 入参（后端即加密）；
  // 任何视图不展示已存 Key（本页永不显示掩码，仅有/无状态局部处理）。

  import { onMount } from 'svelte';

  import { setSettings, verifyConnection, finishOnboarding, onOnboardingBegin } from '../lib/ipc';
  import {
    initSettingsMirror,
    watchSettingsChanged,
    watchSystemTheme,
    applyThemeAppearance,
  } from '../lib/stores';
  import type { OnboardingMode } from '../lib/types';

  let { mode: propMode } = $props<{ mode?: OnboardingMode }>();

  const urlMode: OnboardingMode =
    new URLSearchParams(window.location.search).get('mode') === 'guide' ? 'guide' : 'first';
  /** 事件覆盖优先（onboarding://begin 每次显示复位），其次 prop（测试注入），最后 URL（Rust 建窗写入）。 */
  let eventMode = $state<OnboardingMode | null>(null);
  const mode = $derived(eventMode ?? propMode ?? urlMode);

  /** 步骤序（guide 两步，first 四步）+ 当前步名（design-spec 6）。 */
  const FIRST_STEPS = ['欢迎', '隐私告知', '配置 API Key', '完成'] as const;
  const GUIDE_STEPS = ['隐私告知', '配置 API Key'] as const;
  const stepNames = $derived(mode === 'guide' ? GUIDE_STEPS : FIRST_STEPS);

  let step = $state(0);
  let keyInput = $state('');
  let keyReveal = $state(false);
  let verifying = $state(false);
  let verifyState = $state<'idle' | 'ok' | 'error'>('idle');
  let verifyError = $state('');
  let busy = $state(false);
  let inlineError = $state('');

  const isLastStep = $derived(step === stepNames.length - 1);
  const isPrivacyStep = $derived(stepNames[step] === '隐私告知');
  const isKeyStep = $derived(stepNames[step] === '配置 API Key');

  // ---------------------------------------------------------------------------
  // 挂载：主题 + 镜像 + 复位订阅（复用窗经 onboarding://begin 复位，见 windows/onboarding.rs）
  // ---------------------------------------------------------------------------

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    const bind = (p: Promise<() => void>) => {
      void p.then((u) => unlisteners.push(u));
    };
    void initSettingsMirror();
    bind(watchSettingsChanged());
    watchSystemTheme();
    applyThemeAppearance('system');
    bind(
      onOnboardingBegin((p) => {
        eventMode = p.mode;
        step = 0;
        resetKeyStep();
      }),
    );
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      for (const u of unlisteners) u();
    };
  });

  /** 向导不支持 Esc（design-spec 6）：拦截并吞掉，不隐藏、不跳步。 */
  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function resetKeyStep(): void {
    keyInput = '';
    keyReveal = false;
    verifying = false;
    verifyState = 'idle';
    verifyError = '';
    inlineError = '';
    busy = false;
  }

  // ---------------------------------------------------------------------------
  // 步骤流转（content 区 200ms 水平滑动由 step 键控动画承载，design-spec 2.5）
  // ---------------------------------------------------------------------------

  function next(): void {
    inlineError = '';
    if (isKeyStep) {
      void advanceFromKey();
      return;
    }
    if (isLastStep) {
      void finish();
      return;
    }
    step += 1;
  }

  function skip(): void {
    inlineError = '';
    if (isPrivacyStep && mode === 'first') {
      // 「暂不使用」= 跳过整个引导（FR-11.3 路径）→ 完成步（应用照常驻留）
      step = FIRST_STEPS.length - 1;
      return;
    }
    if (isKeyStep && mode === 'first') {
      // 「暂时跳过，稍后在设置中配置」→ 完成步（FR-11.3）
      step = FIRST_STEPS.length - 1;
      return;
    }
    // guide 模式：跳过即结束（隐藏 + 回调 hasKey=false；T16 丢弃挂起翻译）
    void finish();
  }

  /** Key 步前进：先保存录入的 Key（空则跳过保存），后进入完成步/结束。 */
  async function advanceFromKey(): Promise<void> {
    if (keyInput.trim().length > 0) {
      try {
        await setSettings({ ai: { apiKey: keyInput } });
      } catch (err) {
        inlineError = `API Key 保存失败：${String(err)}`;
        return;
      }
    }
    if (mode === 'guide') {
      await finish();
      return;
    }
    step = FIRST_STEPS.length - 1; // 完成步
  }

  async function finish(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await finishOnboarding();
      // 完成后窗体由后端隐藏（保留复用）；本页无后续动作
    } catch (err) {
      inlineError = `保存失败：${String(err)}`;
      busy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // 验证连接（FR-11.1(c)/D6）：行内「验证成功」/失败原因，可重试可跳过
  // ---------------------------------------------------------------------------

  async function verify(): Promise<void> {
    if (verifying) return;
    verifying = true;
    verifyState = 'idle';
    verifyError = '';
    try {
      await verifyConnection({ key: keyInput });
      verifyState = 'ok';
    } catch (err) {
      verifyState = 'error';
      verifyError = String(err);
    } finally {
      verifying = false;
    }
  }
</script>

<svelte:head>
  <title>欢迎使用 Alya</title>
</svelte:head>

<div class="wizard">
  <header class="top">
    <div class="dots" aria-hidden="true">
      {#each stepNames as name, i (i)}
        <span class="dot {i <= step ? 'active' : ''}" data-step={name}></span>
      {/each}
    </div>
    <span class="step-name" role="status">{stepNames[step]}</span>
  </header>

  <div class="step-body-anchor">
    {#key `${step}-${mode}`}
      <div class="step-body">
        {#if stepNames[step] === '欢迎'}
          <div class="welcome">
            <div class="logo" aria-hidden="true">A</div>
            <h1 class="display">欢迎使用 Alya</h1>
            <p>在任意软件里划词、输入、复制或截图，一键得到 AI 翻译。</p>
            <div class="key-tour" aria-label="快捷键速览">
              <span>Alt+Q 划词</span>
              <span>Alt+E 输入</span>
              <span>Alt+S/D 截图</span>
            </div>
          </div>
        {:else if isPrivacyStep}
          <div class="privacy">
            <h2 class="title">隐私告知</h2>
            <div class="privacy-block">
              <p>
                <span class="mark" aria-hidden="true">(!)</span>
                使用翻译功能时，你选取的原文将被发送至你所配置的第三方云端翻译服务（出厂为 DeepSeek）。
              </p>
              <p>
                <span class="mark" aria-hidden="true">(?)</span>
                截图识别的文字提取在本机完成，截图本身不会上传。
              </p>
            </div>
            <p class="privacy-note-line">以上内容完整展示、不可折叠；阅读后才能继续。</p>
          </div>
        {:else if isKeyStep}
          <div class="key-step">
            <h2 class="title">配置 API Key（可跳过）</h2>
            <p class="lead">前往 DeepSeek 开放平台创建 API Key 并粘贴：</p>
            <div class="key-row">
              <input
                class="key-input"
                type={keyReveal ? 'text' : 'password'}
                value={keyInput}
                placeholder="sk-••••••••••••••••"
                aria-label="API Key"
                autocomplete="off"
                oninput={(e) => {
                  keyInput = (e.currentTarget as HTMLInputElement).value;
                  verifyState = 'idle';
                  verifyError = '';
                }}
              />
              <button
                class="ghost"
                type="button"
                aria-label={keyReveal ? '隐藏' : '显示'}
                onclick={() => (keyReveal = !keyReveal)}
              >
                {#if keyReveal}
                  隐藏
                {:else}
                  显示
                {/if}
              </button>
            </div>
            <div class="verify-row">
              <button
                class="btn-outline"
                type="button"
                onclick={() => void verify()}
                disabled={verifying}
              >
                {verifying ? '验证中…' : '验证连接'}
              </button>
              {#if verifyState === 'ok'}
                <span class="verify-ok" role="status">✓ 验证成功</span>
              {:else if verifyState === 'error'}
                <span class="verify-err" role="alert">✕ 验证失败：{verifyError}</span>
              {/if}
            </div>
            {#if inlineError}
              <p class="inline-error" role="alert">{inlineError}</p>
            {/if}
          </div>
        {:else}
          <div class="done">
            <h2 class="title">一切就绪</h2>
            <p>Alya 将常驻系统托盘。常用快捷键：</p>
            <div class="key-list">
              <span>划词翻译 Alt+Q</span>
              <span>输入翻译 Alt+E</span>
              <span>截图识别 Alt+S</span>
              <span>截图翻译 Alt+D</span>
              <span>切换场景 Alt+W</span>
            </div>
            <p class="muted">右键托盘图标可随时找到以上全部入口。</p>
          </div>
        {/if}
      </div>
    {/key}
  </div>

  <footer class="foot">
    <!-- 辅助链接（左侧） -->
    {#if isPrivacyStep}
      <button class="link-btn" type="button" onclick={skip}>暂不使用</button>
    {:else if isKeyStep}
      <button class="link-btn" type="button" onclick={skip}>暂时跳过，稍后在设置中配置</button>
    {:else}
      <span class="link-placeholder" aria-hidden="true"></span>
    {/if}
    <!-- 主按钮（右侧）：
         welcome「开始使用」/ privacy「我已了解，继续」（唯一前进入口）/
         key「继续」（未验证也可继续）/ done「进入 Alya」 -->
    <button class="btn-primary" type="button" onclick={next} disabled={busy}>
      {#if stepNames[step] === '欢迎'}
        开始使用
      {:else if isPrivacyStep}
        我已了解，继续
      {:else if isKeyStep}
        继续
      {:else}
        进入 Alya
      {/if}
    </button>
  </footer>
</div>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: var(--space-5) var(--space-5) var(--space-4);
    background: var(--bg-base);
    color: var(--fg-body);
    font-size: var(--font-size-body);
    border-radius: var(--radius-window);
    box-shadow: var(--shadow-panel);
    overflow: hidden;
  }
  /* 顶部步骤指示器（圆点 + 当前步名；2/4 点对应 first/guide 模式） */
  .top {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .dots {
    display: flex;
    gap: var(--space-1);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }
  .dot.active {
    background: var(--accent-primary);
  }
  .step-name {
    font-size: var(--font-size-caption);
    color: var(--fg-muted);
  }
  /* 内容区：分步切换 200ms 水平滑动（减弱动效由全局 media 兜底）；外部锚点承载 flex 伸展 */
  .step-body-anchor {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: stretch;
    justify-content: center;
  }
  .step-body {
    width: 100%;
    min-height: 0;
    display: flex;
    align-items: stretch;
    justify-content: center;
    overflow: hidden;
    animation: step-in 200ms var(--ease-in);
  }
  @keyframes step-in {
    from {
      opacity: 0;
      transform: translateX(10px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
  .display {
    margin: var(--space-2) 0 0;
    font-size: var(--font-size-display);
    line-height: var(--line-height-display);
    color: var(--fg-title);
    text-align: center;
  }
  .title {
    margin: var(--space-2) 0 var(--space-3);
    font-size: var(--font-size-title);
    line-height: var(--line-height-title);
    color: var(--fg-title);
    font-weight: var(--font-weight-medium);
    text-align: center;
  }
  .welcome {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    text-align: center;
  }
  .logo {
    width: 48px;
    height: 48px;
    display: grid;
    place-content: center;
    border-radius: var(--radius-control);
    background: var(--accent-primary);
    color: var(--accent-on-primary);
    font-size: 24px;
    font-weight: var(--font-weight-medium);
  }
  .key-tour {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-3);
    font-size: var(--font-size-caption);
    color: var(--fg-muted);
  }
  /* 隐私告知：强调底色块完整平铺（无折叠、无内部滚动；warning 强调） */
  .privacy {
    width: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: var(--space-3);
  }
  .privacy-block {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--bg-subtle);
    border-left: 3px solid var(--warning);
    border-radius: var(--radius-card);
  }
  .privacy-block p {
    margin: 0;
    line-height: var(--line-height-body);
  }
  .mark {
    margin-right: var(--space-1);
    font-weight: var(--font-weight-medium);
    color: var(--warning);
  }
  .privacy-note-line {
    margin: 0;
    font-size: var(--font-size-caption);
    line-height: var(--line-height-caption);
    color: var(--fg-muted);
    text-align: center;
  }
  /* Key 配置步 */
  .key-step {
    width: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: var(--space-3);
  }
  .lead {
    margin: 0;
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
  }
  .key-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .key-input {
    flex: 1;
    height: 36px;
    padding: 0 var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: var(--bg-subtle);
    color: var(--fg-body);
    font-size: var(--font-size-body);
  }
  .ghost {
    height: 36px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--fg-muted);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .ghost:hover {
    background: var(--bg-hover);
  }
  .verify-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 28px;
  }
  .btn-outline {
    height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--accent-primary);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--accent-primary);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .btn-outline:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .btn-outline:disabled {
    color: var(--fg-disabled);
    border-color: var(--border-subtle);
    cursor: default;
  }
  .verify-ok {
    font-size: var(--font-size-secondary);
    color: var(--success);
  }
  .verify-err {
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
    color: var(--error);
  }
  .inline-error {
    margin: 0;
    font-size: var(--font-size-secondary);
    color: var(--error);
  }
  /* 完成步 */
  .done {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    text-align: center;
  }
  .done p {
    margin: 0;
  }
  .key-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    align-items: flex-start;
    margin: var(--space-2) 0;
  }
  .key-list span {
    font-size: var(--font-size-secondary);
  }
  .muted {
    font-size: var(--font-size-caption);
    color: var(--fg-muted);
  }
  /* 底部：左辅助链接 + 右主按钮 */
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 36px;
  }
  .link-btn {
    height: 32px;
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--fg-muted);
    font-size: var(--font-size-secondary);
    cursor: pointer;
  }
  .link-btn:hover {
    background: var(--bg-hover);
    color: var(--fg-body);
  }
  .link-placeholder {
    min-width: 0;
  }
  .btn-primary {
    height: 36px;
    padding: 0 var(--space-4);
    border: none;
    border-radius: var(--radius-control);
    background: var(--accent-primary);
    color: var(--accent-on-primary);
    font-size: var(--font-size-body);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }
  .btn-primary:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }
  .btn-primary:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
