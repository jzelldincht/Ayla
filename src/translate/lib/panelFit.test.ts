// panelFit 单测（真机修复轮 13）：内容自适应高度测量纯函数。
import { describe, expect, it } from 'vitest';

import { computeFitViewportHeight } from './panelFit';

describe('computeFitViewportHeight', () => {
  it('内容溢出：所需窗口高 = 视口 − 可视 + 内容自然高', () => {
    expect(
      computeFitViewportHeight({ viewportHeight: 340, clientHeight: 200, contentHeight: 900 }),
    ).toBe(1040);
  });

  it('内容未溢出（clientHeight == scrollHeight 态）：所需 = 内容自然高 + 其余 chrome 高', () => {
    // 视口 340、译文可视 200 → chrome 占 140；内容自然 80 → 只需 220（收缩回偏好值的依据）。
    expect(
      computeFitViewportHeight({ viewportHeight: 340, clientHeight: 200, contentHeight: 80 }),
    ).toBe(220);
  });

  it('可视与内容相等（临界）：所需 = 当前视口高（不变更）', () => {
    expect(
      computeFitViewportHeight({ viewportHeight: 340, clientHeight: 200, contentHeight: 200 }),
    ).toBe(340);
  });
});
