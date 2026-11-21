import { readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({ args: process.argv.slice(2), options: { keep: { type: "string" }, "dry-run": { type: "boolean" } }, allowPositionals: true });
const keep = Number(values.keep ?? "3");
if (!Number.isInteger(keep) || keep < 1) {
  console.error("bad --keep");
  process.exit(2);
}
const dir = positionals[0];
const dated = /^(.+)-(\d{4})-(\d{2})-(\d{2})\.tar\.gz$/;
const report: Record<string, string[]> = {};
for (const service of readdirSync(dir, { withFileTypes: true })) {
  if (!service.isDirectory()) continue;
  const path = join(dir, service.name);
  const old = readdirSync(path, { withFileTypes: true })
    .filter((e) => e.isFile() && dated.exec(e.name)?.[1] === service.name)
    .map((e) => e.name)
    .sort((a, b) => (a.slice(-17) < b.slice(-17) ? 1 : -1))
    .slice(keep)
    .sort();
  if (!values["dry-run"]) for (const name of old) rmSync(join(path, name));
  report[service.name] = old;
}
console.log(JSON.stringify(report));
