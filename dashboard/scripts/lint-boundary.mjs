import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const scanDirs = ["app", "components", "lib"].map((dir) => join(root, "src", dir));
const sourceFile = /\.(tsx?|css)$/;

const forbiddenPatterns = [
  {
    name: "target repository write control",
    pattern: /\b(write|modify|commit|push|merge|apply)\s+(to\s+)?target\s+(repo|repository|repositories)\b/i,
  },
  {
    name: "target write API path",
    pattern: /\/api\/v1\/[^"'`\s)]*(target-(repo|repository)-write|target-write|target-repo-write|target-repository-write)[^"'`\s)]*/i,
  },
  {
    name: "deploy/release API path",
    pattern: /\/api\/v1\/[^"'`\s)]*\b(deploy|deployment|release|tag|merge)[^"'`\s)]*/i,
  },
  {
    name: "patch apply API path",
    pattern: /\/api\/v1\/[^"'`\s)]*(apply-patch|patch-apply|apply-to-target)[^"'`\s)]*/i,
  },
  {
    name: "provider/CLI gate mutation control",
    pattern: /\b(enable|turn on|activate)\s+(provider|cli)\s+(execution|executor|gate)\b/i,
  },
  {
    name: "default-on provider/CLI config",
    pattern: /\b(ACP_ENABLE_PROVIDER_EXECUTION|ACP_ENABLE_CLI_EXECUTION)\s*[:=]\s*["'`]?1\b/,
  },
  {
    name: "default-on execution mode",
    pattern: /\bACP_EXECUTION_MODE\s*[:=]\s*["'`]?(provider|cli|auto)\b/i,
  },
  {
    name: "unattended worker control",
    pattern: /\b(unattended|autonomous)\s+(worker|agent|loop|execution)\b/i,
  },
  {
    name: "provider failover control",
    pattern: /\bprovider\s+failover\b/i,
  },
  {
    name: "legacy admin token environment variable",
    pattern: /\bACP_ADMIN_TOKEN\b/,
  },
  {
    name: "source-tree engine startup command",
    pattern: /\.\/target\/debug\/engine\b/,
  },
  {
    name: "runtime worker state mislabeled as CLI",
    pattern: /\bruntime_workers\b[\s\S]{0,320}\blabel:\s*["']CLI["']/,
  },
];

const forbiddenControls = [
  "Deploy",
  "Release",
  "Merge",
  "Apply patch",
  "Apply to target",
  "Push to target",
  "Enable provider",
  "Enable CLI",
  "Start worker",
  "Run unattended",
  "Provider failover",
];

function files(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

function rel(path) {
  return relative(root, path);
}

const findings = [];
for (const dir of scanDirs) {
  for (const file of files(dir)) {
    if (!sourceFile.test(file)) continue;
    const text = readFileSync(file, "utf8");
    for (const { name, pattern } of forbiddenPatterns) {
      if (pattern.test(text)) {
        findings.push(`${rel(file)}: forbidden boundary capability (${name})`);
      }
    }
    for (const label of forbiddenControls) {
      const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const controlPattern = new RegExp(
        `<button[^>]*>[\\s\\S]{0,240}\\b${escaped}\\b[\\s\\S]{0,120}<\\/button>|aria-label=["'][^"']*\\b${escaped}\\b[^"']*["']`,
        "i",
      );
      if (controlPattern.test(text)) {
        findings.push(`${rel(file)}: forbidden boundary control label "${label}"`);
      }
    }
  }
}

if (findings.length) {
  console.error(findings.join("\n"));
  process.exit(1);
}

console.log("dashboard boundary lint passed");
