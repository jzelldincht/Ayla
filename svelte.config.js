import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Svelte 5（runes）+ TypeScript 预处理（spec 5.2 决策）。
export default {
  preprocess: vitePreprocess(),
};
