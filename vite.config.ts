import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// Alya 前端：四个窗口各自独立 HTML 入口（spec 5.2/5.5），共享 src/lib/。
// dev 服务端口 1420 与 tauri.conf.json devUrl 一致（Tauri 模板约定）。
export default defineConfig({
  clearScreen: false,
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
});
