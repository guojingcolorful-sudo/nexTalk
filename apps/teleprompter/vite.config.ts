import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Safari 15.6 floor (macOS 12.7 WKWebView + old phones) — Vite 7 defaults to Safari 16.0
// base './' so the built H5 serves correctly from the LAN server subpath (axum ServeDir)
export default defineConfig({
  plugins: [react()],
  base: './',
  build: { target: ['safari15', 'es2022'] },
});
