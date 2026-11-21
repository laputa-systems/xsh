import { execFile } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

type Outcome = { input: string; status: "encoded" | "skipped" | "failed"; error: string | null };

const INCOMING = "/work/incoming";
const OUT = "/work/out";
mkdirSync(OUT, { recursive: true });

const target = (name: string) => join(OUT, `${name.slice(0, -4)}.flac`);
const names = readdirSync(INCOMING)
  .filter((name) => name.endsWith(".wav") && lstatSync(join(INCOMING, name)).isFile())
  .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));

function encode(name: string): Promise<Outcome> {
  return new Promise((resolve) => {
    execFile("/work/bin/encode", [join(INCOMING, name), target(name)], (error, _stdout, stderr) => {
      if (error === null) return resolve({ input: name, status: "encoded", error: null });
      rmSync(target(name), { force: true });
      resolve({ input: name, status: "failed", error: String(stderr).trim() });
    });
  });
}

const outcomes = new Map<string, Outcome>();
const pending: string[] = [];
for (const name of names) {
  if (existsSync(target(name))) outcomes.set(name, { input: name, status: "skipped", error: null });
  else pending.push(name);
}

let next = 0;
async function worker(): Promise<void> {
  while (next < pending.length) {
    const name = pending[next++];
    outcomes.set(name, await encode(name));
  }
}
await Promise.all([worker(), worker(), worker()]);

let failed = false;
for (const name of names) {
  const outcome = outcomes.get(name)!;
  if (outcome.status === "failed") failed = true;
  console.log(JSON.stringify(outcome));
}
process.exitCode = failed ? 4 : 0;
