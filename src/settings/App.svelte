<script lang="ts">
  // 设置页（label=settings，T13 实现；design-spec 7 章 + FR-13 全表 + spec 6.1/INV-8）。
  //
  // 布局：左侧六分组导航（AI 服务/翻译/面板/快捷键/截图/通用）+ 右侧内容区（design-spec 7.1）；
  // 分组与条目对齐 FR-13 十六项，出厂默认值来自后端 config（get_settings 渲染，不另立默认值）。
  //
  // 数据流（INV-8）：本页状态 = 只读镜像（settings://changed/广播回流）；任何变更经 IPC 命令
  // （set_settings / rebind_hotkey / set_clipboard_monitor），禁止前端自行改状态；
  // 即时保存（design-spec 7.4）：无「保存」按钮，控件变更即刻生效 → 行尾「已保存」1.5s 淡出；
  // 失败行内 error 常驻直至解决（热键冲突等）。
  //
  // API Key（7.3/INV-1）：仅以固定 8 位掩码「••••••••」/「未配置」展示；「修改」→ 清空录入 +
  // 录入期可见性切换（不涉及已存 Key 回显）；「保存」空值=未配置（后端清密文）。

  import { onMount } from 'svelte';

  import {
    getSettings,
    setSettings,
    setClipboardMonitor,
    rebindHotkey,
    listOcrLanguages,
    hotkeyRegistrationFailures,
  } from '../lib/ipc';
  import {
    settingsState,
    initSettingsMirror,
    watchSettingsChanged,
    watchSystemTheme,
    applyThemeAppearance,
    applySettingsView,
  } from '../lib/stores';
  import { DIRECTED_TARGET_LANGS } from '../lib/types';
  import type { HotkeyFunction, OcrLanguage, SettingsPatch, ThemeMode } from '../lib/types';
  import HotkeyRecorder from './HotkeyRecorder.svelte';

  type GroupId = 'ai' | 'translation' | 'panel' | 'hotkeys' | 'ocr' | 'general';

  const GROUPS: { id: GroupId; label: string }[] = [
    { id: 'ai', label: 'AI 服务' },
    { id: 'translation', label: '翻译' },
    { id: 'panel', label: '面板' },
    { id: 'hotkeys', label: '快捷键' },
    { id: 'ocr', label: '截图' },
    { id: 'general', label: '通用' },
  ];

  const THEME_OPTIONS: { value: ThemeMode; label: string }[] = [
    { value: 'system', label: '跟随系统' },
    { value: 'light', label: '亮色' },
    { value: 'dark', label: '暗色' },
  ];

  const HOTKEY_ROWS: { fn: HotkeyFunction; label: string }[] = [
    { fn: 'selection', label: '划词翻译' },
    { fn: 'input', label: '输入翻译' },
    { fn: 'ocrRecognize', label: '截图识别' },
    { fn: 'ocrTranslate', label: '截图翻译' },
    { fn: 'toggleScene', label: '切换场景' },
  ];

  /** design-spec 7.2 冲突/注册失败回退文案（外部占用；内部共键冲突显示后端点名消息）。 */
  const CONFLICT_RESTORED = '该快捷键已被其他程序占用，已恢复原快捷键';

  let activeGroup = $state<GroupId>('ai');
  let rowErrors = $state<Record<string, string>>({});
  let savedState = $state<Record<string, { shown: boolean; fading: boolean }>>({});
  let degraded = $state<HotkeyFunction[]>([]);
  let ocrLangs = $state<OcrLanguage[]>([]);
  let keyDraft = $state<{ editing: boolean; input: string; reveal: boolean }>({
    editing: false,
    input: '',
    reveal: false,
  });
  let keySaving = $state(false);

  const flashTimers: Record<string, number> = {};

  const settings = $derived(settingsState.current);

  /** OCR 语种下拉选项：自动（恒定首项）+ 已装清单（list_ocr_languages 数据源，防 EX-6）；
   *  已存语种不在已装清单时附「（当前未安装）」提示并保留可选（运行期后端仍二次校验）。 */
  const ocrOptions = $derived.by(() => {
    const current = settings?.ocr.language ?? 'auto';
    const list: { tag: string; label: string }[] = ocrLangs;
    if (current !== 'auto' && !list.some((l) => l.tag === current)) {
      return [...list, { tag: current, label: `${current}（当前未安装）` }];
    }
    return list;
  });

  // ---------------------------------------------------------------------------
  // 挂载：镜像拉取 + 广播订阅 + 主题 + 数据集（降级清单 / OCR 语种）
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
    void refreshDegraded();
    void refreshOcrLangs();
    return () => {
      for (const u of unlisteners) u();
    };
  });

  async function refreshSettings(): Promise<void> {
    try {
      applySettingsView(await getSettings());
    } catch (err) {
      // 命令级失败：镜像保持旧值（保守渲染）；记录而非静默
      console.warn('get_settings 刷新失败', err);
    }
  }

  async function refreshDegraded(): Promise<void> {
    try {
      degraded = await hotkeyRegistrationFailures();
    } catch (err) {
      // 服务未就绪异常路径：保守渲染（无 warning 图标），记录
      console.warn('hotkey_registration_failures 未就绪', err);
    }
  }

  async function refreshOcrLangs(): Promise<void> {
    try {
      ocrLangs = await listOcrLanguages();
    } catch (err) {
      // 命令未就绪：下拉仅剩「自动」（EX-6 由后端使用校验兜底）
      console.warn('list_ocr_languages 未就绪', err);
    }
  }

  // ---------------------------------------------------------------------------
  // 即时保存（7.4）：行尾「已保存」1.5s 淡出；失败行内 error 常驻
  // ---------------------------------------------------------------------------

  function flashSaved(key: string): void {
    savedState[key] = { shown: true, fading: false };
    const old = flashTimers[key];
    if (old !== undefined) window.clearTimeout(old);
    flashTimers[key] = window.setTimeout(() => {
      const entry = savedState[key];
      if (!entry) return;
      entry.fading = true; // 1.5s 后进入 300ms 淡出
      flashTimers[key] = window.setTimeout(() => {
        if (savedState[key] === entry) delete savedState[key];
      }, 300);
    }, 1500);
  }

  async function saveRow(key: string, patch: SettingsPatch): Promise<boolean> {
    rowErrors[key] = '';
    try {
      await setSettings(patch);
      flashSaved(key);
      return true;
    } catch (err) {
      rowErrors[key] = String(err);
      return false;
    }
  }

  async function saveClipboardRow(next: boolean): Promise<void> {
    rowErrors['clipboardMonitor'] = '';
    try {
      await setClipboardMonitor(next);
      flashSaved('clipboardMonitor');
    } catch (err) {
      rowErrors['clipboardMonitor'] = String(err);
    }
  }

  async function rebindRow(fn: HotkeyFunction, combo: string): Promise<void> {
    const key = `hotkey-${fn}`;
    rowErrors[key] = '';
    try {
      await rebindHotkey({ function: fn, combo });
      // config 为事实源（INV-8）：键帽/降级图标经权威值刷新；rebind 无广播（T4 范围）
      await refreshSettings();
      await refreshDegraded();
      flashSaved(key);
    } catch (err) {
      const msg = String(err);
      // 内部共键冲突（后端点名占用方）→ 用后端消息；外部占用/注册失败 → design-spec 固定文案
      rowErrors[key] = msg.includes('已被「') ? msg : CONFLICT_RESTORED;
    }
  }

  // ---------------------------------------------------------------------------
  // 各控件事件
  // ---------------------------------------------------------------------------

  function onBaseUrlChange(e: Event): void {
    void saveRow('baseUrl', { ai: { baseUrl: (e.currentTarget as HTMLInputElement).value } });
  }

  function onModelChange(e: Event): void {
    void saveRow('model', { ai: { model: (e.currentTarget as HTMLInputElement).value } });
  }

  function onTargetLangChange(e: Event): void {
    void saveRow('defaultTargetLang', {
      translation: { defaultTargetLang: (e.currentTarget as HTMLSelectElement).value },
    });
  }

  function onPresentationChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value;
    void saveRow('presentation', {
      translation: { presentation: value === 'staged' ? 'staged' : 'stream_all' },
    });
  }

  function onThresholdChange(e: Event): void {
    const value = Number((e.currentTarget as HTMLInputElement).value);
    if (!Number.isInteger(value) || value < 0) {
      rowErrors['threshold'] = '请输入不小于 0 的整数（单位：字符，含空白与换行）';
      return;
    }
    void saveRow('threshold', { translation: { longTextThresholdChars: value } });
  }

  /** 自动翻译开关/延迟（轮 12）：走 set_settings 通用通道，面板经镜像实时生效。 */
  const AUTO_TRANSLATE_DELAY_OPTIONS = [
    { value: 300, label: '0.3 秒' },
    { value: 500, label: '0.5 秒' },
    { value: 700, label: '0.7 秒' },
    { value: 1000, label: '1 秒' },
    { value: 1500, label: '1.5 秒' },
    { value: 2000, label: '2 秒' },
  ];

  function onAutoTranslateChange(next: boolean): void {
    void saveRow('autoTranslate', { translation: { autoTranslate: { enabled: next } } });
  }

  function onAutoTranslateDelayChange(e: Event): void {
    const value = Number((e.currentTarget as HTMLSelectElement).value);
    void saveRow('autoTranslateDelay', { translation: { autoTranslate: { delayMs: value } } });
  }

  function onHideOnBlurChange(next: boolean): void {
    void saveRow('hideOnBlur', { panel: { hideOnBlur: next } });
  }

  function onClipboardMonitorChange(next: boolean): void {
    void saveClipboardRow(next);
  }

  function onOcrLangChange(e: Event): void {
    void saveRow('ocrLang', { ocr: { language: (e.currentTarget as HTMLSelectElement).value } });
  }

  function onAutostartChange(next: boolean): void {
    void saveRow('autostart', { general: { autostart: next } });
  }

  function onThemeChange(theme: ThemeMode): void {
    void saveRow('theme', { general: { theme } });
  }

  // ---------------------------------------------------------------------------
  // API Key 修改流（7.3 / INV-1）
  // ---------------------------------------------------------------------------

  function startKeyEdit(): void {
    keyDraft = { editing: true, input: '', reveal: false }; // 录入框清空，不含任何已存值
  }

  function cancelKeyEdit(): void {
    keyDraft = { editing: false, input: '', reveal: false };
  }

  async function saveKey(): Promise<void> {
    if (keySaving) return;
    keySaving = true;
    const ok = await saveRow('apiKey', { ai: { apiKey: keyDraft.input } });
    if (ok) {
      keyDraft = { editing: false, input: '', reveal: false };
      // hasKey 不随 settings://changed 广播更新（INV-1 仅 hasKey 视图字段），权威刷新
      await refreshSettings();
    }
    keySaving = false;
  }
</script>

<svelte:head>
  <title>设置</title>
</svelte:head>

<div class="layout">
  <nav class="side" aria-label="设置分组">
    {#each GROUPS as group (group.id)}
      <button
        type="button"
        class="nav-item"
        aria-current={activeGroup === group.id ? 'page' : undefined}
        onclick={() => (activeGroup = group.id)}
      >
        {group.label}
      </button>
    {/each}
  </nav>

  <main class="content">
    {#if activeGroup === 'ai'}
      <h2 class="group-title">AI 服务</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">接口地址</span>
          <span class="desc">OpenAI 兼容协议</span>
        </div>
        <div class="row-control">
          {#if rowErrors['baseUrl']}
            <span class="row-error" role="alert">{rowErrors['baseUrl']}</span>
          {:else if savedState['baseUrl']}
            <span class="row-saved {savedState['baseUrl'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <input
            class="text-input input-wide"
            type="text"
            aria-label="接口地址"
            value={settings?.ai.baseUrl ?? ''}
            spellcheck="false"
            onchange={onBaseUrlChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">模型名</span>
          <span class="desc">服务商模型标识</span>
        </div>
        <div class="row-control">
          {#if rowErrors['model']}
            <span class="row-error" role="alert">{rowErrors['model']}</span>
          {:else if savedState['model']}
            <span class="row-saved {savedState['model'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <input
            class="text-input input-wide"
            type="text"
            aria-label="模型名"
            value={settings?.ai.model ?? ''}
            spellcheck="false"
            onchange={onModelChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">API Key</span>
          <span class="desc">本地加密存储，任何视图不回显明文（INV-1）</span>
        </div>
        <div class="row-control key-control">
          {#if rowErrors['apiKey']}
            <span class="row-error" role="alert">{rowErrors['apiKey']}</span>
          {:else if savedState['apiKey']}
            <span class="row-saved {savedState['apiKey'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          {#if !keyDraft.editing}
            <span class="key-mask" data-has-key={settings?.ai.hasKey ? '1' : '0'}>
              {settings?.ai.hasKey ? '••••••••' : '未配置'}
            </span>
            <button class="btn" type="button" disabled={!settings} onclick={startKeyEdit}>
              {settings?.ai.hasKey ? '修改' : '配置'}
            </button>
          {:else}
            <input
              class="text-input"
              type={keyDraft.reveal ? 'text' : 'password'}
              value={keyDraft.input}
              placeholder="粘贴 API Key；留空保存即清除配置"
              aria-label="API Key"
              autocomplete="off"
              oninput={(e) => (keyDraft.input = (e.currentTarget as HTMLInputElement).value)}
            />
            <button
              class="btn btn-plain"
              type="button"
              aria-label={keyDraft.reveal ? '隐藏' : '显示'}
              onclick={() => (keyDraft.reveal = !keyDraft.reveal)}
            >
              {#if keyDraft.reveal}
                隐藏
              {:else}
                显示
              {/if}
            </button>
            <button
              class="btn btn-primary"
              type="button"
              onclick={() => void saveKey()}
              disabled={keySaving}
            >
              保存
            </button>
            <button class="btn btn-plain" type="button" onclick={cancelKeyEdit}>取消</button>
          {/if}
        </div>
      </div>

      <!-- 常驻隐私提示块（requirements 第 7 章 / J3；design-spec 7.1） -->
      <div class="privacy-note" role="note">
        <span class="privacy-mark" aria-hidden="true">(!)</span>
        <span>使用翻译功能时，原文将被发送至所配置的第三方云端服务；截图识别在本机完成。</span>
      </div>
    {:else if activeGroup === 'translation'}
      <h2 class="group-title">翻译</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">定向目标语言</span>
          <span class="desc">翻译面板可临时切换，不回写默认值</span>
        </div>
        <div class="row-control">
          {#if rowErrors['defaultTargetLang']}
            <span class="row-error" role="alert">{rowErrors['defaultTargetLang']}</span>
          {:else if savedState['defaultTargetLang']}
            <span
              class="row-saved {savedState['defaultTargetLang'].fading ? 'fading' : ''}"
              role="status">已保存</span
            >
          {/if}
          <select
            class="select"
            aria-label="定向目标语言"
            value={settings?.translation.defaultTargetLang ?? '英文'}
            onchange={onTargetLangChange}
          >
            {#each DIRECTED_TARGET_LANGS as lang (lang.label)}
              <option value={lang.label}>{lang.label}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">译文呈现</span>
          <span class="desc">分档呈现下长文等待后整段显示</span>
        </div>
        <div class="row-control">
          {#if rowErrors['presentation']}
            <span class="row-error" role="alert">{rowErrors['presentation']}</span>
          {:else if savedState['presentation']}
            <span
              class="row-saved {savedState['presentation'].fading ? 'fading' : ''}"
              role="status">已保存</span
            >
          {/if}
          <select
            class="select"
            aria-label="译文呈现"
            value={settings?.translation.presentation ?? 'stream_all'}
            onchange={onPresentationChange}
          >
            <option value="stream_all">全部流式</option>
            <option value="staged">分档呈现</option>
          </select>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">长文整段阈值</span>
          <span class="desc">仅分档呈现下生效，单位字符，含空白与换行</span>
        </div>
        <div class="row-control">
          {#if rowErrors['threshold']}
            <span class="row-error" role="alert">{rowErrors['threshold']}</span>
          {:else if savedState['threshold']}
            <span class="row-saved {savedState['threshold'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <input
            class="num-input"
            type="number"
            min="0"
            step="1"
            aria-label="长文整段阈值"
            value={settings?.translation.longTextThresholdChars ?? ''}
            onchange={onThresholdChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">自动翻译</span>
          <span class="desc">原文输入或修改后自动翻译；关闭时按回车翻译</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autoTranslate']}
            <span class="row-error" role="alert">{rowErrors['autoTranslate']}</span>
          {:else if savedState['autoTranslate']}
            <span
              class="row-saved {savedState['autoTranslate'].fading ? 'fading' : ''}"
              role="status">已保存</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label="自动翻译"
            aria-checked={settings?.translation.autoTranslate?.enabled ?? false}
            disabled={!settings}
            onclick={() =>
              onAutoTranslateChange(!(settings?.translation.autoTranslate?.enabled ?? false))}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">自动翻译延迟</span>
          <span class="desc">打字停顿后等待多久触发；未开启自动翻译时不生效</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autoTranslateDelay']}
            <span class="row-error" role="alert">{rowErrors['autoTranslateDelay']}</span>
          {:else if savedState['autoTranslateDelay']}
            <span
              class="row-saved {savedState['autoTranslateDelay'].fading ? 'fading' : ''}"
              role="status">已保存</span
            >
          {/if}
          <select
            class="select"
            aria-label="自动翻译延迟"
            disabled={!settings?.translation.autoTranslate?.enabled}
            value={settings?.translation.autoTranslate?.delayMs ?? 700}
            onchange={onAutoTranslateDelayChange}
          >
            {#each AUTO_TRANSLATE_DELAY_OPTIONS as opt (opt.value)}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </div>
      </div>
    {:else if activeGroup === 'panel'}
      <h2 class="group-title">面板</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">失焦即隐</span>
          <span class="desc">面板失去焦点即隐藏；关闭后保留直至手动关闭</span>
        </div>
        <div class="row-control">
          {#if rowErrors['hideOnBlur']}
            <span class="row-error" role="alert">{rowErrors['hideOnBlur']}</span>
          {:else if savedState['hideOnBlur']}
            <span class="row-saved {savedState['hideOnBlur'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label="失焦即隐"
            aria-checked={settings?.panel.hideOnBlur ?? true}
            disabled={!settings}
            onclick={() => onHideOnBlurChange(!(settings?.panel.hideOnBlur ?? true))}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">剪贴板监听</span>
          <span class="desc">与面板开关、托盘勾选三方实时同步</span>
        </div>
        <div class="row-control">
          {#if rowErrors['clipboardMonitor']}
            <span class="row-error" role="alert">{rowErrors['clipboardMonitor']}</span>
          {:else if savedState['clipboardMonitor']}
            <span
              class="row-saved {savedState['clipboardMonitor'].fading ? 'fading' : ''}"
              role="status">已保存</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label="剪贴板监听"
            aria-checked={settings?.clipboardMonitor ?? false}
            disabled={!settings}
            onclick={() => onClipboardMonitorChange(!(settings?.clipboardMonitor ?? false))}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
      </div>
    {:else if activeGroup === 'hotkeys'}
      <h2 class="group-title">快捷键</h2>

      {#each HOTKEY_ROWS as row (row.fn)}
        {@const key = `hotkey-${row.fn}`}
        <div class="row">
          <div class="row-label">
            <span class="label">{row.label}</span>
            <span class="desc">录制式改键：冲突自动回退并通知</span>
          </div>
          <div class="row-control">
            {#if rowErrors[key]}
              <span class="row-error" role="alert">{rowErrors[key]}</span>
            {:else if savedState[key]}
              <span class="row-saved {savedState[key].fading ? 'fading' : ''}" role="status"
                >已保存</span
              >
            {/if}
            <HotkeyRecorder
              combo={settings?.hotkeys[row.fn] ?? '—'}
              degraded={degraded.includes(row.fn)}
              onRebind={(combo) => rebindRow(row.fn, combo)}
            />
          </div>
        </div>
      {/each}
    {:else if activeGroup === 'ocr'}
      <h2 class="group-title">截图</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">OCR 语种</span>
          <span class="desc">自动＝跟随系统首选语言；固定语种需系统已装语言包</span>
        </div>
        <div class="row-control">
          {#if rowErrors['ocrLang']}
            <span class="row-error" role="alert">{rowErrors['ocrLang']}</span>
          {:else if savedState['ocrLang']}
            <span class="row-saved {savedState['ocrLang'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <select
            class="select"
            aria-label="OCR 语种"
            value={settings?.ocr.language ?? 'auto'}
            onchange={onOcrLangChange}
          >
            <option value="auto">自动（跟随系统首选语言）</option>
            {#each ocrOptions as lang (lang.tag)}
              <option value={lang.tag}>{lang.label}</option>
            {/each}
          </select>
        </div>
      </div>
    {:else}
      <h2 class="group-title">通用</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">开机自启</span>
          <span class="desc">开启后随系统启动驻留托盘</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autostart']}
            <span class="row-error" role="alert">{rowErrors['autostart']}</span>
          {:else if savedState['autostart']}
            <span class="row-saved {savedState['autostart'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label="开机自启"
            aria-checked={settings?.general.autostart ?? false}
            disabled={!settings}
            onclick={() => onAutostartChange(!(settings?.general.autostart ?? false))}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">主题</span>
          <span class="desc">变更即时全窗预览</span>
        </div>
        <div class="row-control">
          {#if rowErrors['theme']}
            <span class="row-error" role="alert">{rowErrors['theme']}</span>
          {:else if savedState['theme']}
            <span class="row-saved {savedState['theme'].fading ? 'fading' : ''}" role="status"
              >已保存</span
            >
          {/if}
          <div class="segmented" role="group" aria-label="主题">
            {#each THEME_OPTIONS as opt (opt.value)}
              <button
                type="button"
                class="seg-btn"
                aria-pressed={(settings?.general.theme ?? 'system') === opt.value}
                onclick={() => onThemeChange(opt.value)}
              >
                {opt.label}
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </main>
</div>

<style>
  .layout {
    display: flex;
    height: 100vh;
    background: var(--bg-base);
    color: var(--fg-body);
    font-size: var(--font-size-body);
  }
  .side {
    flex: none;
    width: 140px;
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .nav-item {
    text-align: left;
    height: 36px;
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--fg-body);
    font-size: var(--font-size-body);
    cursor: pointer;
  }
  .nav-item:hover {
    background: var(--bg-hover);
  }
  .nav-item[aria-current='page'] {
    background: var(--bg-active);
    color: var(--fg-title);
    font-weight: var(--font-weight-medium);
  }
  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-5);
  }
  .group-title {
    margin: 0 0 var(--space-4);
    font-size: var(--font-size-title);
    line-height: var(--line-height-title);
    color: var(--fg-title);
    font-weight: var(--font-weight-medium);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    min-height: var(--setting-row-height);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .row:last-child {
    border-bottom: none;
  }
  .row-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-size: var(--font-size-body);
    color: var(--fg-title);
  }
  .desc {
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
    color: var(--fg-muted);
  }
  .row-control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    min-width: 0;
    flex: none;
    max-width: 60%;
  }
  /* 行尾反馈位（7.4）：success「已保存」1.5s 后 300ms 淡出；error 常驻 */
  .row-saved {
    font-size: var(--font-size-caption);
    color: var(--success);
    transition: opacity 300ms ease;
    opacity: 1;
    white-space: nowrap;
  }
  .row-saved.fading {
    opacity: 0;
  }
  .row-error {
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
    color: var(--error);
    max-width: 260px;
    text-align: right;
    white-space: normal;
  }
  .text-input {
    height: 32px;
    padding: 0 var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: var(--bg-subtle);
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
  }
  .input-wide {
    width: 240px;
  }
  .num-input {
    width: 96px;
    height: 32px;
    padding: 0 var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: var(--bg-subtle);
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
  }
  .select {
    height: 32px;
    padding: 0 var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    background: var(--bg-base);
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
    cursor: pointer;
    max-width: 220px;
  }
  .key-mask {
    font-size: var(--font-size-body);
    letter-spacing: 2px;
    color: var(--fg-muted);
  }
  .key-control {
    flex-wrap: wrap;
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
  .btn:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .btn:disabled {
    color: var(--fg-disabled);
    border-color: var(--border-subtle);
    cursor: default;
  }
  .btn-primary {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: var(--accent-on-primary);
  }
  .btn-primary:hover:not(:disabled) {
    background: var(--accent-primary-hover);
    border-color: var(--accent-primary-hover);
  }
  .btn-plain {
    border: none;
    background: transparent;
    color: var(--fg-muted);
  }
  .btn-plain:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--fg-body);
  }
  /* 开关（图标式切换；关=中性，开=accent 填充） */
  .switch {
    position: relative;
    width: 40px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    background: var(--bg-subtle);
    cursor: pointer;
    transition:
      background 120ms var(--ease-in),
      border-color 120ms var(--ease-in);
  }
  .switch-knob {
    position: absolute;
    top: 2px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--fg-disabled);
    transition:
      transform 120ms var(--ease-in),
      background 120ms var(--ease-in);
  }
  .switch[aria-checked='true'] {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
  }
  .switch[aria-checked='true'] .switch-knob {
    transform: translateX(17px);
    background: #ffffff;
  }
  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* 主题三段控件（120ms 选中语义：选中段 accent 底） */
  .segmented {
    display: inline-flex;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  .seg-btn {
    height: 32px;
    padding: 0 var(--space-3);
    border: none;
    background: transparent;
    color: var(--fg-body);
    font-size: var(--font-size-secondary);
    cursor: pointer;
    transition:
      background 120ms var(--ease-in),
      color 120ms var(--ease-in);
  }
  .seg-btn + .seg-btn {
    border-left: 1px solid var(--border-subtle);
  }
  .seg-btn:hover {
    background: var(--bg-hover);
  }
  .seg-btn[aria-pressed='true'] {
    background: var(--accent-primary);
    color: var(--accent-on-primary);
  }
  /* 常驻隐私提示块（requirements 第 7 章 / J3；warning-mark 强调底 + radius-card） */
  .privacy-note {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    margin-top: var(--space-5);
    padding: var(--space-3) var(--space-4);
    background: var(--bg-subtle);
    border-left: 3px solid var(--warning);
    border-radius: var(--radius-card);
    font-size: var(--font-size-secondary);
    line-height: var(--line-height-secondary);
    color: var(--fg-body);
  }
  .privacy-mark {
    flex: none;
    font-weight: var(--font-weight-medium);
    color: var(--warning);
  }
</style>
