import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

// Alya 前端：四个窗口各自独立 HTML 入口（spec 5.2/5.5），共享 src/lib/。
// dev 服务端口 1420 与 tauri.conf.json devUrl 一致（Tauri 模板约定）。
// 测试：vitest（T12 起）运行 src/** 组件/逻辑单测（jsdom 环境；ipc/Tauri 依赖由测试 mock）。
export default defineConfig({
  clearScreen: false,
  // vitest（jsdom 环境）默认缺少 browser 条件下 svelte 包会解析到 index-server（SSR 产物），
  // 导致 mount() unavailable；browser 条件追加不影响 dev/build（客户端构建本就含该条件）。
  resolve: {
    conditions: ['browser'],
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 不监听 src-tauri/，避免 Rust 输出文件触发前端热更新
      ignored: ['**/src-tauri/**'],
    },
  },
  appType: 'mpa',
  build: {
    outDir: 'dist',
    rollupOptions: {
      input: {
        translate: 'src/translate/index.html',
        mask: 'src/mask/index.html',
        settings: 'src/settings/index.html',
        onboarding: 'src/onboarding/index.html',
      },
    },
  },
  plugins: [svelte()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    testTimeout: 15000,
  },
});
