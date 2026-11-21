import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

type Desired = { service: string; values: Record<string, string>; drop: string[] };

function merged(entries: svc.Entry[], desired: Desired): svc.Entry[] {
  const present = new Set(entries.map((e) => e.key));
  const kept = entries
    .filter((e) => !desired.drop.includes(e.key))
    .map((e) => ({ key: e.key, value: Object.hasOwn(desired.values, e.key) ? desired.values[e.key] : e.value }));
  const added = Object.keys(desired.values).filter((k) => !present.has(k)).sort().map((k) => ({ key: k, value: desired.values[k] }));
  return [...kept, ...added];
}

const same = (a: svc.Entry[], b: svc.Entry[]) => a.length === b.length && a.every((e, i) => e.key === b[i].key && e.value === b[i].value);

const desired = JSON.parse(readFileSync("/work/desired.json", "utf8")) as Desired;
let doc = await svc.configGet({ service: desired.service });
let wanted = merged(doc.entries, desired);

if (same(wanted, doc.entries)) {
  console.log(`unchanged ${doc.version}`);
  process.exit(0);
}

for (let attempt = 0; attempt < 2; attempt++) {
  try {
    const stored = await svc.configPut({ service: desired.service, version: doc.version, entries: wanted });
    console.log(`updated ${stored.version}`);
    process.exit(0);
  } catch (error) {
    if (!(error instanceof svc.ServiceError) || error.kind !== "Conflict") throw error;
    if (attempt === 0) {
      doc = await svc.configGet({ service: desired.service });
      wanted = merged(doc.entries, desired);
    }
  }
}

console.error(`${desired.service}: the document kept changing; gave up after two conflicts`);
process.exit(4);
