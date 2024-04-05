import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import wasm from "vite-plugin-wasm"
import topLevelAwait from 'vite-plugin-top-level-await';
import { VitePWA } from 'vite-plugin-pwa';

// https://vitejs.dev/config/
export default defineConfig({
	resolve: {
		alias: {
			"argon2-browser": "argon2-browser/dist/argon2-bundled.min.js"
		}
	},
	build: {
		minify: false,
	},
	plugins: [
		preact(),
		wasm(),
		topLevelAwait(),
		VitePWA({
			minify: false,
			strategies: 'injectManifest',
			injectRegister: false,
			injectManifest: {
				injectionPoint: null,
			},
			manifest: false,
			devOptions: {
				enabled: true,
				type: 'module',
			},
			srcDir: 'src',
			filename: 'sw.ts',
		})
	],
	worker: {
		plugins: [
			wasm(),
			topLevelAwait(),
		]
	}
});
