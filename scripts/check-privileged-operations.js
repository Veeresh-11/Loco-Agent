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
  "fs",
  "node:fs",
  "fs/promises",
  "node:fs/promises",
  "child_process",
  "node:child_process",
  "worker_threads",
  "node:worker_threads"
]);

const fsMethods = new Set([
  "readFile", "readFileSync", "writeFile", "writeFileSync",
  "appendFile", "appendFileSync", "truncate", "truncateSync",
  "unlink", "unlinkSync", "rename", "renameSync",
  "copyFile", "copyFileSync", "mkdir", "mkdirSync",
  "rm", "rmSync", "rmdir", "rmdirSync", "readdir",
  "readdirSync", "stat", "statSync", "lstat", "lstatSync",
  "realpath", "realpathSync", "open", "openSync",
  "createReadStream", "createWriteStream"
]);

const processMethods = new Set([
  "exec", "execSync", "execFile", "execFileSync",
  "spawn", "spawnSync", "fork"
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

function propertyName(node) {
  if (!node) return undefined;
  if (ts.isIdentifier(node) || ts.isStringLiteral(node)) return node.text;
  return undefined;
}

function moduleName(node) {
  return ts.isStringLiteral(node) ? node.text : undefined;
}

function scanFile(file) {
  const text = fs.readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const rel = path.relative(ROOT, file).split(path.sep).join("/");
  const importedFsNames = new Set();
  const importedProcessNames = new Set();

  function visit(node) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      const specifier = moduleName(node.moduleSpecifier);
      if (specifier && bannedModules.has(specifier)) {
        const bindings = node.importClause?.namedBindings;
        if (bindings && ts.isNamespaceImport(bindings)) {
          (specifier.includes("child_process") || specifier.includes("worker_threads")
            ? importedProcessNames : importedFsNames).add(bindings.name.text);
        }
        report(file, node, "direct import of privileged module '" + specifier + "'");
      }
    }

    if (ts.isCallExpression(node)) {
      const expr = node.expression;
      if (ts.isPropertyAccessExpression(expr)) {
        const owner = expr.expression.getText(source);
        const method = expr.name.text;
        const canonical = owner + "." + method;
        const isVscodeFs = owner === "vscode.workspace.fs" ||
          owner.endsWith(".workspace.fs") ||
          owner === "workspace.fs";

        if ((isVscodeFs && fsMethods.has(method)) ||
            (owner === "cp" && processMethods.has(method)) ||
            (importedFsNames.has(owner) && fsMethods.has(method)) ||
            (importedProcessNames.has(owner) && processMethods.has(method))) {
          const key = rel + ":" + canonical;
          if (!allowedDirectCalls.has(key)) {
            report(file, node, "direct privileged API call '" + canonical + "'");
          }
        }
      }

      if (ts.isIdentifier(expr)) {
        const method = expr.text;
        if (processMethods.has(method)) {
          report(file, node, "unqualified process API call '" + method + "'");
        }
      }
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
  console.error("");
  console.error("Review the operation, update the security inventory, and add a narrowly scoped allowlist entry only when justified.");
  process.exit(1);
}

console.log("TypeScript privileged-operation regression gate passed.");
