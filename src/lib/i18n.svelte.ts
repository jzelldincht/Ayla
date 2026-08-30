// 多语言内核（i18n-1）：轻量 t() + 语言解析，无任何运行时依赖。
//
// - Locale：'zh-CN'（源头语言，目录为键结构权威）| 'en'（类型约束 typeof zh，缺键编译期报错）；
// - LanguageSetting（config general.language）：'system' | 'zh-CN' | 'en'；
//   'system' 按 navigator.language 探测（zh* → 中文，其余 → 英文），探测结果进程内缓存；
// - t(path, params?)：点路径查目录（en → zh 兜底 → 路径原文），{name} 插值；
//   模板中调用即建立响应依赖——设置页切换语言全窗实时生效（INV-8 镜像回流驱动）；
// - 测试：vitest 全局 setup 经 setTestLocale('zh-CN') 锁定中文（jsdom navigator 为 en-US）。

import { settingsState } from './stores';
import type { LanguageSetting, Locale } from './types';
import { zh } from './i18n/zh-CN';
import { en } from './i18n/en';
import { zhTw } from './i18n/zh-TW';
import { ja } from './i18n/ja';
import { ko } from './i18n/ko';
import { fr } from './i18n/fr';
import { de } from './i18n/de';
import { es } from './i18n/es';
import { ru } from './i18n/ru';

type Catalog = typeof zh;

/** 九语种目录注册表（i18n-2：与定向目标语言清单一致）。 */
const CATALOGS: Record<Locale, Catalog> = {
  'zh-CN': zh,
  'zh-TW': zhTw,
  en,
  ja,
  ko,
  fr,
  de,
  es,
  ru,
};

let systemLocaleCache: Locale | null = null;

/** 测试锁定标记（vitest setup 经 globalThis 注入，不做任何模块 import——见 test-setup.ts）。 */
function testLocaleOverride(): Locale | null {
  const flag = (globalThis as Record<string, unknown>).__ALYA_TEST_LOCALE__;
  return flag === 'zh-CN' || flag === 'en' ? flag : null;
}

function detectSystemLocale(): Locale {
  if (systemLocaleCache) return systemLocaleCache;
  const nav = (typeof navigator === 'undefined' ? 'en' : (navigator.language ?? 'en')).toLowerCase();
  let locale: Locale = 'en';
  if (nav.startsWith('zh')) {
    locale = /(tw|hk|mo|hant)/.test(nav) ? 'zh-TW' : 'zh-CN';
  } else if (nav.startsWith('ja')) locale = 'ja';
  else if (nav.startsWith('ko')) locale = 'ko';
  else if (nav.startsWith('fr')) locale = 'fr';
  else if (nav.startsWith('de')) locale = 'de';
  else if (nav.startsWith('es')) locale = 'es';
  else if (nav.startsWith('ru')) locale = 'ru';
  systemLocaleCache = locale;
  return systemLocaleCache;
}

export function currentLocale(): Locale {
  const override = testLocaleOverride();
  if (override) return override;
  const setting: LanguageSetting = settingsState.current?.general.language ?? 'system';
  return setting === 'system' ? detectSystemLocale() : setting;
}

function lookup(dict: Catalog, path: string): string | undefined {
  let node: unknown = dict;
  for (const segment of path.split('.')) {
    if (typeof node !== 'object' || node === null) return undefined;
    node = (node as Record<string, unknown>)[segment];
  }
  return typeof node === 'string' ? node : undefined;
}

export function t(path: string, params?: Record<string, string | number>): string {
  const locale = currentLocale();
  const raw = lookup(CATALOGS[locale], path) ?? lookup(zh, path) ?? path;
  if (!params) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, key: string) =>
    params[key] === undefined ? `{${key}}` : String(params[key]),
  );
}
