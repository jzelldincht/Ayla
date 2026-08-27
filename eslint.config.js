import js from '@eslint/js';
import prettierConfig from 'eslint-config-prettier';
import eslintPluginSvelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

export default ts.config(
  {
    ignores: ['**/node_modules/**', '**/dist/**', '**/src-tauri/**', '**/gen/**'],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...eslintPluginSvelte.configs['flat/recommended'],
  prettierConfig,
  {
    files: ['**/*.svelte', '**/*.svelte.js', '**/*.svelte.ts'],
    languageOptions: {
      // TS 解析：Svelte 组件经 typescript-eslint 解析器处理 lang="ts" 脚本（eslint-plugin-svelte
      // 官方 TS 配置方式）；T9 遮罩页起组件脚本含类型标注，缺此 parser 会报 Parsing error。
      parserOptions: {
        parser: ts.parser,
        svelteConfig,
      },
    },
  },
  {
    files: ['**/*.{js,mjs,ts,svelte}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
);
