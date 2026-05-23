import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const stylesheetPath = path.resolve(__dirname, "../apps/codex-pilot-manager/src/styles.css");
const stylesheet = readFileSync(stylesheetPath, "utf8");

const allowedRanges = findTopLevelThemeBlocks(stylesheet, [":root", ":root.dark"]);
const colorPattern = /#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla)\([^)]*\)/g;
const violations = [];

for (const match of stylesheet.matchAll(colorPattern)) {
  const start = match.index ?? 0;
  if (isInAllowedRange(start, allowedRanges)) continue;

  violations.push({
    line: lineNumberAt(stylesheet, start),
    value: match[0],
  });
}

if (violations.length > 0) {
  console.error("Manager theme color guard failed:");
  for (const violation of violations) {
    console.error(
      `- styles.css:${violation.line} uses "${violation.value}" outside :root/:root.dark. Replace it with a theme variable.`,
    );
  }
  process.exit(1);
}

console.log("Manager theme color guard passed.");

function findTopLevelThemeBlocks(source, selectors) {
  const ranges = [];

  for (const selector of selectors) {
    let searchFrom = 0;
    while (searchFrom < source.length) {
      const selectorIndex = source.indexOf(selector, searchFrom);
      if (selectorIndex === -1) break;
      const openBraceIndex = skipWhitespace(source, selectorIndex + selector.length);
      if (source[openBraceIndex] !== "{") {
        searchFrom = selectorIndex + selector.length;
        continue;
      }

      if (!isTopLevelSelector(source, selectorIndex)) {
        searchFrom = selectorIndex + selector.length;
        continue;
      }

      const closeBraceIndex = findMatchingBrace(source, openBraceIndex);
      ranges.push([selectorIndex, closeBraceIndex + 1]);
      searchFrom = closeBraceIndex + 1;
    }
  }

  return ranges;
}

function isTopLevelSelector(source, selectorIndex) {
  let depth = 0;
  for (let index = 0; index < selectorIndex; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
  }
  return depth === 0;
}

function skipWhitespace(source, index) {
  let cursor = index;
  while (cursor < source.length && /\s/.test(source[cursor])) cursor += 1;
  return cursor;
}

function findMatchingBrace(source, openBraceIndex) {
  let depth = 0;
  for (let index = openBraceIndex; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }

  throw new Error("Unmatched brace while scanning manager theme blocks.");
}

function isInAllowedRange(position, ranges) {
  return ranges.some(([start, end]) => position >= start && position < end);
}

function lineNumberAt(source, position) {
  let line = 1;
  for (let index = 0; index < position; index += 1) {
    if (source[index] === "\n") line += 1;
  }
  return line;
}
