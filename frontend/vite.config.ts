import { defineConfig, Plugin } from 'vite';
import preact from '@preact/preset-vite';
import wasm from "vite-plugin-wasm"
import topLevelAwait from 'vite-plugin-top-level-await';
import { buildSync } from "esbuild";
import { join } from "node:path";
import versionPlugin from './vite-plugin-version';


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
			define: {
				__BUILD_TIMESTAMP__: `"${new Date().toISOString()}"`,
			}
		})
	}
}

// https://vitejs.dev/config/
export default defineConfig({
	css: {
		preprocessorOptions: {
			scss: {
				additionalData: process.env.IS_SEB === "true" ? `@import "./_seb.scss";` : `@import "./_warp.scss";`,
				quietDeps: true,
				silenceDeprecations: [
					"mixed-decls",
					"import",
					"color-functions",
					"global-builtin",
				],
				verbose: false,
			}
		},
	},
	resolve: {
		alias: {
			"argon2-browser": "argon2-browser/dist/argon2-bundled.min.js",
			"logo": process.env.IS_SEB === "true" ? "src/assets/seb_logo.png" : "src/assets/warp_logo.png",
			"favicon": process.env.IS_SEB === "true" ? "src/assets/seb_favicon.png" : "src/assets/warp_favicon.png",
			"links": process.env.IS_SEB === "true" ? "src/links/seb.ts" : "src/links/warp.ts",
			"translations-de": process.env.IS_SEB === "true" ? "src/locales/branding/seb_de.ts" : "src/locales/branding/warp_de.ts",
			"translations-en": process.env.IS_SEB === "true" ? "src/locales/branding/seb_en.ts" : "src/locales/branding/warp_en.ts",
		}
	},
	// build: {
	// 	minify: false,
	// 	sourcemap: true,
	// },
	plugins: [
		preact(),
		wasm(),
		topLevelAwait(),
		swBuildPlugin,
		versionPlugin,
	],
	worker: {
		format: "es",
		plugins: () => [
			wasm(),
			topLevelAwait(),
		],
	}
});
