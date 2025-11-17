import { defineConfig, globalIgnores } from "eslint/config";
import globals from "globals";
import tsParser from "@typescript-eslint/parser";
import js from "@eslint/js";
import tseslint from "typescript-eslint";


export default defineConfig([
    globalIgnores([
        "**/dist/",
        "**/node_modules/",
        "**/target/",
        "**/pkg/",
        "**/*.config.ts",
        "**/*.config.js",
        "**/test-results/",
        "**/playwright-report/",
        "**/dist/",
        "**/node_modules/",
        "**/target/",
        "**/pkg/",
        "**/*.config.ts",
        "**/*.config.js",
        "**/test-results/",
        "**/playwright-report/",
        "**/coverage/",
        "**/.vscode/",
    ]),
    js.configs.recommended,
    tseslint.configs.recommended,
    {
        languageOptions: {
            globals: {
                ...globals.browser,
                ...globals.node,
                ...globals.worker,
                ...globals.serviceworker,
            },

            parser: tsParser,
            ecmaVersion: "latest",
            sourceType: "module",

            parserOptions: {
                ecmaFeatures: {
                    jsx: true,
                },

                project: "./tsconfig.json",
            },
        },

        rules: {
            "react-hooks/rules-of-hooks": "off",
            "react-hooks/exhaustive-deps": "off",
            "no-undef": "off",
            "no-unused-vars": "off",
            "quote-props": "off",
            "no-duplicate-imports": "error",
            "no-case-declarations": "off",
            "no-undef-init": "warn",
            "prefer-const": "warn",
            "prefer-template": "warn",
            "no-else-return": "warn",
            "no-async-promise-executor": "off",
        },
    }, {
        files: ["**/*.test.ts", "**/*.test.tsx", "src/__tests__/**"],
    }
]);
