// 六类错误卡片文案单一来源（design-spec 4.4 表逐条照录；EX-6 为形态 C/D 错误横条）。
// 标题/说明/按钮文案与设计规范逐字一致——前端不得另存变体。

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
export const ERROR_CARDS: Record<string, ErrorCardDefinition> = {
  'EX-1': {
    title: '网络连接失败',
    description: '无法连接到翻译服务，请检查网络后重试。',
    actionLabel: '重试',
    action: 'retry',
  },
  'EX-2': {
    title: '鉴权失败，请检查 API Key',
    description: 'API Key 无效或已被撤销。',
    actionLabel: '打开设置',
    action: 'openSettings',
  },
  'EX-3': {
    title: '余额不足',
    description: '账户余额不足，请前往服务商控制台处理后重试。',
    actionLabel: '打开设置',
    action: 'openSettings',
  },
  'EX-4': {
    title: '请求超时',
    description: '服务 30 秒内未响应。',
    actionLabel: '重试',
    action: 'retry',
  },
  'EX-5': {
    title: '传输中断',
    description: '连接中断，已保留部分译文。',
    actionLabel: '重试',
    action: 'retry',
  },
  'EX-14': {
    title: '翻译服务返回异常',
    description: '服务端返回了无效响应，请稍后重试。',
    actionLabel: '重试',
    action: 'retry',
  },
};

/** 按 ex_code 取卡片定义（未知编码→通用"翻译失败"卡片，不静默）；
 *  未知编码属契约外防御路径，不冒充六类中的任何一类。 */
export function errorCardFor(exCode: string): ErrorCardDefinition {
  return (
    ERROR_CARDS[exCode] ?? {
      title: '翻译失败',
      description: '发生未知错误，请稍后重试。',
      actionLabel: '重试',
      action: 'retry',
    }
  );
}

// ---------------------------------------------------------------------------
// EX-6：OCR 语言包缺失（形态 C/D 错误横条，design-spec 4.4 附注）
// ---------------------------------------------------------------------------

/** EX-6 横条标题。 */
export const OCR_LANGPACK_TITLE = '缺少 OCR 语言包';
/** 官方微软语言包指引（与 ocr/mod.rs MICROSOFT_LANGUAGE_PACK_GUIDE_URL 同源工件的展示面）。
 *  链接文本与设计规范一致。 */
export const MICROSOFT_GUIDE_URL =
  'https://support.microsoft.com/zh-cn/topic/%E5%9C%A8-windows-10-%E4%B8%AD%E6%B7%BB%E5%8A%A0%E8%AF%AD%E8%A8%80%E8%AE%BE%E7%BD%AE-ed0cec6b-c21b-407b-91be-6f2e14e5c4cb';

/**
 * EX-6 说明文案（固定语种形态：点名语种 tag）。
 * 自动形态（无可用包）：「未找到可用的 OCR 语言包，请安装后重试。」
 */
export function ocrLangpackDescription(tag: string | null): string {
  if (tag) {
    return `未安装“${tag}”语言包，无法识别该语种。`;
  }
  return '未找到可用的 OCR 语言包，请安装后重试。';
}
