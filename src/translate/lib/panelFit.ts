// 译文内容自适应高度测量（真机修复轮 13：译文区随内容自动展开）。
//
// 分工：前端只负责测量——"译文不滚动所需的窗口总高"（CSS 逻辑像素）；上限推导与应用在后端
// （panel_resize_for_content：max(用户偏好, 内容所需) 钳到工作区底 − 窗顶 − 8px）。
// 本模块保持纯函数便于单测；DOM 读取在 App.svelte 的 applyPanelFit。

export interface PanelFitMeasure {
  /** 当前视口总高（window.innerHeight，含顶栏/原文区等常驻 chrome）。 */
  viewportHeight: number;
  /** 译文滚动容器可视高（clientHeight）。 */
  clientHeight: number;
  /** 译文内容自然高（各子元素高之和 + 容器上下 padding；未滚动也成立，可据此收缩回偏好值）。 */
  contentHeight: number;
}

/** 译文不滚动所需的窗口总高＝视口总高 − 译文可视高 + 译文内容自然高。 */
export function computeFitViewportHeight(m: PanelFitMeasure): number {
  return m.viewportHeight - m.clientHeight + m.contentHeight;
}

/** 读取容器内容自然高：子元素高之和 + 上下 padding（滚动态下 scrollHeight 只会报可视高，
 *  故逐子元素求和——chunk-host / 光标条 / 错误卡片等分支子元素互斥或并列均可覆盖）。 */
export function measureContentHeight(el: HTMLElement): number {
  const style = window.getComputedStyle(el);
  let total = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  for (const child of Array.from(el.children)) {
    total += child.getBoundingClientRect().height;
  }
  return total;
}
