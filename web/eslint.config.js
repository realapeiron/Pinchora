import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import jsxA11y from "eslint-plugin-jsx-a11y";
import featureBoundaries from "./scripts/feature-boundaries.mjs";
import tseslint from "typescript-eslint";

const boundaries = { rules: { "feature-boundaries": featureBoundaries } };

export default defineConfig(
  { ignores: ["dist", "node_modules", "test-results", "playwright-report"] },
  js.configs.recommended,
  tseslint.configs.recommended,
  { plugins: { local: boundaries }, rules: { "local/feature-boundaries": "error" } },
  { files: ["**/*.{jsx,tsx}"], plugins: { "jsx-a11y": jsxA11y }, rules: jsxA11y.configs.recommended.rules },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },
);
