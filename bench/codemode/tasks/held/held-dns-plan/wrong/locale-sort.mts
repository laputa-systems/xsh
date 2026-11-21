import { readFileSync } from "node:fs";
import { parseArgs } from "node:util";
import * as svc from "./svc.mts";

type Desired = { name: string; kind: string; value: string; ttl: number };
type Action = { action: "create" | "update" | "conflict" | "delete"; name: string; kind: string; value: string; ttl: number };

const USAGE = "usage: dnsplan [--apply] ZONE DESIRED";

function usage(message: string): never {
  console.error(`${message}\n${USAGE}`);
  process.exit(2);
}

function parseCommandLine(): { apply: boolean; zone: string; file: string } {
  let parsed;
  try {
    parsed = parseArgs({
      args: process.argv.slice(2),
      options: { apply: { type: "boolean", default: false } },
      allowPositionals: true,
      strict: true,
    });
  } catch (error) {
    usage(error instanceof Error ? error.message : String(error));
  }
  if (parsed.positionals.length !== 2) usage("expected ZONE and DESIRED");
  return { apply: parsed.values.apply ?? false, zone: parsed.positionals[0], file: parsed.positionals[1] };
}

function isDesired(value: unknown): value is Desired {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const r = value as Record<string, unknown>;
  return typeof r.name === "string" && typeof r.kind === "string" && typeof r.value === "string"
    && Number.isInteger(r.ttl);
}

function loadDesired(file: string): Desired[] {
  let data: unknown;
  try {
    data = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    usage(`${file}: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!Array.isArray(data) || !data.every(isDesired)) usage(`${file}: not an array of records`);
  const seen = new Set<string>();
  for (const record of data) {
    const key = JSON.stringify([record.name, record.kind]);
    if (seen.has(key)) usage(`${file}: ${record.name} ${record.kind} is listed twice`);
    seen.add(key);
  }
  return data;
}

function compareBytes(a: string, b: string): number {
  return Buffer.compare(Buffer.from(a), Buffer.from(b));
}

function plan(desired: Desired[], current: svc.DnsRecord[]): Action[] {
  const actions: Action[] = [];
  for (const wanted of desired) {
    const present = current.find((r) => r.name === wanted.name && r.kind === wanted.kind);
    if (present === undefined) actions.push({ action: "create", ...wanted });
    else if (present.value !== wanted.value || present.ttl !== wanted.ttl) {
      actions.push({ action: present.owner === "dnsplan" ? "update" : "conflict", ...wanted });
    }
  }
  for (const record of current) {
    const desiredHere = desired.some((w) => w.name === record.name && w.kind === record.kind);
    if (record.owner === "dnsplan" && !desiredHere) {
      actions.push({ action: "delete", name: record.name, kind: record.kind, value: record.value, ttl: record.ttl });
    }
  }
  return actions.sort((a, b) => a.name.localeCompare(b.name) || a.kind.localeCompare(b.kind));
}

async function perform(zone: string, action: Action, apply: boolean): Promise<string> {
  if (action.action === "conflict") return "skipped";
  if (!apply) return "planned";
  try {
    if (action.action === "delete") await svc.dnsDelete({ zone, name: action.name, kind: action.kind });
    else await svc.dnsPut({ zone, name: action.name, kind: action.kind, value: action.value, ttl: action.ttl });
    return "applied";
  } catch {
    return "failed";
  }
}

const { apply, zone, file } = parseCommandLine();
const desired = loadDesired(file);
const current = await svc.dnsRecords({ zone });
let clean = true;
for (const action of plan(desired, current)) {
  const result = await perform(zone, action, apply);
  if (result !== "planned" && result !== "applied") clean = false;
  console.log(JSON.stringify({ action: action.action, name: action.name, kind: action.kind, result }));
}
process.exitCode = clean ? 0 : 4;
