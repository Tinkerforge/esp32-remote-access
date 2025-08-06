import { defineConfig } from 'vitest/config';
import preact from '@preact/preset-vite';
import { resolve } from 'node:path';

export default defineConfig({
  plugins: [preact()],
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    globals: true,
    include: ['src/**/*.{test,spec}.{js,ts,jsx,tsx}'],
    exclude: ['tests/**', 'pre-test/**', 'post-test/**', 'node_modules/**'],
  },
  resolve: {
    alias: {
      "react": "preact/compat",
      "react-dom": "preact/compat",
      "react-dom/test-utils": "preact/test-utils",
      "react/jsx-runtime": "preact/jsx-runtime",
      "argon2-browser": "argon2-browser/dist/argon2-bundled.min.js",
      "logo": resolve(__dirname, "src/assets/warp_logo.png"),
      "favicon": resolve(__dirname, "src/assets/warp_favicon.png"),
      "links": resolve(__dirname, "src/links/warp.ts"),
      "translations-de": resolve(__dirname, "src/locales/branding/warp_de.ts"),
      "translations-en": resolve(__dirname, "src/locales/branding/warp_en.ts"),
    }
  },
});
