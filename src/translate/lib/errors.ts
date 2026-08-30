// 六类错误卡片文案（design-spec 4.4 表；i18n-1 起文案迁移至 i18n 目录 err.*，
// 本文件保留 ex_code → 卡片动作/键 的映射唯一来源；标题/说明经 t() 实时取语言）。

import { t } from '../../lib/i18n.svelte';

export type ErrorCardAction = 'retry' | 'openSettings';

export interface ErrorCardDefinition {
  /** 标题（加粗）。 */
  title: string;
  /** 说明文字（fg/muted）。 */
  description: string;
  /** 操作按钮文案；"打开设置"同时直达 AI 服务分组（spec 6.1 open_settings）。 */
  actionLabel: string;
  action: ErrorCardAction;
}

/** EX-1/2/3/4/5/14 六类（design-spec 4.4）。ex_code 与 error.rs AppError::ex_code 一致。 */
const ERROR_CARDS: Record<string, { titleKey: string; descKey: string; action: ErrorCardAction }> =
  {
    'EX-1': { titleKey: 'err.ex1Title', descKey: 'err.ex1Desc', action: 'retry' },
    'EX-2': { titleKey: 'err.ex2Title', descKey: 'err.ex2Desc', action: 'openSettings' },
    'EX-3': { titleKey: 'err.ex3Title', descKey: 'err.ex3Desc', action: 'openSettings' },
    'EX-4': { titleKey: 'err.ex4Title', descKey: 'err.ex4Desc', action: 'retry' },
    'EX-5': { titleKey: 'err.ex5Title', descKey: 'err.ex5Desc', action: 'retry' },
    'EX-14': { titleKey: 'err.ex14Title', descKey: 'err.ex14Desc', action: 'retry' },
  };

/** 按 ex_code 取卡片定义（未知编码→通用"翻译失败"卡片，不静默）；
 *  未知编码属契约外防御路径，不冒充六类中的任何一类。 */
export function errorCardFor(exCode: string): ErrorCardDefinition {
  const card = ERROR_CARDS[exCode];
  if (card) {
    return {
      title: t(card.titleKey),
      description: t(card.descKey),
      actionLabel: t(card.action === 'retry' ? 'err.retry' : 'err.openSettings'),
      action: card.action,
    };
  }
  return {
    title: t('err.unknownTitle'),
    description: t('err.unknownDesc'),
    actionLabel: t('err.retry'),
    action: 'retry',
  };
}

// ---------------------------------------------------------------------------
// EX-6：OCR 语言包缺失（形态 C/D 错误横条，design-spec 4.4 附注）
// ---------------------------------------------------------------------------

/** EX-6 横条标题。 */
export function ocrLangpackTitle(): string {
  return t('err.ocrLangpackTitle');
}

/** 官方微软语言包指引（与 ocr/mod.rs MICROSOFT_LANGUAGE_PACK_GUIDE_URL 同源工件的展示面）。 */
export const MICROSOFT_GUIDE_URL =
  'https://support.microsoft.com/zh-cn/topic/%E5%9C%A8-windows-10-%E4%B8%AD%E6%B7%BB%E5%8A%A0%E8%AF%AD%E8%A8%80%E8%AE%BE%E7%BD%AE-ed0cec6b-c21b-407b-91be-6f2e14e5c4cb';

/**
 * EX-6 说明文案（固定语种形态：点名语种 tag）。
 * 自动形态（无可用包）：「未找到可用的 OCR 语言包，请安装后重试。」
 */
export function ocrLangpackDescription(tag: string | null): string {
  return tag ? t('err.ocrLangpackNamed', { tag }) : t('err.ocrLangpackNone');
}
