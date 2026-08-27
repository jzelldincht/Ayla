// 重译按钮浮现逻辑单测（用户裁决 A1：内容被修改后浮现「重译」；FR-4.3 空白不发起）。
import { describe, expect, it } from 'vitest';
import { retranslateVisible } from './retranslate';

describe('retranslateVisible（A/D 修改后浮现）', () => {
  it('内容 ≠ 已提交版本 → 浮现', () => {
    expect(retranslateVisible('原文', '原文修改')).toBe(true);
  });

  it('未修改（等于已提交版）→ 不浮现', () => {
    expect(retranslateVisible('敏捷的狐狸', '敏捷的狐狸')).toBe(false);
  });

  it('修改后为纯空白（清空/空格）→ 不浮现（FR-4.3 空提交拦截）', () => {
    expect(retranslateVisible('原文', '')).toBe(false);
    expect(retranslateVisible('原文', '  \n  ')).toBe(false);
  });

  it('直接输入（无已提交版）且非空 → 浮现', () => {
    expect(retranslateVisible('', '新文本')).toBe(true);
    expect(retranslateVisible('', '')).toBe(false);
  });
});
