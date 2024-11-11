import { defineConfig, Plugin } from 'vite';
import preact from '@preact/preset-vite';
import wasm from "vite-plugin-wasm"
import topLevelAwait from 'vite-plugin-top-level-await';
import { buildSync } from "esbuild";
import { join } from "node:path";

function plugins() {
	return [
		wasm(),
		topLevelAwait(),
	];
}

const swBuildPlugin: Plugin = {
	name: "SWBuild",
	apply: "build",
	enforce: "post",
	transformIndexHtml() {
		buildSync({
			minify: true,
			bundle: true,
			entryPoints: [join(process.cwd(), "src", "sw.ts")],
			outfile: join(process.cwd(), "dist", "sw.js"),
			format: "esm",
		})
	}
}

// https://vitejs.dev/config/
export default defineConfig({
	resolve: {
		alias: {
			"argon2-browser": "argon2-browser/dist/argon2-bundled.min.js"
		}
	},
	build: {
		minify: false,
		sourcemap: true,
	},
	plugins: [
		preact(),
		wasm(),
		topLevelAwait(),
		swBuildPlugin
	],
	worker: {
		plugins: plugins
	}
});
