import { readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

function usage(message: string): never {
  console.error(`prune: ${message}\nusage: prune [--keep N] [--dry-run] DIR`);
  process.exit(2);
}

let parsed;
try {
  parsed = parseArgs({
    args: process.argv.slice(2),
    options: { keep: { type: "string" }, "dry-run": { type: "boolean" } },
    allowPositionals: true,
    strict: true,
  });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}

const keepText = parsed.values.keep ?? "3";
if (!/^[0-9]+$/.test(keepText) || Number(keepText) < 1) usage(`--keep must be a positive integer, got ${keepText}`);
const keep = Number(keepText);
if (parsed.positionals.length !== 1) usage("expected exactly one DIR");
const dir = parsed.positionals[0];
let isDirectory = false;
try {
  isDirectory = statSync(dir).isDirectory();
} catch {
  isDirectory = false;
}
if (!isDirectory) usage(`${dir} is not a directory`);

const dated = /^(.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$/;
const report: Record<string, string[]> = {};
for (const service of readdirSync(dir, { withFileTypes: true })) {
  if (!service.isDirectory()) continue;
  const path = join(dir, service.name);
  const entries = readdirSync(path, { withFileTypes: true });
  if (entries.some((e) => e.name === ".lock")) {
    report[service.name] = [];
    continue;
  }
  const backups = entries
    .filter((e) => e.isFile())
    .map((e) => ({ name: e.name, match: dated.exec(e.name) }))
    .filter((b) => b.match !== null && b.match[1] === service.name)
    .map((b) => ({ name: b.name, date: `${b.match![2]}${b.match![3]}${b.match![4]}` }))
    .sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0));
  const old = backups.slice(keep).map((b) => b.name).sort();
  if (!parsed.values["dry-run"]) for (const name of old) rmSync(join(path, name));
  report[service.name] = old;
}
console.log(JSON.stringify(report));
