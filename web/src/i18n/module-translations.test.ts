import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import ts from "typescript";
import { expect, it } from "vitest";

function sources(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? sources(path) : /(?<!\.test)\.tsx?$/.test(path) ? [path] : [];
  });
}

it("defers translation calls in module constants, tables, schemas, defaults and route metadata", () => {
  const eager: string[] = [];
  for (const path of sources("src")) {
    const tree = ts.createSourceFile(path, readFileSync(path, "utf8"), ts.ScriptTarget.Latest, true);
    const names = new Set<string>();
    for (const statement of tree.statements) {
      if (!ts.isImportDeclaration(statement)) continue;
      const bindings = statement.importClause?.namedBindings;
      if (!bindings || !ts.isNamedImports(bindings)) continue;
      for (const binding of bindings.elements) {
        if (/^(translate|t|serverText|plural)$/.test((binding.propertyName ?? binding.name).text))
          names.add(binding.name.text);
      }
    }
    function visit(node: ts.Node) {
      if (ts.isFunctionLike(node)) {
        // Function declarations/default arguments are deferred; immediately invoked callbacks are not.
        if (!ts.isArrowFunction(node) && !ts.isFunctionExpression(node)) return;
        let parent = node.parent;
        while (ts.isParenthesizedExpression(parent)) parent = parent.parent;
        if (!ts.isCallExpression(parent)) return;
        if (parent.expression.getText(tree) === "lazy") return;
      }
      if (ts.isCallExpression(node) && names.has(node.expression.getText(tree))) {
        eager.push(`${path}:${tree.getLineAndCharacterOfPosition(node.getStart()).line + 1}`);
      }
      ts.forEachChild(node, visit);
    }
    visit(tree);
  }
  expect(eager).toEqual([]);
});
