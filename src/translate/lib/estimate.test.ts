// 预估剩余时间单测（FR-10.3/Q3：近 2 秒速率外推；"预估"冠词；不足速率仅"正在翻译…"）。
import { describe, expect, it } from 'vitest';
import {
  computeEtaSeconds,
  formatEtaLabel,
  ETA_MIN_PRODUCED,
  ETA_MIN_RATE_CHARS_PER_SEC,
} from './estimate';

describe('computeEtaSeconds（整段模式徽标）', () => {
  it('按（总量−已生成）/速率 外推并向上取整', () => {
    // 源 300 字符，已生成 180，速率 30 字符/秒 → 剩余 120 → 4 秒
    expect(computeEtaSeconds({ sourceLength: 300, produced: 180, ratePerSecond: 30 })).toBe(4);
  });

  it('已生成已超源长（译文更长）→ 以已生成为总量，剩余 0 → 0 秒（即将完成）', () => {
    expect(computeEtaSeconds({ sourceLength: 100, produced: 150, ratePerSecond: 50 })).toBe(0);
  });

  it('已生成字符过少（<16）不足以估算 → null（仅"正在翻译…"）', () => {
    expect(computeEtaSeconds({ sourceLength: 1000, produced: ETA_MIN_PRODUCED - 1, ratePerSecond: 40 })).toBeNull();
  });

  it('速率不足（<1 字符/秒）→ null', () => {
    expect(
      computeEtaSeconds({ sourceLength: 1000, produced: 500, ratePerSecond: ETA_MIN_RATE_CHARS_PER_SEC - 0.5 }),
    ).toBeNull();
  });

  it('零速率且样本充足 → null（不产出 0/∞ 误导数值）', () => {
    expect(computeEtaSeconds({ sourceLength: 1000, produced: 500, ratePerSecond: 0 })).toBeNull();
  });

  it('文案固定格式「预估剩余约 N 秒」', () => {
    expect(formatEtaLabel(5)).toBe('预估剩余约 5 秒');
  });
});
