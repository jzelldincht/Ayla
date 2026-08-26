// Alya 占位图标生成脚本（原创线性风格「A」，C4 裁决自制占位资产，禁止取用 pot 资产）。
//
// 用途：生成 src 用的 1024×1024 PNG（`pnpm tauri icon` 的输入源）与
//      16px/32px 缩略自检图（16px 剪影可辨性检查）。
//
// 实现：纯 Node（zlib + 手写 PNG 编码），无第三方依赖；颜色取 design-spec 2.1
//      accent/primary 亮色值 #2E63DC（饱和钴蓝，亮色任务栏偏浅底上可读、暗色任务栏上可读）。
//
// 用法：
//   node scripts/generate-placeholder-icon.mjs
//   pnpm tauri icon icons-src/icon-1024.png

import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const SIZE = 1024;
const COLOR = [0x2e, 0x63, 0xdc]; // design-spec accent/primary
const OUT_DIR = path.resolve(process.cwd(), 'icons-src');

// ---------------------------------------------------------------------------
// PNG 编码（最小实现：RGBA、8bit、无交错、每行 filter 0）
// ---------------------------------------------------------------------------

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const head = Buffer.alloc(4);
  head.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([head, body, crc]);
}

function encodePng(width, height, rgba) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type: RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    sig,
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------------------------------------------------------------------------
// 几何：线性「A」= 两腿（圆头线段）+ 横梁，粗描边，抗锯齿（2×2 超采样）
// ---------------------------------------------------------------------------

const LEG_WIDTH = 150; // 描边粗（1024 基准的 ~14.6%，16px 下约 2.3px，16px 剪影可辨）
const APEX = [512, 130];
const LEFT_FOOT = [210, 880];
const RIGHT_FOOT = [814, 880];
const CROSSBAR_Y = 700;
const CROSSBAR_LEFT = 290;
const CROSSBAR_RIGHT = 734;

function distToSegment(px, py, ax, ay, bx, by) {
  const abx = bx - ax;
  const aby = by - ay;
  const apx = px - ax;
  const apy = py - ay;
  const len2 = abx * abx + aby * aby;
  let t = (apx * abx + apy * aby) / len2;
  t = Math.max(0, Math.min(1, t));
  const dx = px - (ax + t * abx);
  const dy = py - (ay + t * aby);
  return Math.hypot(dx, dy);
}

function coverageAt(px, py) {
  const half = LEG_WIDTH / 2;
  // d：到描边中心线的距离；形内（d < half）alpha≈1，形内 0.5px ~ 形外 0.5px 之间做 1px 软边抗锯齿。
  const d = Math.min(
    distToSegment(px, py, APEX[0], APEX[1], LEFT_FOOT[0], LEFT_FOOT[1]),
    distToSegment(px, py, APEX[0], APEX[1], RIGHT_FOOT[0], RIGHT_FOOT[1]),
    distToSegment(px, py, CROSSBAR_LEFT, CROSSBAR_Y, CROSSBAR_RIGHT, CROSSBAR_Y),
  );
  return Math.max(0, Math.min(1, half + 0.5 - d));
}

function renderIcon(size) {
  const rgba = Buffer.alloc(size * size * 4);
  const scale = size / SIZE;
  const halfUv = 0.5 / scale; // 超采样半步长（按源坐标 0.5px）
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const cx = (x + 0.5) * scale;
      const cy = (y + 0.5) * scale;
      // 2×2 超采样平均
      const c =
        coverageAt(cx - halfUv, cy - halfUv) +
        coverageAt(cx + halfUv, cy - halfUv) +
        coverageAt(cx - halfUv, cy + halfUv) +
        coverageAt(cx + halfUv, cy + halfUv);
      const alpha = (c / 4) * 255;
      const o = (y * size + x) * 4;
      rgba[o] = COLOR[0];
      rgba[o + 1] = COLOR[1];
      rgba[o + 2] = COLOR[2];
      rgba[o + 3] = Math.round(alpha);
    }
  }
  return rgba;
}

function downscaleBox(src, srcSize, targetSize) {
  const out = Buffer.alloc(targetSize * targetSize * 4);
  const block = srcSize / targetSize;
  for (let y = 0; y < targetSize; y += 1) {
    for (let x = 0; x < targetSize; x += 1) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      let n = 0;
      const y0 = Math.floor(y * block);
      const x0 = Math.floor(x * block);
      const step = Math.max(1, Math.floor(block / 8)); // 抽样子采样（block 大时）
      for (let sy = y0; sy < y0 + block; sy += step) {
        for (let sx = x0; sx < x0 + block; sx += step) {
          const o = (sy * srcSize + sx) * 4;
          r += src[o];
          g += src[o + 1];
          b += src[o + 2];
          a += src[o + 3];
          n += 1;
        }
      }
      const o = (y * targetSize + x) * 4;
      out[o] = Math.round(r / n);
      out[o + 1] = Math.round(g / n);
      out[o + 2] = Math.round(b / n);
      out[o + 3] = Math.round(a / n);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------

fs.mkdirSync(OUT_DIR, { recursive: true });
const full = renderIcon(SIZE);
fs.writeFileSync(path.join(OUT_DIR, 'icon-1024.png'), encodePng(SIZE, SIZE, full));

for (const size of [16, 32]) {
  const small = downscaleBox(full, SIZE, size);
  fs.writeFileSync(
    path.join(OUT_DIR, `icon-${size}-check.png`),
    encodePng(size, size, small),
  );
  console.log(
    `written icon-${size}-check.png (alpha center=${
      small[(Math.floor(size / 2) * size + Math.floor(size / 2)) * 4 + 3]
    })`,
  );
}
console.log('written icon-1024.png');
