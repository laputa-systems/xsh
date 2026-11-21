import { readFileSync, writeFileSync } from "node:fs";

type Setting = { section: string; key: string; value: string };
function parse(argument: string): Setting | null {
  const eq = argument.indexOf("=");
  if (eq < 0) return null;
  const left = argument.slice(0, eq);
  const dot = left.indexOf(".");
  if (dot <= 0 || dot === left.length - 1) return null;
  return { section: left.slice(0, dot), key: left.slice(dot + 1), value: argument.slice(eq + 1) };
}
function header(line: string): string | null {
  const t = line.trim();
  return t.length >= 2 && t.startsWith("[") && t.endsWith("]") ? t.slice(1, -1) : null;
}
function apply(lines: string[], s: Setting): string[] {
  const wanted = `${s.key} = ${s.value}`;
  let current: string | null = null;
  let headerAt: number | null = null;
  let last: number | null = null;
  for (const [i, line] of lines.entries()) {
    const name = header(line);
    if (name !== null) {
      current = name;
      if (name === s.section && headerAt === null) headerAt = i;
      continue;
    }
    const t = line.trim();
    if (current === s.section && headerAt !== null && !t.startsWith("#") && !t.startsWith(";") && line.includes("=")) {
      if (line.slice(0, line.indexOf("=")).trim() === s.key) return [...lines.slice(0, i), wanted, ...lines.slice(i + 1)];
      last = i;
    }
  }
  if (headerAt === null) return [...lines, `[${s.section}]`, wanted];
  const at = (last ?? headerAt) + 1;
  return [...lines.slice(0, at), wanted, ...lines.slice(at)];
}
const settings: Setting[] = [];
for (const a of process.argv.slice(2)) {
  const s = parse(a);
  if (s === null) {
    console.error(`usage: ${a}`);
    process.exit(2);
  }
  settings.push(s);
}
const before = readFileSync("/work/app.conf", "utf8");
let lines = before.split("\n").slice(0, -1);
for (const s of settings) lines = apply(lines, s);
const after = lines.join("\n") + "\n";
if (after === before) console.log("unchanged");
else {
  writeFileSync("/work/app.conf", after);
  console.log("changed");
}
