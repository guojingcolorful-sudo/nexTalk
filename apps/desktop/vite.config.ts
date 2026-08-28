import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Safari 15.6 floor (macOS 12.7 WKWebView) — Vite 7 defaults to Safari 16.0
export default defineConfig({
  plugins: [react()],
  build: { target: ['safari15', 'es2022'] },
  server: { port: 1420, strictPort: true },
});
