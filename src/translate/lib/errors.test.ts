// 六类错误卡片文案单测（design-spec 4.4 表逐字核对：标题/说明/操作按钮逐条不得变异）。
import { describe, expect, it } from 'vitest';
import { ERROR_CARDS, errorCardFor, ocrLangpackDescription, OCR_LANGPACK_TITLE } from './errors';

describe('六类错误卡片（design-spec 4.4 逐字）', () => {
  const expected: Array<[string, string, string, string, string]> = [
    // [ex, title, description, actionLabel, action]
    ['EX-1', '网络连接失败', '无法连接到翻译服务，请检查网络后重试。', '重试', 'retry'],
    ['EX-2', '鉴权失败，请检查 API Key', 'API Key 无效或已被撤销。', '打开设置', 'openSettings'],
    ['EX-3', '余额不足', '账户余额不足，请前往服务商控制台处理后重试。', '打开设置', 'openSettings'],
    ['EX-4', '请求超时', '服务 30 秒内未响应。', '重试', 'retry'],
    ['EX-5', '传输中断', '连接中断，已保留部分译文。', '重试', 'retry'],
    ['EX-14', '翻译服务返回异常', '服务端返回了无效响应，请稍后重试。', '重试', 'retry'],
  ];

  it.each(expected)('%s 文案与按钮逐字一致', (ex, title, desc, actionLabel, action) => {
    const card = ERROR_CARDS[ex];
    expect(card).toBeDefined();
    expect(card.title).toBe(title);
    expect(card.description).toBe(desc);
    expect(card.actionLabel).toBe(actionLabel);
    expect(card.action).toBe(action);
  });

  it('EX-2/EX-3 打开设置；EX-1/4/5/14 重试（spec 5.8 呈现层口径）', () => {
    expect(ERROR_CARDS['EX-2'].action).toBe('openSettings');
    expect(ERROR_CARDS['EX-3'].action).toBe('openSettings');
    for (const ex of ['EX-1', 'EX-4', 'EX-5', 'EX-14']) {
      expect(ERROR_CARDS[ex].action).toBe('retry');
    }
  });

  it('未知编码回落通用卡片（不静默、不冒充六类）', () => {
    const card = errorCardFor('internal');
    expect(card.title).toBe('翻译失败');
    expect(card.action).toBe('retry');
  });

  it('EX-6 横条文案（4.4 附注：固定语种点名 tag；自动形态通用提示）', () => {
    expect(OCR_LANGPACK_TITLE).toBe('缺少 OCR 语言包');
    expect(ocrLangpackDescription('zh-CN')).toBe('未安装“zh-CN”语言包，无法识别该语种。');
    expect(ocrLangpackDescription(null)).toBe('未找到可用的 OCR 语言包，请安装后重试。');
  });
});
