#!/usr/bin/env node
"use strict";

// AST-based guard for direct TypeScript filesystem and process access.
// New direct operations must be reviewed and entered in
// docs/security/TYPESCRIPT_PRIVILEGED_OPERATION_INVENTORY.md.

const fs = require("node:fs");
const path = require("node:path");
const ts = require("typescript");

const ROOT = path.resolve(__dirname, "..");
const SOURCE_ROOT = path.join(ROOT, "src");
const inventoryPath = "docs/security/TYPESCRIPT_PRIVILEGED_OPERATION_INVENTORY.md";

const allowedDirectCalls = new Set([
  // Explicit user-directed history export: target comes from showSaveDialog.
  "src/extension.ts:vscode.workspace.fs.writeFile"
]);

const bannedModules = new Set([
  "fs", "node:fs", "fs/promises", "node:fs/promises",
  "child_process", "node:child_process", "worker_threads", "node:worker_threads"
]);
const fsMethods = new Set([
  "readFile", "readFileSync", "writeFile", "writeFileSync", "appendFile",
  "appendFileSync", "truncate", "truncateSync", "unlink", "unlinkSync",
  "rename", "renameSync", "copyFile", "copyFileSync", "mkdir", "mkdirSync",
  "rm", "rmSync", "rmdir", "rmdirSync", "readdir", "readdirSync",
  "stat", "statSync", "lstat", "lstatSync", "realpath", "realpathSync",
  "open", "openSync", "createReadStream", "createWriteStream"
]);
const processMethods = new Set([
  "exec", "execSync", "execFile", "execFileSync", "spawn", "spawnSync", "fork"
]);

const findings = [];
function report(file, node, message) {
  const pos = node.getSourceFile().getLineAndCharacterOfPosition(node.getStart());
  findings.push({
    file: path.relative(ROOT, file).split(path.sep).join("/"),
    line: pos.line + 1,
    message
  });
}
function getString(node) {
  return node && ts.isStringLiteral(node) ? node.text : undefined;
}
function scanFile(file) {
  const text = fs.readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const rel = path.relative(ROOT, file).split(path.sep).join("/");
  const namespaceImports = new Map();
  const namedImports = new Map();
  const localAliases = new Map();

  function visit(node) {
    if (ts.isImportDeclaration(node)) {
      const specifier = getString(node.moduleSpecifier);
      if (specifier && bannedModules.has(specifier)) {
        const kind = /child_process|worker_threads/.test(specifier) ? "process" : "fs";
        const bindings = node.importClause?.namedBindings;
        if (bindings && ts.isNamespaceImport(bindings)) {
          namespaceImports.set(bindings.name.text, kind);
        } else if (bindings && ts.isNamedImports(bindings)) {
          for (const element of bindings.elements) {
            namedImports.set(element.name.text, { kind, imported: (element.propertyName || element.name).text });
          }
        }
        report(file, node, "direct import of privileged module '" + specifier + "'");
      }
    }

    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) {
      const initializer = node.initializer;
      if (ts.isIdentifier(initializer) && namespaceImports.has(initializer.text)) {
        localAliases.set(node.name.text, namespaceImports.get(initializer.text));
      } else if (ts.isPropertyAccessExpression(initializer)) {
        const owner = initializer.expression.getText(source);
        const method = initializer.name.text;
        if ((namespaceImports.get(owner) === "fs" && fsMethods.has(method)) ||
            (namespaceImports.get(owner) === "process" && processMethods.has(method))) {
          localAliases.set(node.name.text, namespaceImports.get(owner));
        }
      }
    }

    if (ts.isCallExpression(node)) {
      const expr = node.expression;
      let finding;

      if (ts.isPropertyAccessExpression(expr)) {
        const owner = expr.expression.getText(source);
        const method = expr.name.text;
        const isVscodeFs = owner === "vscode.workspace.fs" ||
          owner.endsWith(".workspace.fs") || owner === "workspace.fs";
        const isImportedNamespace = namespaceImports.has(owner) || localAliases.has(owner);
        const importedKind = namespaceImports.get(owner) || localAliases.get(owner);

        if ((isVscodeFs && fsMethods.has(method)) ||
            (importedKind === "fs" && fsMethods.has(method))) {
          const canonical = owner + "." + method;
          const key = rel + ":" + canonical;
          if (!allowedDirectCalls.has(key)) finding = "direct filesystem API call '" + canonical + "'";
        } else if (importedKind === "process" && processMethods.has(method)) {
          finding = "direct process API call '" + owner + "." + method + "'";
        }
      } else if (ts.isIdentifier(expr)) {
        const imported = namedImports.get(expr.text);
        const aliasKind = localAliases.get(expr.text);
        if (imported && imported.kind === "process" && processMethods.has(imported.imported)) {
          finding = "direct process API call '" + imported.imported + "'";
        } else if (aliasKind === "process" || (imported && imported.kind === "fs")) {
          finding = "aliased privileged API call '" + expr.text + "'";
        }
      }

      if (finding) report(file, node, finding);
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
}

function walk(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.isFile() && full.endsWith(".ts") && !full.endsWith(".d.ts")) scanFile(full);
  }
}
if (!fs.existsSync(path.join(ROOT, inventoryPath))) {
  console.error("Missing privileged-operation inventory: " + inventoryPath);
  process.exit(2);
}
walk(SOURCE_ROOT);
if (findings.length) {
  console.error("Uninventoryed direct filesystem/process operations detected:");
  for (const finding of findings) {
    console.error("- " + finding.file + ":" + finding.line + " " + finding.message);
  }
  console.error("Review each operation and update the security inventory before adding a narrow exception.");
  process.exit(1);
}
console.log("TypeScript privileged-operation regression gate passed.");
