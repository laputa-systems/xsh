import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

type Desired = { service: string; values: Record<string, string>; drop: string[] };
function merged(entries: svc.Entry[], desired: Desired): svc.Entry[] {
  const present = new Set(entries.map((e) => e.key));
  const kept = entries.filter((e) => !desired.drop.includes(e.key)).map((e) => ({ key: e.key, value: desired.values[e.key] ?? e.value }));
  return [...kept, ...Object.keys(desired.values).filter((k) => !present.has(k)).sort().map((k) => ({ key: k, value: desired.values[k] }))];
}
const desired = JSON.parse(readFileSync("/work/desired.json", "utf8")) as Desired;
let doc = await svc.configGet({ service: desired.service });
for (let attempt = 0; attempt < 2; attempt++) {
  try {
    const stored = await svc.configPut({ service: desired.service, version: doc.version, entries: merged(doc.entries, desired) });
    console.log(`updated ${stored.version}`);
    process.exit(0);
  } catch (error) {
    if (error instanceof svc.ServiceError && error.kind === "Conflict") {
      doc = await svc.configGet({ service: desired.service });
      continue;
    }
    console.log(`unchanged ${doc.version}`);
    process.exit(0);
  }
}
console.error("gave up");
process.exit(4);
