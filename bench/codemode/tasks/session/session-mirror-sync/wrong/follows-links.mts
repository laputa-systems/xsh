import { createHash } from "node:crypto";
import { statSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import * as svc from "./svc.mts";

type Change = { key: string; upload: boolean; digest: string; content: string };
type Done = { key: string; upload: boolean; ok: boolean };

const ROOT = "/work/site";

function usage(message: string): never {
  console.error(`${message}\nusage: mirror [--delete] BUCKET`);
  process.exit(2);
}

let parsed;
try {
  parsed = parseArgs({ args: process.argv.slice(2), options: { delete: { type: "boolean", default: false } },
    allowPositionals: true, strict: true });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}
if (parsed.positionals.length !== 1) usage("expected one BUCKET");
const bucket = parsed.positionals[0];
const removeExtra = parsed.values.delete ?? false;

function walk(rel: string, found: string[]): void {
  for (const name of readdirSync(join(ROOT, rel))) {
    if (name.startsWith(".")) continue;
    const child = rel === "" ? name : `${rel}/${name}`;
    const info = statSync(join(ROOT, child));
    if (info.isDirectory()) walk(child, found);
    else if (info.isFile()) found.push(child);
  }
}

const keys: string[] = [];
walk("", keys);
const stored = await svc.storeList({ bucket });
const known = new Map(stored.map((o) => [o.key, o.sha256]));

const changes: Change[] = [];
let unchanged = 0;
for (const key of keys) {
  const data = readFileSync(join(ROOT, key));
  const digest = createHash("sha256").update(data).digest("hex");
  if (known.get(key) === digest) unchanged += 1;
  else changes.push({ key, upload: true, digest, content: data.toString("base64") });
}
if (removeExtra) {
  const local = new Set(keys);
  for (const object of stored) {
    if (!local.has(object.key)) changes.push({ key: object.key, upload: false, digest: "", content: "" });
  }
}

async function apply(change: Change): Promise<Done> {
  try {
    if (change.upload) {
      await svc.storePut({ bucket, key: change.key, sha256: change.digest, content_base64: change.content });
    } else {
      await svc.storeDelete({ bucket, key: change.key });
    }
    return { key: change.key, upload: change.upload, ok: true };
  } catch (error) {
    const kind = error instanceof svc.ServiceError ? error.kind : "Failed";
    const ok = kind === "Conflict" || (kind === "NotFound" && !change.upload);
    return { key: change.key, upload: change.upload, ok };
  }
}

const done: Done[] = [];
let next = 0;
async function worker(): Promise<void> {
  while (next < changes.length) {
    const change = changes[next++];
    done.push(await apply(change));
  }
}
await Promise.all([worker(), worker(), worker()]);

const byteOrder = (a: string, b: string) => Buffer.compare(Buffer.from(a), Buffer.from(b));
const uploaded = done.filter((d) => d.ok && d.upload).map((d) => d.key).sort(byteOrder);
const deleted = done.filter((d) => d.ok && !d.upload).map((d) => d.key).sort(byteOrder);
const failed = done.filter((d) => !d.ok).map((d) => d.key).sort(byteOrder);
console.log(JSON.stringify({ uploaded, deleted, failed, unchanged }));
process.exitCode = failed.length > 0 ? 4 : 0;
