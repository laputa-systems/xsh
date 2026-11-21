import { lstatSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

function usage(message: string): never {
  console.error(`${message}\nusage: du-summary [--depth N] [--min-bytes N] DIR`);
  process.exit(2);
}

function count(text: string | undefined, fallback: number, name: string): number {
  if (text === undefined) return fallback;
  if (!/^[0-9]+$/.test(text)) usage(`${name} must be a decimal integer of at least 0`);
  return Number(text);
}

let parsed;
try {
  parsed = parseArgs({
    args: process.argv.slice(2),
    options: { depth: { type: "string" }, "min-bytes": { type: "string" } },
    allowPositionals: true,
    strict: true,
  });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}
if (parsed.positionals.length !== 1) usage("expected one DIR");
const depth = count(parsed.values.depth, 1, "--depth");
const minBytes = count(parsed.values["min-bytes"], 0, "--min-bytes");
const root = parsed.positionals[0];

let isDir = false;
try {
  isDir = statSync(root).isDirectory();
} catch {
  isDir = false;
}
if (!isDir) {
  console.error(`${root} is not a directory`);
  process.exit(2);
}

const totals = new Map<string, number>();

function walk(rel: string, level: number): number {
  let total = 0;
  for (const name of readdirSync(rel === "." ? root : join(root, rel))) {
    const child = rel === "." ? name : `${rel}/${name}`;
    const info = lstatSync(join(root, child));
    if (info.isDirectory()) walk(child, level + 1);
    else if (info.isFile()) total += info.size;
  }
  if (level <= depth) totals.set(rel, total);
  return total;
}
walk(".", 0);

const rows = [...totals].filter(([, total]) => total >= minBytes);
rows.sort((a, b) => b[1] - a[1] || Buffer.compare(Buffer.from(a[0]), Buffer.from(b[0])));
for (const [path, total] of rows) console.log(`${total}\t${path}`);
