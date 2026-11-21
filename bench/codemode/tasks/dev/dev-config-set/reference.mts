import { readFileSync, renameSync, writeFileSync } from "node:fs";

type Setting = { section: string; key: string; value: string };

function parse(argument: string): Setting | null {
  const eq = argument.indexOf("=");
  if (eq < 0) return null;
  const left = argument.slice(0, eq);
  const dot = left.indexOf(".");
  if (dot < 0) return null;
  const section = left.slice(0, dot);
  const key = left.slice(dot + 1);
  if (section === "" || key === "") return null;
  return { section, key, value: argument.slice(eq + 1) };
}

function header(line: string): string | null {
  const text = line.trim();
  return text.length >= 2 && text.startsWith("[") && text.endsWith("]") ? text.slice(1, -1) : null;
}

function settingKey(line: string): string | null {
  const text = line.trim();
  if (header(line) !== null || text.startsWith("#") || text.startsWith(";") || !line.includes("=")) return null;
  return line.slice(0, line.indexOf("=")).trim();
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
    if (current === s.section && headerAt !== null) {
      const key = settingKey(line);
      if (key !== null) {
        if (key === s.key) return [...lines.slice(0, i), wanted, ...lines.slice(i + 1)];
        last = i;
      }
    }
  }
  if (headerAt === null) return [...lines, `[${s.section}]`, wanted];
  const at = (last ?? headerAt) + 1;
  return [...lines.slice(0, at), wanted, ...lines.slice(at)];
}

const settings: Setting[] = [];
for (const argument of process.argv.slice(2)) {
  const setting = parse(argument);
  if (setting === null) {
    console.error(`usage: SECTION.KEY=VALUE ...; cannot read ${argument}`);
    process.exit(2);
  }
  settings.push(setting);
}

const file = "/work/app.conf";
const before = readFileSync(file, "utf8");
let lines = before.split("\n").slice(0, -1);
for (const setting of settings) lines = apply(lines, setting);
const after = lines.join("\n") + "\n";
if (after === before) {
  console.log("unchanged");
} else {
  const temporary = `${file}.tmp-${process.pid}`;
  writeFileSync(temporary, after);
  renameSync(temporary, file);
  console.log("changed");
}
