import { readFileSync } from "node:fs";
import { describe, expect, test } from "bun:test";

const theme = readFileSync(new URL("../app/claude-theme.css", import.meta.url), "utf8");

function hexValue(name: string) {
  const match = theme.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6});`));
  if (!match) throw new Error(`missing --${name} color token`);
  return match[1];
}

function relativeLuminance(hex: string) {
  const channels = [1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16) / 255);
  const [red, green, blue] = channels.map((channel) => (
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  ));
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(first: string, second: string) {
  const [lighter, darker] = [relativeLuminance(first), relativeLuminance(second)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

describe("Claude workspace theme accessibility", () => {
  test("keeps normal-size light-theme muted and accent text at WCAG AA contrast", () => {
    const background = hexValue("bg");
    expect(contrastRatio(hexValue("muted"), background)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(hexValue("accent"), background)).toBeGreaterThanOrEqual(4.5);
  });

  test("allows responsive data tables to scroll horizontally instead of clipping", () => {
    expect(theme).toMatch(/\.table-wrap\s*\{[^}]*overflow-x:\s*auto;[^}]*overflow-y:\s*hidden;/s);
  });
});
