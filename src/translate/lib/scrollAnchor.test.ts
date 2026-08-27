// 滚动锚定单测（design-spec 4.3：距底 <24px 跟随；上滚停锚；滚回底部恢复）。
import { describe, expect, it } from 'vitest';
import { ANCHOR_THRESHOLD_PX, ScrollAnchor } from './scrollAnchor';

describe('ScrollAnchor 滚动锚定', () => {
  it('距底 <24px → 跟随态；追加后贴底（scrollTop=scrollHeight）', () => {
    const anchor = new ScrollAnchor();
    // scrollHeight 1000, clientHeight 200, scrollTop 780 → 距底 20 <24
    expect(anchor.update(780, 1000, 200)).toBe(true);
    const el = { scrollTop: 780, scrollHeight: 1200, clientHeight: 200 } as HTMLElement;
    anchor.applyAppend(el);
    expect(el.scrollTop).toBe(1200);
  });

  it('上滚超过 24px → 停锚；后续追加不滚动（内容不被强行拖回）', () => {
    const anchor = new ScrollAnchor();
    expect(anchor.update(700, 1000, 200)).toBe(false); // 距底 300
    const el = { scrollTop: 700, scrollHeight: 1200, clientHeight: 200 } as HTMLElement;
    anchor.applyAppend(el);
    expect(el.scrollTop).toBe(700);
  });

  it('滚回底部 → 恢复跟随（距底 <24px 判定）', () => {
    const anchor = new ScrollAnchor();
    anchor.update(700, 1000, 200);
    expect(anchor.isFollowing).toBe(false);
    // 用户滚回底：scrollTop = 1000-200 = 800 → 距底 0
    expect(anchor.update(800, 1000, 200)).toBe(true);
    const el = { scrollTop: 800, scrollHeight: 1400, clientHeight: 200 } as HTMLElement;
    anchor.applyAppend(el);
    expect(el.scrollTop).toBe(1400);
  });

  it('阈值常量为 24px（design-spec 4.3 字面）', () => {
    expect(ANCHOR_THRESHOLD_PX).toBe(24);
  });
});
