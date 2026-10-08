// Renders each React mirror case in parity/cases.tsx and compares it with the
// Dioxus snapshot of the same case in parity/<Component>.<case>.html. The Rust
// components are the source of truth; regenerate snapshots with
// `UPDATE_PARITY=1 cargo test -p keepbook-dioxus parity`.
import { build } from "esbuild";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseFragment } from "parse5";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

const root = new URL("..", import.meta.url).pathname;
// Inside node_modules so the bundle resolves the package's own react.
const out = join(root, "node_modules/.cache/keepbook-parity.mjs");
await build({
  entryPoints: [join(root, "parity/cases.tsx")],
  bundle: true,
  format: "esm",
  platform: "node",
  jsx: "automatic",
  outfile: out,
  external: ["react", "react-dom", "react/jsx-runtime"],
  loader: { ".svg": "text", ".css": "empty", ".woff2": "empty" },
  logLevel: "error",
});
const { cases } = await import(pathToFileURL(out).href);

const BOOLEAN = new Set(["disabled", "checked", "selected", "hidden", "readonly", "required", "multiple"]);
function attrs(node) {
  const result = [];
  for (const { name, value } of node.attrs ?? []) {
    // React renders these as the selected option and the text content.
    if (name === "value" && (node.nodeName === "select" || node.nodeName === "textarea")) continue;
    if (BOOLEAN.has(name)) {
      if (value !== "false") result.push(`${name}`);
      continue;
    }
    if (name === "class") {
      const classes = value.split(/\s+/).filter(Boolean).sort().join(" ");
      if (classes) result.push(`class="${classes}"`);
      continue;
    }
    if (name === "style") {
      const decls = value
        .split(";")
        .map((d) => d.trim().replace(/\s*:\s*/, ":"))
        .filter(Boolean)
        .sort()
        .join(";");
      if (decls) result.push(`style="${decls}"`);
      continue;
    }
    result.push(`${name}="${value}"`);
  }
  return result.sort().join(" ");
}
function canonical(nodes, depth = 0) {
  const lines = [];
  for (const node of nodes) {
    if (node.nodeName === "#text") {
      const text = node.value.replace(/\s+/g, " ").trim();
      if (text) lines.push(`${"  ".repeat(depth)}"${text}"`);
    } else if (node.nodeName === "#comment") {
      continue;
    } else {
      const a = attrs(node);
      lines.push(`${"  ".repeat(depth)}<${node.nodeName}${a ? " " + a : ""}>`);
      lines.push(...canonical(node.content?.childNodes ?? node.childNodes ?? [], depth + 1));
    }
  }
  return lines;
}
const normalize = (html) => canonical(parseFragment(html).childNodes);

const snapshots = readdirSync(join(root, "parity"))
  .filter((f) => f.endsWith(".html"))
  .map((f) => f.slice(0, -".html".length))
  .sort();
let failures = 0;
for (const name of snapshots) {
  const expected = normalize(readFileSync(join(root, "parity", `${name}.html`), "utf8"));
  if (!cases[name]) {
    console.error(`✗ ${name}: no React case in parity/cases.tsx`);
    failures++;
    continue;
  }
  const actual = normalize(renderToStaticMarkup(createElement(cases[name])));
  const at = expected.findIndex((line, i) => line !== actual[i]);
  if (at === -1 && expected.length === actual.length) continue;
  failures++;
  const i = at === -1 ? Math.min(expected.length, actual.length) : at;
  console.error(`✗ ${name}: differs at line ${i + 1}\n    rust:  ${expected[i] ?? "(end)"}\n    react: ${actual[i] ?? "(end)"}`);
}
for (const name of Object.keys(cases)) {
  if (!snapshots.includes(name)) {
    console.error(`✗ ${name}: React case has no Rust snapshot`);
    failures++;
  }
}
console.error(failures ? `\n${failures} parity failure(s)` : `✓ ${snapshots.length} cases match the Dioxus components`);
process.exit(failures ? 1 : 0);
