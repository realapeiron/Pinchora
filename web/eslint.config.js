import js from "@eslint/js";
import { defineConfig } from "eslint/config";
import jsxA11y from "eslint-plugin-jsx-a11y";
import { dirname, relative, resolve } from "node:path";
import tseslint from "typescript-eslint";

const featureRoot = resolve("src/features");
const boundaries = {
  rules: {
    "feature-boundaries": {
      meta: {
        type: "problem",
        messages: {
          internal:
            "Import another feature through its index.ts, or move genuinely shared code outside features.",
        },
        schema: [],
      },
      create(context) {
        const source = relative(featureRoot, context.filename).split("/");
        if (source.length < 2 || source[0] === "..") return {};
        function check(node) {
          const specifier = node.source?.value;
          if (typeof specifier !== "string" || !specifier.startsWith(".")) return;
          const target = relative(featureRoot, resolve(dirname(context.filename), specifier)).split("/");
          if (target.length < 2 || target[0] === ".." || target[0] === source[0]) return;
          if (target.length === 2 && (target[1] === "index" || target[1] === "index.ts")) return;
          context.report({ node, messageId: "internal" });
        }
        return {
          ImportDeclaration: check,
          ExportNamedDeclaration: check,
          ExportAllDeclaration: check,
          ImportExpression: check,
        };
      },
    },
  },
};

export default defineConfig(
  { ignores: ["dist", "node_modules", "test-results", "playwright-report"] },
  js.configs.recommended,
  tseslint.configs.recommended,
  { plugins: { local: boundaries }, rules: { "local/feature-boundaries": "error" } },
  { files: ["**/*.{jsx,tsx}"], plugins: { "jsx-a11y": jsxA11y }, rules: jsxA11y.configs.recommended.rules },
  // Reef gameplay keeps its existing canvas keyboard model; its source is outside this change.
  {
    files: ["src/reef/ReefStage.tsx"],
    rules: {
      "jsx-a11y/no-interactive-element-to-noninteractive-role": "off",
      "jsx-a11y/no-noninteractive-element-interactions": "off",
    },
  },
  {
    files: ["src/reef/games/{Board,Picross}.tsx"],
    rules: { "jsx-a11y/no-noninteractive-tabindex": "off" },
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },
);
