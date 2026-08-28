<script lang="ts">
  // 翻译面板页（label=translate，T12 实现；spec 5.6 各链路前端段 + design-spec 第 3/4 章）。
  //
  // 数据流：
  // - 后端统一 show 后发射 `panel://show {mode, text?, sessionId?}`（6.2）——本页据此切四形态；
  // - 形态 A（划词/剪贴板监听）：展示原文后前端 invoke `translate {text, source}`（spec 5.6.1⑥）；
  // - 形态 B（输入）：用户输入后回车/按钮 invoke translate（FR-4.1~4.3）；
  // - 形态 C（OCR 识别）：`ocr://result` 展示（勿需 API Key）；语种下拉 → reocr（FR-6.2）；
  //   「送翻译」→ send_ocr_to_translate（FR-6.5），begin 到达后转形态 D；
  // - 形态 D（截图翻译）：识别原文（可编辑，用户裁决 A1）+ 译文流；Alt+D 全景 begin 后自动转入；
  // - 译文：translation://begin/chunk/done/error（C1 renderMode 驱动流式/整段；EX-5 partial 保留）。
  //
  // 键盘契约（design-spec 3.5）：Esc 隐藏（进行中翻译随之中止——cancel + hide）；
  // Enter 提交/重译、Shift+Enter 换行（OriginalEditor 内处理）；Tab 依 DOM 顺序流转。
  //
  // INV-8：本页所有设置状态只读镜像 store（settings://changed/广播回流）；写动作仅经 IPC 命令。

  import { onMount } from 'svelte';

  import {
    translate,
    cancelTranslation,
    sendOcrToTranslate,
    reocr,
    listOcrLanguages,
    openSettings,
    onPanelShow,
    onTranslationBegin,
    onTranslationChunk,
    onTranslationDone,
    onTranslationError,
    onOcrResult,
    onSelectionClosed,
    panelHide,
  } from '../lib/ipc';
  import {
    settingsState,
    currentScene,
    initSettingsMirror,
    watchSettingsChanged,
    watchSystemTheme,
    applyThemeAppearance,
  } from '../lib/stores';
  import type {
    OcrLanguage,
    PanelMode,
    RenderMode,
    TranslateSource,
    TranslationErrorPayload,
  } from '../lib/types';
  import { computeEtaSeconds, ETA_RATE_WINDOW_MS } from './lib/estimate';
  import { OCR_LANGPACK_TITLE, ocrLangpackDescription, MICROSOFT_GUIDE_URL } from './lib/errors';
  import TopBar from './TopBar.svelte';
  import OriginalEditor from './OriginalEditor.svelte';
  import StreamOutput from './StreamOutput.svelte';

  type FormMode = 'A' | 'B' | 'C' | 'D';
  type OutStatus = 'idle' | 'waiting' | 'streaming' | 'block' | 'done' | 'error';

  // ---------------------------------------------------------------------------
  // 面板状态
  // ---------------------------------------------------------------------------

  let form = $state<FormMode>('B');
  let panelMode = $state<PanelMode>('input');
  let original = $state('');
  let submitted = $state('');
  let outputText = $state('');
  let outStatus = $state<OutStatus>('idle');
  let renderMode = $state<RenderMode>('stream');
  let currentRound = $state<string | null>(null);
  let errorPayload = $state<TranslationErrorPayload | null>(null);
  let roundKey = $state(0);
  let etaSeconds = $state<number | null>(null);
  let ocrEmpty = $state(false);
  let ocrError = $state<'ex6' | 'ocr' | null>(null);
  let ocrBusy = $state(false);
  let ocrLangs = $state<OcrLanguage[]>([]);
  let ocrLang = $state('auto');
  let sessionId = $state<string | null>(null);
  let tempTarget = $state<string | null>(null);
  /** 最近一轮使用的来源（重试/重译参数一致性）。 */
  let activeSource = $state<TranslateSource>('selection');

  const targetLabel = $derived(
    tempTarget ?? settingsState.current?.translation.defaultTargetLang ?? '英文',
  );
  const translationLabel = $derived(currentScene() === 'directed' ? `译文 · ${targetLabel}` : '译文');

  // ---------------------------------------------------------------------------
  // 事件接线
  // ---------------------------------------------------------------------------

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    const bind = (p: Promise<() => void>) => {
      void p.then((u) => unlisteners.push(u));
    };

    void initSettingsMirror();
    bind(watchSettingsChanged());
    watchSystemTheme();
    // 初始主题（镜像未达前按 system 缺省渲染，权威值在镜像到达/变更事件后对齐）
    applyThemeAppearance('system');

    bind(onPanelShow((payload) => handlePanelShow(payload.mode, payload)));
    bind(onTranslationBegin((p) => handleBegin(p.roundId, p.renderMode)));
    bind(onTranslationChunk((p) => handleChunk(p.roundId, p.delta)));
    bind(onTranslationDone((p) => handleDone(p.roundId)));
    bind(onTranslationError((p) => handleError(p)));
    bind(onOcrResult((p) => handleOcrResult(p.sessionId, p.text)));
    bind(onSelectionClosed((p) => {
      if (p.sessionId === sessionId) sessionId = null;
    }));

    window.addEventListener('keydown', onWindowKeydown);
    // Esc 先隐藏前若活动轮存在 → 中止（design-spec 3.5 任何形态任何状态）
    return () => {
      window.removeEventListener('keydown', onWindowKeydown);
      for (const u of unlisteners) u();
    };
  });

  // ---------------------------------------------------------------------------
  // panel://show 与四形态切换
  // ---------------------------------------------------------------------------

  function handlePanelShow(mode: PanelMode, p: { text?: string; sessionId?: string }) {
    panelMode = mode;
    errorPayload = null;
    outputText = '';
    currentRound = null;
    renderMode = 'stream';
    outStatus = 'idle';
    ocrError = null;
    ocrEmpty = false;
    submitted = '';
    roundKey += 1;

    if (mode === 'selection' || mode === 'clipboard') {
      form = 'A';
      original = p.text ?? '';
      if (original.trim()) {
        void startTranslation(original, mode);
      }
    } else if (mode === 'input') {
      form = 'B';
      original = '';
    } else {
      // ocr：识别结果形态（Alt+S）；Alt+D 的翻译 begin 到达后自动转形态 D
      form = 'C';
      sessionId = p.sessionId ?? null;
      original = p.text ?? '';
      submitted = p.text ?? '';
      ocrLang = 'auto';
      void ensureOcrLangs();
      if (original.trim()) ocrEmpty = false;
    }
  }

  // ---------------------------------------------------------------------------
  // 翻译发起（EX-13 顶替语义前端段：新一轮清空译文区进入等待态）
  // ---------------------------------------------------------------------------

  async function startTranslation(text: string, source: TranslateSource): Promise<void> {
    if (text.trim().length === 0) return; // FR-4.3/EX-9 前端拦截：不发起、无错误
    submitted = text;
    activeSource = source;
    outputText = '';
    errorPayload = null;
    renderMode = 'stream';
    outStatus = 'waiting';
    currentRound = null;
    etaSeconds = null;
    try {
      const res = await translate({ text, source });
      currentRound = res.roundId;
    } catch (err) {
      // 命令级失败（非流内错误）：无 EX 结构化语义，用通用失败卡片呈现（不静默）
      errorPayload = {
        roundId: '',
        ex_code: 'internal',
        message: String(err),
        retriable: true,
      };
      outStatus = 'error';
    }
  }

  async function sendOcrToTranslation() {
    if (!sessionId || original.trim().length === 0 || ocrEmpty) return;
    submitted = original;
    activeSource = 'ocr';
    outputText = '';
    errorPayload = null;
    renderMode = 'stream';
    outStatus = 'waiting';
    currentRound = null;
    etaSeconds = null;
    try {
      const res = await sendOcrToTranslate({ sessionId });
      currentRound = res.roundId;
    } catch (err) {
      errorPayload = { roundId: '', ex_code: 'internal', message: String(err), retriable: true };
      outStatus = 'error';
    }
  }

  /** 重试（4.4/4.5）：以相同参数重新发起本轮完整翻译，替换当前错误卡片/部分结果。 */
  function retry() {
    if (submitted.trim().length === 0) return;
    void startTranslation(submitted, activeSource);
  }

  // ---------------------------------------------------------------------------
  // translation://* 事件（roundId 守卫：仅呈现最新一轮，EX-13）
  // ---------------------------------------------------------------------------

  function handleBegin(roundId: string, mode: RenderMode) {
    if (roundId !== currentRound) {
      currentRound = roundId;
      roundKey += 1; // 新轮：译文区清空进入新一轮等待态（含 Alt+D 后端发起的前端未知轮）
    }
    renderMode = mode;
    outputText = '';
    errorPayload = null;
    etaSeconds = null;
    outStatus = mode === 'block' ? 'block' : 'waiting';
    // 送翻/Alt+D：begin 到达 → 形态 C 转形态 D（识别原文 + 译文区）
    if (form === 'C' && original.trim()) form = 'D';
  }

  function handleChunk(roundId: string, delta: string) {
    if (roundId !== currentRound) return; // 旧轮残留增量（EX-13 顶替）直接丢弃
    if (outStatus === 'error') return;
    if (renderMode === 'block') {
      outputText += delta; // 整段模式：缓冲不渲染（4.3）
      return;
    }
    if (outStatus === 'waiting') outStatus = 'streaming'; // 首字到达移除进度条（4.3）
    if (outStatus === 'done') return; // 终态后残留增量忽略
    outputText += delta;
  }

  function handleDone(roundId: string) {
    if (roundId !== currentRound) return;
    if (outStatus === 'waiting' || outStatus === 'streaming' || outStatus === 'block') {
      outStatus = 'done';
    }
  }

  function handleError(p: TranslationErrorPayload) {
    if (currentRound !== null && p.roundId !== currentRound) return; // 旧轮错误不呈现
    // EX-5：保留已收部分（4.5 正常样式保留，绝不删除/置灰），其后紧跟错误卡片
    outputText = p.partial ?? '';
    errorPayload = p;
    etaSeconds = null;
    outStatus = 'error';
  }

  // ---------------------------------------------------------------------------
  // ocr://result 与语种（FR-6.2/FR-7.2）
  // ---------------------------------------------------------------------------

  function handleOcrResult(sid: string, text: string) {
    if (sid !== sessionId) return;
    original = text;
    submitted = text;
    ocrEmpty = text.trim().length === 0;
    if (ocrEmpty) outStatus = 'idle';
  }

  async function ensureOcrLangs() {
    if (ocrLangs.length > 0) return;
    try {
      ocrLangs = await listOcrLanguages();
    } catch {
      // 命令未就绪/失败：下拉仅剩「自动」（EX-6 由后端使用校验兜底）
    }
  }

  async function onOcrLangChange(lang: string) {
    ocrLang = lang;
    if (!sessionId || lang === 'auto' || lang === ocrLang) return;
    ocrBusy = true;
    ocrError = null;
    try {
      const res = await reocr({ sessionId, lang });
      original = res.text;
      submitted = res.text;
      ocrEmpty = res.text.trim().length === 0;
    } catch (err) {
      const msg = String(err);
      ocrError = msg.includes('语言包') ? 'ex6' : 'ocr';
    } finally {
      ocrBusy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // 键盘（Esc）与关闭（FR-1.3：关闭/隐藏仅隐藏面板，进程驻留托盘）
  // ---------------------------------------------------------------------------

  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      // 进行中的翻译随之中止（design-spec 3.5；其后 hide）
      if (currentRound) {
        void cancelTranslation({ roundId: currentRound }).catch(() => {
          // 已结束/竞态：后端幂等，无需前端动作
        });
      }
      void hideWindow();
    }
  }

  async function hideWindow() {
    try {
      // 真机修复轮 2：getCurrentWindow().hide() 被 Tauri ACL 拒绝且静默吞错，改走自定义命令。
      await panelHide();
    } catch {
      // 非 Tauri 环境（vitest/浏览器）或窗口已销毁：忽略
    }
  }

  function openSettingsClick() {
    void openSettings().catch((err) => console.warn('open_settings 未就绪', err));
  }

  // ---------------------------------------------------------------------------
  // 整段等待：预估剩余时间（每秒刷新；近 2 秒窗口速率外推）
  // ---------------------------------------------------------------------------

  let etaSamples: Array<{ t: number; produced: number }> = [];

  $effect(() => {
    if (outStatus !== 'block') {
      etaSamples = [];
      etaSeconds = null;
      return;
    }
    const timer = window.setInterval(() => {
      const now = performance.now();
      etaSamples.push({ t: now, produced: outputText.length });
      const windowStart = now - ETA_RATE_WINDOW_MS;
      while (etaSamples.length > 1 && etaSamples[0].t < windowStart) etaSamples.shift();
      const first = etaSamples[0];
      const last = etaSamples[etaSamples.length - 1];
      const dtSec = (last.t - first.t) / 1000;
      const rate = dtSec > 0 ? (last.produced - first.produced) / dtSec : 0;
      etaSeconds = computeEtaSeconds({
        sourceLength: submitted.length,
        produced: outputText.length,
        ratePerSecond: rate,
      });
    }, 1000);
    return () => window.clearInterval(timer);
  });
</script>

<div class="panel-container">
  <TopBar {tempTarget} onTargetChange={(v) => (tempTarget = v)} onClose={hideWindow} />

  {#if outStatus === 'waiting' && renderMode === 'stream'}
    <!-- 2px 高不确定细进度条（accent，往复滑动），首字到达即移除（design-spec 4.3） -->
    <div class="indeterminate" aria-hidden="true"></div>
  {/if}

  <main class="body">
    <!-- 形态 A：翻译（划词 / 剪贴板监听结果互译·定向共用） -->
    {#if form === 'A'}
      <section class="src-block">
        <div class="label">原文</div>
        <OriginalEditor
          value={original}
          submitted={submitted}
          oninput={(v) => (original = v)}
          onaction={() => void startTranslation(original, panelMode === 'clipboard' ? 'clipboard' : 'selection')}
          buttonMode="whenModified"
          buttonLabel="重译 ↵"
          variant="bare"
          ariaLabel="原文"
        />
      </section>
      <div class="divider" aria-hidden="true"></div>
      <section class="dest-block">
        <div class="label">{translationLabel}</div>
        <StreamOutput
          status={outStatus}
          text={outputText}
          renderMode={renderMode}
          etaSeconds={etaSeconds}
          errorExCode={errorPayload?.ex_code ?? null}
          roundKey={roundKey}
          onRetry={retry}
          onOpenSettings={openSettingsClick}
        />
      </section>

    {:else if form === 'B'}
      <!-- 形态 B：输入翻译（Alt+E / 托盘 / 二次启动） -->
      <section class="src-block">
        <OriginalEditor
          value={original}
          submitted={submitted}
          oninput={(v) => (original = v)}
          onaction={() => void startTranslation(original, 'input')}
          buttonMode="always"
          buttonLabel="翻译 ↵"
          placeholder="输入要翻译的文本…"
          variant="boxed"
          autofocus
          ariaLabel="输入要翻译的文本"
        />
      </section>
      <!-- 真机修复轮 1：形态 B 也保留分隔线（与形态 A 一致），并留出 12px 呼吸间距——
           消除"翻译后/首次进入横线时有时无、译文与原文挤在一起"的不一致。 -->
      <div class="divider" aria-hidden="true"></div>
      <section class="dest-block">
        <div class="label">译文</div>
        <StreamOutput
          status={outStatus}
          text={outputText}
          renderMode={renderMode}
          etaSeconds={etaSeconds}
          errorExCode={errorPayload?.ex_code ?? null}
          roundKey={roundKey}
          onRetry={retry}
          onOpenSettings={openSettingsClick}
        />
        <div class="caption">Enter 翻译 · Shift+Enter 换行</div>
      </section>

    {:else if form === 'C'}
      <!-- 形态 C：截图识别结果（不需要 API Key；FR-6.2/6.5） -->
      <section class="func-row">
        <span class="func-title">识别结果</span>
        <select
          class="lang-select"
          aria-label="OCR 语种"
          value={ocrLang}
          onchange={(e) => void onOcrLangChange((e.currentTarget as HTMLSelectElement).value)}
        >
          <option value="auto">自动</option>
          {#each ocrLangs as lang (lang.tag)}
            <option value={lang.tag}>{lang.label}</option>
          {/each}
        </select>
        <button
          class="btn-outline"
          type="button"
          onclick={() => void sendOcrToTranslation()}
          disabled={ocrEmpty || original.trim().length === 0}
        >
          送翻译
        </button>
      </section>

      {#if ocrError === 'ex6' || ocrError === 'ocr'}
        <div class="ocr-error-bar" role="alert">
          <span class="ocr-error-title">{OCR_LANGPACK_TITLE}</span>
          <span class="ocr-error-desc">
            {ocrError === 'ex6' ? ocrLangpackDescription(ocrLang === 'auto' ? null : ocrLang) : '识别失败，请重试。'}
          </span>
          {#if ocrError === 'ex6'}
            <a class="guide-link" href={MICROSOFT_GUIDE_URL} target="_blank" rel="noreferrer">查看微软官方指引</a>
          {/if}
        </div>
      {/if}

      {#if ocrEmpty}
        <div class="empty-ocr" role="status">
          <div>未识别到文字</div>
          <div class="empty-sub">请调整选区后重试</div>
        </div>
      {:else}
        {#if original.trim().length === 0 && ocrBusy}
          <div class="ocr-loading" role="status">识别中…</div>
        {/if}
        <OriginalEditor
          value={original}
          submitted={submitted}
          oninput={(v) => (original = v)}
          onaction={() => void startTranslation(original, 'ocr')}
          buttonMode="whenModified"
          buttonLabel="重译 ↵"
          variant="subtle"
          ariaLabel="识别结果"
        />
      {/if}

    {:else}
      <!-- 形态 D：截图翻译结果（识别原文可编辑 + 译文区；4.2 空态不渲染译文区） -->
      {#if ocrEmpty}
        <div class="empty-ocr" role="status">
          <div>未识别到文字</div>
          <div class="empty-sub">请调整选区后重试</div>
        </div>
      {:else}
        <section class="src-block">
          <div class="label">识别原文</div>
          <OriginalEditor
            value={original}
            submitted={submitted}
            oninput={(v) => (original = v)}
            onaction={() => void startTranslation(original, 'ocr')}
            buttonMode="whenModified"
            buttonLabel="重译 ↵"
            variant="subtle"
            ariaLabel="识别原文"
          />
        </section>
        <div class="divider" aria-hidden="true"></div>
        <section class="dest-block">
          <div class="label">{translationLabel}</div>
          <StreamOutput
            status={outStatus}
            text={outputText}
            renderMode={renderMode}
            etaSeconds={etaSeconds}
            errorExCode={errorPayload?.ex_code ?? null}
            roundKey={roundKey}
            onRetry={retry}
            onOpenSettings={openSettingsClick}
          />
        </section>
      {/if}
    {/if}
  </main>
</div>

<style>
  .panel-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 0 var(--panel-padding-x);
    background: var(--bg-base);
    border-radius: var(--radius-window);
    box-shadow: var(--shadow-panel);
    overflow: hidden;
    color: var(--fg-body);
    font-size: var(--font-size-body);
  }
  /* 2px 不确定进度条（accent 往复滑动；首字到达即移除） */
  .indeterminate {
    position: relative;
    height: 2px;
    flex: none;
    overflow: hidden;
    background: var(--border-subtle);
  }
  .indeterminate::after {
    content: '';
    position: absolute;
    left: -30%;
    width: 30%;
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent-primary);
    animation: indet-slide 1.2s var(--ease-in) infinite;
  }
  @keyframes indet-slide {
    0% {
      left: -30%;
    }
    100% {
      left: 100%;
    }
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding-bottom: var(--space-2);
  }
  .src-block {
    flex: 0 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .dest-block {
    flex: 1 1 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .divider {
    flex: none;
    height: 1px;
    border-top: 1px solid var(--border-subtle);
    /* 真机修复轮 1：间距从 8px 增至 12px——译文不再与原文输入框贴死，且各状态一致。 */
    margin: var(--space-3) 0;
  }
  .label {
    font-size: var(--font-size-caption);
    line-height: var(--line-height-caption);
    color: var(--fg-muted);
    user-select: none;
  }
  .caption {
    flex: none;
    font-size: var(--font-size-caption);
    line-height: var(--line-height-caption);
    color: var(--fg-muted);
    user-select: none;
  }
  /* 形态 C 功能行 */
  .func-row {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0 var(--space-2);
  }
  .func-title {
    font-size: var(--font-size-caption);
    color: var(--fg-muted);
    margin-right: auto;
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
  /* 送翻译：主色描边样式（非实心，与形态 B 主动作区分层级，3.3-C） */
  .btn-outline {
    height: var(--control-height-panel);
    padding: 0 var(--space-3);
    background: transparent;
    border: 1px solid var(--accent-primary);
    border-radius: var(--radius-control);
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
  /* EX-6 错误横条（4.4 附注；形态 C/D 功能行下方） */
  .ocr-error-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    margin-bottom: var(--space-2);
    background: var(--error-bg);
    border-left: 3px solid var(--error);
    border-radius: var(--radius-card);
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
  }
  .ocr-error-title {
    color: var(--fg-title);
    font-weight: var(--font-weight-medium);
  }
  .ocr-error-desc {
    color: var(--fg-muted);
  }
  .guide-link {
    color: var(--accent-primary);
  }
  .ocr-loading {
    padding: var(--space-3) var(--space-2);
    color: var(--fg-muted);
    font-size: var(--font-size-secondary);
  }
  /* 空态（4.2 形态 D / FR-7.2）：未识别到文字 + 副行；不渲染译文区、不发起请求 */
  .empty-ocr {
    flex: 1;
    min-height: 0;
    display: grid;
    place-content: center;
    text-align: center;
    gap: var(--space-1);
    color: var(--fg-body);
  }
  .empty-sub {
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
    user-select: none;
  }
</style>
