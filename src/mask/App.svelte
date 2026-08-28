<script lang="ts">
  // 截图遮罩窗页面（label=mask-{i}，T9 实现；design-spec 第 5 章 + spec 5.7.4）。
  //
  // 数据流（INV-3 例外经主 Agent 批准的"方案二"，见 implementation-report T9 节）：
  //   后端会话创建后经 mask://begin 广播 `{sessionId, monitorIndex}`（无位图）；
  //   本页按 URL 查询参数 `monitorIndex`（Rust 建窗时写入，非 IPC）invoke `get_mask_snapshot`
  //   拿到本屏冻结帧的 data: URL（内存字符串，无磁盘/无 HTTP），`<img>` 解码后绘制到 canvas
  //   一次即弃——页面内不持久化任何位图对象之外的数据。
  // 交互：canvas 铺满窗；拖拽画选区（选区内透亮=快照原像素，选区外 rgba(15,18,24,0.35) 暗层）；
  //   拖拽中显示「宽 × 高」物理 px 尺寸标签（12px 等宽、白字、黑底胶囊）；十字光标全屏；
  //   Esc → invoke cancel_selection（150ms 淡出后）；松开 → 无效（Q11：位移<4px 或面积<64px²）
  //   视同取消，有效 → invoke complete_selection（100ms 淡出后）。坐标换算：
  //   物理 = 逻辑 × (canvas 背板 / window.innerWidth)——遮罩窗物理尺寸 == 显示器物理尺寸，
  //   canvas 背板按 devicePixelRatio 铺满，即每窗独立 DPI 上下文（混 DPI 各窗自洽）。

  import { onMount } from 'svelte';
  import { completeSelection, cancelSelection, getMaskSnapshot, onMaskBegin } from '../lib/ipc';

  const monitorIndex = Number(new URLSearchParams(window.location.search).get('monitorIndex') ?? '0');

  // design-spec 2.1 / 第 5 章：遮罩与主题无关，恒定（同一屏遮罩页写死）
  const MASK_DIM = 'rgba(15,18,24,0.35)';
  const ACCENT = '#2E63DC';
  const INNER_LINING = 'rgba(255,255,255,0.55)';
  // Q11 无效判定（物理 px；前端判一次，后端 capture::is_valid_selection 复验兜底）
  const MIN_DRAG_PX = 4;
  const MIN_AREA_PX2 = 64;
  const RETRY_LIMIT = 200; // 挂载兜底重试上限（~20s；事件驱动后即刻成功）

  let canvasEl: HTMLCanvasElement;
  let hostEl: HTMLDivElement;
  let ctx: CanvasRenderingContext2D | null = null;

  let snapshot: HTMLImageElement | null = null;
  let sessionId: string | null = null;
  let retries = 0;
  /** 本会话已提交/已取消（防 Esc 与松开竞态双触发） */
  let finished = false;

  type PxRect = { x: number; y: number; width: number; height: number };
  let sel: PxRect | null = null;
  let dragStart: { x: number; y: number } | null = null;
  let dragging = false;

  // -------------------------------------------------------------------------
  // 画布与绘制
  // -------------------------------------------------------------------------

  function resizeCanvas() {
    if (!canvasEl) return;
    const dpr = window.devicePixelRatio || 1;
    canvasEl.width = Math.max(1, Math.round(window.innerWidth * dpr));
    canvasEl.height = Math.max(1, Math.round(window.innerHeight * dpr));
    draw();
  }

  /** CSS px → 该窗物理 px 比例（window 物理尺寸 == 显示器物理尺寸；canvas 背板即物理像素） */
  function pxScale(): { sx: number; sy: number } {
    return {
      sx: canvasEl.width / Math.max(1, window.innerWidth),
      sy: canvasEl.height / Math.max(1, window.innerHeight),
    };
  }

  function draw() {
    if (!ctx) return;
    const w = window.innerWidth;
    const h = window.innerHeight;
    const { sx, sy } = pxScale();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
    // 逻辑坐标绘制（变换到物理背板，描边/字体随 dpr 放大——与设计逻辑 px 口径一致）
    ctx.setTransform(sx, 0, 0, sy, 0, 0);

    // 冻结快照底图（D4）
    if (snapshot) {
      ctx.drawImage(snapshot, 0, 0, w, h);
    }
    // 暗色遮罩层：选区矩形以 even-odd 挖空（选区内完全透亮=快照原像素）
    ctx.beginPath();
    ctx.rect(0, 0, w, h);
    if (sel) {
      ctx.rect(sel.x, sel.y, sel.width, sel.height);
    }
    ctx.fillStyle = MASK_DIM;
    ctx.fill(sel ? 'evenodd' : 'nonzero');

    // 选区边框：2px accent 外框 + 1px 白内衬（深浅背景下都可见）
    if (sel && (sel.width > 0 || sel.height > 0)) {
      ctx.strokeStyle = ACCENT;
      ctx.lineWidth = 2;
      ctx.strokeRect(sel.x, sel.y, sel.width, sel.height);
      ctx.strokeStyle = INNER_LINING;
      ctx.lineWidth = 1;
      ctx.strokeRect(sel.x + 2, sel.y + 2, Math.max(0, sel.width - 4), Math.max(0, sel.height - 4));
      drawSizeLabel(sel);
    }
  }

  /** 尺寸标签：「宽 × 高」物理 px；12px 等宽、白字、rgba(0,0,0,0.72) 胶囊底；选区上方 8px 居中，
   *  空间不足改下方（design-spec 第 5 章）。 */
  function drawSizeLabel(rect: PxRect) {
    if (!ctx) return;
    const { sx, sy } = pxScale();
    const pw = Math.round(rect.width * sx);
    const ph = Math.round(rect.height * sy);
    const label = `${pw} × ${ph}`;
    ctx.font = '12px "Consolas", monospace';
    const tw = ctx.measureText(label).width;
    const lw = tw + 16;
    const lh = 22;
    let lx = rect.x + rect.width / 2 - lw / 2;
    let ly = rect.y - lh - 8;
    const topSpace = rect.y;
    const bottomSpace = window.innerHeight - (rect.y + rect.height);
    if (topSpace < lh + 8 && bottomSpace > topSpace) {
      ly = rect.y + rect.height + 8; // 上方不足且下方有余 → 改下方
    }
    const lxClamped = Math.min(Math.max(4, lx), window.innerWidth - lw - 4);
    const lyClamped = Math.min(Math.max(4, ly), window.innerHeight - lh - 4);
    ctx.fillStyle = 'rgba(0,0,0,0.72)';
    ctx.beginPath();
    ctx.roundRect(lxClamped, lyClamped, lw, lh, 11);
    ctx.fill();
    ctx.fillStyle = '#FFFFFF';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, lxClamped + 8, lyClamped + lh / 2 + 0.5);
  }

  // -------------------------------------------------------------------------
  // 帧拉取（mask://begin 驱动；首次创建页挂载时重试兜底——规避建窗→发射的时序竞态）
  // -------------------------------------------------------------------------

  async function fetchFrame(): Promise<void> {
    if (finished) return;
    try {
      const res = await getMaskSnapshot({ monitorIndex });
      sessionId = res.sessionId;
      const img = new Image();
      img.onload = () => {
        snapshot = img;
        draw();
        show(100); // 遮罩淡入 100ms（design-spec 第 5 章）
      };
      img.onerror = () => {
        scheduleRetry();
      };
      img.src = res.dataUrl;
    } catch {
      scheduleRetry();
    }
  }

  function scheduleRetry() {
    if (finished) return;
    retries += 1;
    if (retries > RETRY_LIMIT) return;
    window.setTimeout(() => {
      void fetchFrame();
    }, 100);
  }

  function beginSession(nextSessionId: string) {
    // 新会话（含池化复用）：重置交互态并重新拉帧，杜绝旧帧残留；先落位透明再淡入
    finished = false;
    retries = 0;
    sel = null;
    dragStart = null;
    dragging = false;
    sessionId = nextSessionId;
    snapshot = null;
    if (hostEl) {
      hostEl.style.opacity = '0';
    }
    void fetchFrame();
  }

  // -------------------------------------------------------------------------
  // 交互：拖拽框选 + Esc 取消
  // -------------------------------------------------------------------------

  function clampRect(a: { x: number; y: number }, b: { x: number; y: number }): PxRect {
    const w = window.innerWidth;
    const h = window.innerHeight;
    const x1 = Math.max(0, Math.min(a.x, b.x));
    const x2 = Math.min(w, Math.max(a.x, b.x));
    const y1 = Math.max(0, Math.min(a.y, b.y));
    const y2 = Math.min(h, Math.max(a.y, b.y));
    return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
  }

  function onPointerDown(event: PointerEvent) {
    if (finished || event.button !== 0) return;
    dragging = true;
    dragStart = { x: event.clientX, y: event.clientY };
    sel = { x: event.clientX, y: event.clientY, width: 0, height: 0 };
    canvasEl.setPointerCapture(event.pointerId);
    draw();
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging || !dragStart) return;
    // 跨屏拖拽：选区钳制在本窗（=本显示器）边界——FR-8.4 单屏内完成框选
    sel = clampRect(dragStart, { x: event.clientX, y: event.clientY });
    draw();
  }

  function onPointerUp() {
    if (!dragging) return;
    dragging = false;
    if (finished || !sel || !sessionId) return;
    const { sx, sy } = pxScale();
    const pw = Math.round(sel.width * sx);
    const ph = Math.round(sel.height * sy);
    const px = Math.round(sel.x * sx);
    const py = Math.round(sel.y * sy);
    // Q11：位移 <4 物理px 或 面积 <64 物理px² → 视同取消
    const displacementSq = pw * pw + ph * ph;
    const area = pw * ph;
    if (displacementSq < MIN_DRAG_PX * MIN_DRAG_PX || area < MIN_AREA_PX2) {
      cancelWithFade();
      return;
    }
    commitWithFade({
      sessionId,
      monitorIndex,
      rect: { x: px, y: py, width: pw, height: ph },
    });
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      cancelWithFade();
    }
  }

  function cancelWithFade() {
    if (finished) return;
    finished = true;
    // 遮罩 150ms 淡出后屏幕完全复原（design-spec 第 5 章；FR-8.3 无任何残留）
    show(150, () => {
      const id = sessionId;
      if (id) {
        cancelSelection({ sessionId: id }).catch(() => {
          // 取消失败/过期：后端幂等（id 不匹配即无操作），前端不再动作
        });
      }
    });
  }

  function commitWithFade(command: {
    sessionId: string;
    monitorIndex: number;
    rect: { x: number; y: number; width: number; height: number };
  }) {
    if (finished) return;
    finished = true;
    // 确认后遮罩 100ms 淡出（design-spec 第 5 章；FR-8.2）
    show(100, () => {
      completeSelection(command).catch(() => {
        // 命令错误已在后端回收遮罩并保持"无残留"不变量（会话裁剪失败/过期）；这里避免重复动作
      });
    });
  }

  /** 宿主透明度过渡（淡入 100ms / 淡出 150ms），完成后回调 */
  function show(ms: number, onDone?: () => void) {
    if (!hostEl) return;
    hostEl.style.transition = `opacity ${ms}ms`;
    hostEl.style.opacity = ms > 0 && onDone === undefined ? '1' : '0';
    if (onDone) {
      window.setTimeout(onDone, ms);
    }
  }

  // -------------------------------------------------------------------------
  // 生命周期
  // -------------------------------------------------------------------------

  onMount(() => {
    ctx = canvasEl.getContext('2d');
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);
    window.addEventListener('keydown', onKeyDown);
    canvasEl.addEventListener('pointerdown', onPointerDown);
    canvasEl.addEventListener('pointermove', onPointerMove);
    canvasEl.addEventListener('pointerup', onPointerUp);
    canvasEl.addEventListener('pointercancel', onPointerUp);
    // 页已挂载即存在（池化窗常驻）；首建窗与事件发射的竞态由 fetchFrame 重试兜底
    void onMaskBegin((payload) => {
      beginSession(payload.sessionId);
    }).then(() => {
      void fetchFrame();
    });
  });
</script>

<svelte:head>
  <title>截图遮罩</title>
</svelte:head>

<div class="host" bind:this={hostEl}>
  <canvas bind:this={canvasEl}></canvas>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: transparent;
  }
  .host {
    position: fixed;
    inset: 0;
    opacity: 0; /* 帧到达后淡入（100ms） */
    pointer-events: none; /* 仅透明容器：命中测试穿透到 canvas（否则遮住拖拽指针事件） */
  }
  canvas {
    display: block;
    width: 100vw;
    height: 100vh;
    cursor: crosshair; /* 全程十字准星（design-spec 第 5 章） */
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    /* 真机修复轮 6：.host 的 pointer-events:none 会被子元素继承，canvas 必须显式恢复
       auto，否则遮罩页收不到任何指针事件——框选完全失效（用户真机实测复现）。 */
    pointer-events: auto;
  }
</style>
