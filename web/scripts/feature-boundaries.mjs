import { dirname, relative, resolve } from "node:path";
import { URL, fileURLToPath } from "node:url";
import ts from "typescript";

const webRoot = fileURLToPath(new URL("../", import.meta.url));
const featureRoot = resolve(webRoot, "src/features");
const config = ts.readConfigFile(resolve(webRoot, "tsconfig.json"), ts.sys.readFile);
const { options } = ts.parseJsonConfigFileContent(config.config, ts.sys, webRoot);

export default {
  meta: {
    type: "problem",
    messages: {
      dynamic: "Use a literal import path so feature boundaries can be checked.",
      internal:
        "Import another feature through its index.ts, or move genuinely shared code outside features.",
    },
    schema: [],
  },
  create(context) {
    const source = relative(featureRoot, context.filename).split("/");
    if (source.length < 2 || source[0] === "..") return {};
    function check(node) {
      const value = node.source ?? node.argument;
      const specifier =
        value?.value ??
        (value?.type === "TemplateLiteral" && value.expressions.length === 0
          ? value.quasis[0].value.cooked
          : null);
      if (typeof specifier !== "string") {
        if (node.type === "ImportExpression") context.report({ node, messageId: "dynamic" });
        return;
      }
      const resolved = ts.resolveModuleName(specifier, context.filename, options, ts.sys).resolvedModule
        ?.resolvedFileName;
      const path =
        resolved ??
        (specifier.startsWith(".")
          ? resolve(dirname(context.filename), specifier)
          : /^(?:@\/|~\/|\/?src\/)/.test(specifier)
            ? resolve(webRoot, "src", specifier.replace(/^(?:@\/|~\/|\/?src\/)/, ""))
            : null);
      if (!path) return;
      const target = relative(featureRoot, path).split("/");
      if (target.length < 2 || target[0] === ".." || target[0] === source[0]) return;
      if (target.length === 2 && /^index(?:\.[cm]?[jt]sx?)?$/.test(target[1])) return;
      context.report({ node, messageId: "internal" });
    }
    return {
      ImportDeclaration: check,
      ExportNamedDeclaration: check,
      ExportAllDeclaration: check,
      ImportExpression: check,
      TSImportType: check,
    };
  },
};
