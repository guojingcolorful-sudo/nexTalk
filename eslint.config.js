import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';

export default tseslint.config(
  {
    ignores: [
      'node_modules/',
      'dist/',
      'apps/*/dist/',
      'packages/*/dist/',
      'src-tauri/target/',
      'playwright-report/',
      'test-results/',
      'e2e/',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
    },
  },
  {
    files: ['**/*.test.{ts,tsx}', '**/*.spec.{ts,tsx}', '**/vitest.config.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },
  {
    // CommonJS build-config files (Tailwind/PostCSS load these via require)
    files: ['**/tailwind.config.js', '**/postcss.config.js'],
    languageOptions: {
      globals: {
        module: 'writable',
        require: 'readonly',
      },
    },
  },
);
