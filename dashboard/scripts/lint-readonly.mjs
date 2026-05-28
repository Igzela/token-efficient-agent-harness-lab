import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const appDir = join(root, "src", "app");
const forbidden = [
  "approve",
  "deploy",
  "execute",
  "merge",
  "run",
];

function files(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

const findings = [];
for (const file of files(appDir)) {
  if (!/\.(tsx?|css)$/.test(file)) continue;
  const text = readFileSync(file, "utf8").toLowerCase();
  for (const word of forbidden) {
    if (new RegExp(`\\b${word}\\b`).test(text)) {
      findings.push(`${file.replace(root, "")}: forbidden dashboard control word "${word}"`);
    }
  }
  const importsDispatchClient = /import\s*{[^}]*\bdispatch\b[^}]*}\s*from\s*"@\/lib\/api-client"/.test(text);
  if (/\bdispatch\s*\(/.test(text) || importsDispatchClient) {
    findings.push(`${file.replace(root, "")}: dashboard app must not call dispatch`);
  }
}

if (findings.length) {
  console.error(findings.join("\n"));
  process.exit(1);
}

console.log("dashboard readonly lint passed");
