import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import wasm from "vite-plugin-wasm"
import topLevelAwait from 'vite-plugin-top-level-await';
import { VitePWA } from 'vite-plugin-pwa';

// https://vitejs.dev/config/
export default defineConfig({
	build: {
		minify: false,
	},
	plugins: [
		preact(),
		wasm(),
		topLevelAwait(),
		VitePWA({
			injectRegister: false,
			manifest: false,
			injectManifest: {
				injectionPoint: undefined,
				// rollupFormat: 'iife',
				// this only works with a patched version of VitePWA.
				plugins: [
					wasm(),
					topLevelAwait(),
				],
			},
			strategies: 'injectManifest',
			devOptions: {
				enabled: true,
				type: 'module',
			},
			srcDir: 'src',
			filename: 'worker.ts',
		})
	],
	worker: {
		plugins: [
			wasm(),
			topLevelAwait(),
		]
	}
});
