import { defineConfig } from 'vitest/config';
import preact from '@preact/preset-vite';

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
      "argon2-browser": "argon2-browser/dist/argon2-bundled.min.js",
      "logo": "src/assets/warp_logo.png",
      "favicon": "src/assets/warp_favicon.png",
      "links": "src/links/warp.ts",
      "translations-de": "src/locales/branding/warp_de.ts",
      "translations-en": "src/locales/branding/warp_en.ts",
    }
  },
});
