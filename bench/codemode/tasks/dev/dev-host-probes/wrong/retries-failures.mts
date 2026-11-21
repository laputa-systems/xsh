import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

const hosts = JSON.parse(readFileSync("/work/hosts.json", "utf8")) as string[];
const versions: (string | null)[] = hosts.map(() => null);
let next = 0;
async function probe(host: string): Promise<string | null> {
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const p = await svc.healthProbe({ host }, { timeoutMs: 1000 });
      return p.status === "ok" ? p.version : null;
    } catch {
      continue;
    }
  }
  return null;
}
async function worker(): Promise<void> {
  while (next < hosts.length) {
    const index = next++;
    versions[index] = await probe(hosts[index]);
  }
}
await Promise.all([worker(), worker(), worker()]);
const up = hosts.flatMap((host, i) => (versions[i] === null ? [] : [{ host, version: versions[i] }]));
console.log(JSON.stringify({ up, down: hosts.filter((_, i) => versions[i] === null) }));
