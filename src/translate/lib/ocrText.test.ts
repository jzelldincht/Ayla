// OCR 文本规整单测（清单 P2-8）：断词合并/行间合并/下划线归一/保守性。
import { describe, expect, it } from 'vitest';

import { normalizeOcrText } from './ocrText';

describe('normalizeOcrText', () => {
  it('行尾连字符断词合并（win-\\ndow → window）', () => {
    expect(normalizeOcrText('This is Win-\ndows 11 setup.')).toBe('This is Windows 11 setup.');
  });

  it('行间合并：拉丁相邻单空格', () => {
    expect(normalizeOcrText('Hello world\nfoo bar.')).toBe('Hello world foo bar.');
  });

  it('行间合并：CJK 相邻无空格', () => {
    expect(normalizeOcrText('这是一段被拆成\n两行的中文。')).toBe('这是一段被拆成两行的中文。');
  });

  it('句末标点后不合并（保留换行结构）', () => {
    expect(normalizeOcrText('第一句结束。\n第二句开始。')).toBe('第一句结束。\n第二句开始。');
  });

  it('空行为段落边界：跨段不合并', () => {
    expect(normalizeOcrText('第一段\n\n第二段')).toBe('第一段\n\n第二段');
  });

  it('CJK 上下文的 `_` 视为破折号误识 → ——', () => {
    expect(normalizeOcrText('参数一_参数二')).toBe('参数一——参数二');
  });

  it('拉丁上下文的 `_` 保留（保守：可能是合法标识符）', () => {
    expect(normalizeOcrText('snake_case_value')).toBe('snake_case_value');
  });

  it('CRLF 归一并合并相邻中文行（清单 P2-8 行为）', () => {
    expect(normalizeOcrText('中文第一行\r\n中文第二行')).toBe('中文第一行中文第二行');
  });

  it('空串原样返回', () => {
    expect(normalizeOcrText('')).toBe('');
  });
});
