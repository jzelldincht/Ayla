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
    verifyConnection,
    restoreDefaultSettings,
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
  import { t, targetLangDisplay } from '../lib/i18n.svelte';
  import type {
    HotkeyFunction,
    LanguageSetting,
    OcrLanguage,
    SettingsPatch,
    ThemeMode,
  } from '../lib/types';
  import HotkeyRecorder from './HotkeyRecorder.svelte';

  type GroupId = 'ai' | 'translation' | 'panel' | 'hotkeys' | 'ocr' | 'general';

  // i18n-1：label 存 i18n 键，渲染时 t()。
  const GROUPS: { id: GroupId; label: string }[] = [
    { id: 'ai', label: 'settings.groupAi' },
    { id: 'translation', label: 'settings.groupTranslation' },
    { id: 'panel', label: 'settings.groupPanel' },
    { id: 'hotkeys', label: 'settings.groupHotkeys' },
    { id: 'ocr', label: 'settings.groupOcr' },
    { id: 'general', label: 'settings.groupGeneral' },
  ];

  const THEME_OPTIONS: { value: ThemeMode; label: string }[] = [
    { value: 'system', label: 'settings.themeSystem' },
    { value: 'light', label: 'settings.themeLight' },
    { value: 'dark', label: 'settings.themeDark' },
  ];

  const HOTKEY_ROWS: { fn: HotkeyFunction; label: string }[] = [
    { fn: 'selection', label: 'settings.hotkeySelection' },
    { fn: 'input', label: 'settings.hotkeyInput' },
    { fn: 'ocrRecognize', label: 'settings.hotkeyOcrRecognize' },
    { fn: 'ocrTranslate', label: 'settings.hotkeyOcrTranslate' },
    { fn: 'toggleScene', label: 'settings.hotkeyToggleScene' },
  ];

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
  // 清单 P1-3：长文整段阈值仅在「分档呈现」下生效——跟随模式联动禁用（与延迟下拉一致）。
  const isStaged = $derived((settings?.translation.presentation ?? 'stream_all') === 'staged');

  // 清单 P1-1：设置页验证连接（缺省用已存值；Key 编辑中禁用——验证的是将保存的配置）。
  let verifying = $state(false);
  let verifyState = $state<'idle' | 'ok' | 'error'>('idle');
  let verifyError = $state('');

  async function verifyFromSettings(): Promise<void> {
    if (verifying) return;
    verifying = true;
    verifyState = 'idle';
    verifyError = '';
    rowErrors['verifyConnection'] = '';
    try {
      await verifyConnection(keyDraft.editing ? { key: keyDraft.input } : {});
      verifyState = 'ok';
    } catch (err) {
      verifyState = 'error';
      verifyError = String(err);
      rowErrors['verifyConnection'] = verifyError;
    } finally {
      verifying = false;
    }
  }

  // 清单 P3-11：恢复出厂默认（后端保留 API Key/引导状态/界面语言；confirm 二次确认）。
  let restoring = $state(false);

  async function onRestoreDefaults(): Promise<void> {
    if (restoring) return;
    if (!window.confirm(t('settings.restoreDefaultsConfirm'))) return;
    restoring = true;
    rowErrors['restoreDefaults'] = '';
    try {
      await restoreDefaultSettings();
      await refreshSettings();
      flashSaved('restoreDefaults');
    } catch (err) {
      rowErrors['restoreDefaults'] = String(err);
    } finally {
      restoring = false;
    }
  }

  /** OCR 语种下拉选项：自动（恒定首项）+ 已装清单（list_ocr_languages 数据源，防 EX-6）；
   *  已存语种不在已装清单时附「（当前未安装）」提示并保留可选（运行期后端仍二次校验）。 */
  const ocrOptions = $derived.by(() => {
    const current = settings?.ocr.language ?? 'auto';
    const list: { tag: string; label: string }[] = ocrLangs;
    if (current !== 'auto' && !list.some((l) => l.tag === current)) {
      return [...list, { tag: current, label: `${current}${t('settings.ocrLangNotInstalled')}` }];
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
      rowErrors[key] = String(err); // i18n-3：后端已按界面语言本地化，原样呈现
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
      rowErrors['threshold'] = t('settings.thresholdError');
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

  function onLanguageChange(e: Event): void {
    const language = (e.currentTarget as HTMLSelectElement).value as LanguageSetting;
    void saveRow('language', { general: { language } });
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
  <title>{t('settings.title')}</title>
</svelte:head>

<div class="layout">
  <nav class="side" aria-label={t('settings.title')}>
    {#each GROUPS as group (group.id)}
      <button
        type="button"
        class="nav-item"
        aria-current={activeGroup === group.id ? 'page' : undefined}
        onclick={() => (activeGroup = group.id)}
      >
        {t(group.label)}
      </button>
    {/each}
  </nav>

  <main class="content">
    {#if activeGroup === 'ai'}
      <h2 class="group-title">{t('settings.groupAi')}</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.baseUrl')}</span>
          <span class="desc">{t('settings.baseUrlDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['baseUrl']}
            <span class="row-error" role="alert">{rowErrors['baseUrl']}</span>
          {:else if savedState['baseUrl']}
            <span class="row-saved {savedState['baseUrl'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <input
            class="text-input input-wide"
            type="text"
            aria-label={t('settings.baseUrl')}
            value={settings?.ai.baseUrl ?? ''}
            spellcheck="false"
            onchange={onBaseUrlChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.model')}</span>
          <span class="desc">{t('settings.modelDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['model']}
            <span class="row-error" role="alert">{rowErrors['model']}</span>
          {:else if savedState['model']}
            <span class="row-saved {savedState['model'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <input
            class="text-input input-wide"
            type="text"
            aria-label={t('settings.model')}
            value={settings?.ai.model ?? ''}
            spellcheck="false"
            onchange={onModelChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.apiKey')}</span>
          <span class="desc">{t('settings.apiKeyDesc')}</span>
        </div>
        <div class="row-control key-control">
          {#if rowErrors['apiKey']}
            <span class="row-error" role="alert">{rowErrors['apiKey']}</span>
          {:else if savedState['apiKey']}
            <span class="row-saved {savedState['apiKey'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          {#if !keyDraft.editing}
            <span class="key-mask" data-has-key={settings?.ai.hasKey ? '1' : '0'}>
              {settings?.ai.hasKey ? '••••••••' : t('settings.notConfigured')}
            </span>
            <button class="btn" type="button" disabled={!settings} onclick={startKeyEdit}>
              {settings?.ai.hasKey ? t('settings.edit') : t('settings.setUp')}
            </button>
          {:else}
            <input
              class="text-input"
              type={keyDraft.reveal ? 'text' : 'password'}
              value={keyDraft.input}
              placeholder={t('settings.apiKeyPlaceholder')}
              aria-label="API Key"
              autocomplete="off"
              oninput={(e) => (keyDraft.input = (e.currentTarget as HTMLInputElement).value)}
            />
            <button
              class="btn btn-plain"
              type="button"
              aria-label={keyDraft.reveal ? t('settings.hide') : t('settings.show')}
              onclick={() => (keyDraft.reveal = !keyDraft.reveal)}
            >
              {#if keyDraft.reveal}
                {t('settings.hide')}
              {:else}
                {t('settings.show')}
              {/if}
            </button>
            <button
              class="btn btn-primary"
              type="button"
              onclick={() => void saveKey()}
              disabled={keySaving}
            >
              {t('settings.save')}
            </button>
            <button class="btn btn-plain" type="button" onclick={cancelKeyEdit}>{t('settings.cancel')}</button>
          {/if}
        </div>
      </div>

      <!-- 清单 P1-1：验证连接（与引导页同款；缺省用已存值，Key 编辑中禁用——验证的是将保存的配置） -->
      <div class="row">
        <div class="row-label">
          <span class="label">{t('onb.verify')}</span>
          <span class="desc">{t('settings.verifyRowDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['verifyConnection']}
            <span class="row-error" role="alert">{t('onb.verifyFail', { msg: rowErrors['verifyConnection'] })}</span>
          {:else if verifyState === 'ok'}
            <span class="verify-ok" role="status">{t('onb.verifyOk')}</span>
          {/if}
          <button class="btn" type="button" disabled={verifying || keyDraft.editing} onclick={verifyFromSettings}>
            {verifying ? t('onb.verifying') : t('onb.verify')}
          </button>
        </div>
      </div>

      <!-- 常驻隐私提示块（requirements 第 7 章 / J3；design-spec 7.1） -->
      <div class="privacy-note" role="note">
        <span class="privacy-mark" aria-hidden="true">(!)</span>
        <span>{t('settings.privacyNote')}</span>
      </div>
    {:else if activeGroup === 'translation'}
      <h2 class="group-title">{t('settings.groupTranslation')}</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.targetLang')}</span>
          <span class="desc">{t('settings.targetLangDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['defaultTargetLang']}
            <span class="row-error" role="alert">{rowErrors['defaultTargetLang']}</span>
          {:else if savedState['defaultTargetLang']}
            <span
              class="row-saved {savedState['defaultTargetLang'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <select
            class="select"
            aria-label={t('settings.targetLang')}
            value={settings?.translation.defaultTargetLang ?? '英文'}
            onchange={onTargetLangChange}
          >
            {#each DIRECTED_TARGET_LANGS as lang (lang.label)}
              <option value={lang.label}>{targetLangDisplay(lang.label)}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.presentation')}</span>
          <span class="desc">{t('settings.presentationDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['presentation']}
            <span class="row-error" role="alert">{rowErrors['presentation']}</span>
          {:else if savedState['presentation']}
            <span
              class="row-saved {savedState['presentation'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <select
            class="select"
            aria-label={t('settings.presentation')}
            value={settings?.translation.presentation ?? 'stream_all'}
            onchange={onPresentationChange}
          >
            <option value="stream_all">{t('settings.presentationStream')}</option>
            <option value="staged">{t('settings.presentationStaged')}</option>
          </select>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.threshold')}</span>
          <span class="desc">{t('settings.thresholdDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['threshold']}
            <span class="row-error" role="alert">{rowErrors['threshold']}</span>
          {:else if savedState['threshold']}
            <span class="row-saved {savedState['threshold'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <input
            class="num-input"
            type="number"
            min="0"
            step="1"
            disabled={!isStaged}
            aria-label={t('settings.threshold')}
            value={settings?.translation.longTextThresholdChars ?? ''}
            onchange={onThresholdChange}
          />
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.autoTranslate')}</span>
          <span class="desc">{t('settings.autoTranslateDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autoTranslate']}
            <span class="row-error" role="alert">{rowErrors['autoTranslate']}</span>
          {:else if savedState['autoTranslate']}
            <span
              class="row-saved {savedState['autoTranslate'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label={t('settings.autoTranslate')}
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
          <span class="label">{t('settings.autoTranslateDelay')}</span>
          <span class="desc">{t('settings.autoTranslateDelayDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autoTranslateDelay']}
            <span class="row-error" role="alert">{rowErrors['autoTranslateDelay']}</span>
          {:else if savedState['autoTranslateDelay']}
            <span
              class="row-saved {savedState['autoTranslateDelay'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <select
            class="select"
            aria-label={t('settings.autoTranslateDelay')}
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
      <h2 class="group-title">{t('settings.groupPanel')}</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.hideOnBlur')}</span>
          <span class="desc">{t('settings.hideOnBlurDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['hideOnBlur']}
            <span class="row-error" role="alert">{rowErrors['hideOnBlur']}</span>
          {:else if savedState['hideOnBlur']}
            <span class="row-saved {savedState['hideOnBlur'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label={t('settings.hideOnBlur')}
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
          <span class="label">{t('settings.clipboardMonitor')}</span>
          <span class="desc">{t('settings.clipboardMonitorDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['clipboardMonitor']}
            <span class="row-error" role="alert">{rowErrors['clipboardMonitor']}</span>
          {:else if savedState['clipboardMonitor']}
            <span
              class="row-saved {savedState['clipboardMonitor'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label={t('settings.clipboardMonitor')}
            aria-checked={settings?.clipboardMonitor ?? false}
            disabled={!settings}
            onclick={() => onClipboardMonitorChange(!(settings?.clipboardMonitor ?? false))}
          >
            <span class="switch-knob"></span>
          </button>
        </div>
      </div>
    {:else if activeGroup === 'hotkeys'}
      <h2 class="group-title">{t('settings.groupHotkeys')}</h2>
      <p class="group-note">{t('settings.hotkeyDesc')}</p>

      {#each HOTKEY_ROWS as row (row.fn)}
        {@const key = `hotkey-${row.fn}`}
        <div class="row">
          <div class="row-label">
            <span class="label">{t(row.label)}</span>
          </div>
          <div class="row-control">
            {#if rowErrors[key]}
              <span class="row-error" role="alert">{rowErrors[key]}</span>
            {:else if savedState[key]}
              <span class="row-saved {savedState[key].fading ? 'fading' : ''}" role="status"
                >{t('settings.saved')}</span
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
      <h2 class="group-title">{t('settings.groupOcr')}</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.ocrLang')}</span>
          <span class="desc">{t('settings.ocrLangDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['ocrLang']}
            <span class="row-error" role="alert">{rowErrors['ocrLang']}</span>
          {:else if savedState['ocrLang']}
            <span class="row-saved {savedState['ocrLang'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <select
            class="select"
            aria-label={t('settings.ocrLang')}
            value={settings?.ocr.language ?? 'auto'}
            onchange={onOcrLangChange}
          >
            <option value="auto">{t('settings.ocrLangAuto')}</option>
            {#each ocrOptions as lang (lang.tag)}
              <option value={lang.tag}>{lang.label}</option>
            {/each}
          </select>
        </div>
      </div>
    {:else}
      <h2 class="group-title">{t('settings.groupGeneral')}</h2>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.autostart')}</span>
          <span class="desc">{t('settings.autostartDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['autostart']}
            <span class="row-error" role="alert">{rowErrors['autostart']}</span>
          {:else if savedState['autostart']}
            <span class="row-saved {savedState['autostart'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <button
            class="switch"
            type="button"
            role="switch"
            aria-label={t('settings.autostart')}
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
          <span class="label">{t('settings.language')}</span>
          <span class="desc">{t('settings.languageDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['language']}
            <span class="row-error" role="alert">{rowErrors['language']}</span>
          {:else if savedState['language']}
            <span
              class="row-saved {savedState['language'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <select
            class="select"
            aria-label={t('settings.language')}
            value={settings?.general.language ?? 'system'}
            onchange={onLanguageChange}
          >
            <option value="system">{t('settings.langSystem')}</option>
            <option value="zh-CN">{t('settings.langZhHans')}</option>
            <option value="zh-TW">{t('settings.langZhHant')}</option>
            <option value="en">{t('settings.langEn')}</option>
            <option value="ja">{t('settings.langJa')}</option>
            <option value="ko">{t('settings.langKo')}</option>
            <option value="fr">{t('settings.langFr')}</option>
            <option value="de">{t('settings.langDe')}</option>
            <option value="es">{t('settings.langEs')}</option>
            <option value="ru">{t('settings.langRu')}</option>
          </select>
        </div>
      </div>

      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.theme')}</span>
          <span class="desc">{t('settings.themeDesc')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['theme']}
            <span class="row-error" role="alert">{rowErrors['theme']}</span>
          {:else if savedState['theme']}
            <span class="row-saved {savedState['theme'].fading ? 'fading' : ''}" role="status"
              >{t('settings.saved')}</span
            >
          {/if}
          <div class="segmented" role="group" aria-label={t('settings.theme')}>
            {#each THEME_OPTIONS as opt (opt.value)}
              <button
                type="button"
                class="seg-btn"
                aria-pressed={(settings?.general.theme ?? 'system') === opt.value}
                onclick={() => onThemeChange(opt.value)}
              >
                {t(opt.label)}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- 清单 P3-11：恢复默认设置（二次确认；后端保留 Key/引导/语言） -->
      <div class="row">
        <div class="row-label">
          <span class="label">{t('settings.restoreDefaults')}</span>
        </div>
        <div class="row-control">
          {#if rowErrors['restoreDefaults']}
            <span class="row-error" role="alert">{rowErrors['restoreDefaults']}</span>
          {:else if savedState['restoreDefaults']}
            <span
              class="row-saved {savedState['restoreDefaults'].fading ? 'fading' : ''}"
              role="status">{t('settings.saved')}</span
            >
          {/if}
          <button class="btn" type="button" disabled={restoring} onclick={() => void onRestoreDefaults()}>
            {t('settings.restoreDefaults')}
          </button>
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
  .verify-ok {
    font-size: var(--font-size-secondary);
    color: var(--success);
    white-space: nowrap;
  }
  .group-note {
    margin: calc(-1 * var(--space-2)) 0 var(--space-3);
    font-size: var(--font-size-secondary);
    color: var(--fg-muted);
  }
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
