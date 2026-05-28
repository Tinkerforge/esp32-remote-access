import { defineConfig, Plugin } from 'vite';
import preact from '@preact/preset-vite';
import wasm from "vite-plugin-wasm"
import { buildSync } from "esbuild";
import { join, resolve } from "node:path";
import versionPlugin from './vite-plugin-version';


const appleItunesMetaPlugin: Plugin = {
	name: "AppleItunesMeta",
	transformIndexHtml() {
		if (process.env.BRAND !== "seb") {
			return [
				{
					tag: "meta",
					attrs: {
						name: "apple-itunes-app",
						content: "app-id=6736695801",
					},
					injectTo: "head",
				},
			];
		}
	},
};

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
				additionalData: process.env.BRAND === "seb" ? `@import "./_seb.scss";` : `@import "./_warp.scss";`,
				quietDeps: true,
				silenceDeprecations: [
					"import",
					"color-functions",
					"global-builtin",
					"if-function",
				],
				verbose: false,
			}
		},
	},
	resolve: {
		alias: {
			"argon2-browser": "argon2-browser/dist/argon2-bundled.min.js",
			"logo": resolve(__dirname, process.env.BRAND === "seb" ? "src/assets/seb_logo.png" : "src/assets/warp_logo.png"),
			"favicon": resolve(__dirname, process.env.BRAND === "seb" ? "src/assets/seb_favicon.png" : "src/assets/warp_favicon.png"),
			"links": resolve(__dirname, process.env.BRAND === "seb" ? "src/links/seb.ts" : "src/links/warp.ts"),
			"translations-de": resolve(__dirname, process.env.BRAND === "seb" ? "src/locales/branding/seb_de.ts" : "src/locales/branding/warp_de.ts"),
			"translations-en": resolve(__dirname, process.env.BRAND === "seb" ? "src/locales/branding/seb_en.ts" : "src/locales/branding/warp_en.ts"),
		}
	},
	// build: {
	// 	minify: false,
	// 	sourcemap: true,
	// },
	plugins: [
		preact(),
		wasm(),
		swBuildPlugin,
		appleItunesMetaPlugin,
		versionPlugin,
	],
	build: {
		rolldownOptions: {
			checks: {
				commonJsVariableInEsm: false,
				pluginTimings: false,
			},
		},
	},
	worker: {
		format: "es",
		rolldownOptions: {
			checks: {
				commonJsVariableInEsm: false,
			},
		},
		plugins: () => [
			wasm(),
		],
	}
});
