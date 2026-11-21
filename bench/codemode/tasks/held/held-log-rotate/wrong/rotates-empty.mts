import { lstatSync, readdirSync, readFileSync, renameSync, truncateSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { gzipSync } from "node:zlib";

const USAGE = "usage: rotate [--keep N] [--min-bytes N] DIR";

function usage(message: string): never {
  console.error(`${message}\n${USAGE}`);
  process.exit(2);
}

function integer(text: string | undefined, fallback: number, minimum: number, name: string): number {
  if (text === undefined) return fallback;
  if (!/^[0-9]+$/.test(text)) usage(`${name} must be a decimal integer`);
  const value = Number(text);
  if (value < minimum) usage(`${name} must be at least ${minimum}`);
  return value;
}

let parsed;
try {
  parsed = parseArgs({
    args: process.argv.slice(2),
    options: { keep: { type: "string" }, "min-bytes": { type: "string" } },
    allowPositionals: true,
    strict: true,
  });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}
if (parsed.positionals.length !== 1) usage("expected one DIR");
const keep = integer(parsed.values.keep, 3, 1, "--keep");
const minBytes = integer(parsed.values["min-bytes"], 1024, 0, "--min-bytes");
const dir = parsed.positionals[0];

const names = readdirSync(dir);
const byteOrder = (a: string, b: string) => Buffer.compare(Buffer.from(a), Buffer.from(b));

for (const log of names.filter((n) => n.endsWith(".log")).sort(byteOrder)) {
  const info = lstatSync(join(dir, log));
  if (!info.isFile() || info.size < minBytes) continue;
  const prefix = `${log}.`;
  const generations: number[] = [];
  for (const name of names) {
    if (!name.startsWith(prefix) || !name.endsWith(".gz")) continue;
    const digits = name.slice(prefix.length, name.length - 3);
    if (/^[1-9][0-9]*$/.test(digits)) generations.push(Number(digits));
  }
  const deleted: number[] = [];
  for (const k of generations.sort((a, b) => b - a)) {
    const path = join(dir, `${log}.${k}.gz`);
    if (k + 1 <= keep) renameSync(path, join(dir, `${log}.${k + 1}.gz`));
    else {
      unlinkSync(path);
      deleted.push(k);
    }
  }
  writeFileSync(join(dir, `${log}.1.gz`), gzipSync(readFileSync(join(dir, log))));
  truncateSync(join(dir, log), 0);
  deleted.sort((a, b) => a - b);
  console.log(JSON.stringify({ log, bytes: info.size, deleted: deleted.map((k) => `${log}.${k}.gz`) }));
}
