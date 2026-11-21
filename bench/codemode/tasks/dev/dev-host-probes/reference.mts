import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

const hosts = JSON.parse(readFileSync("/work/hosts.json", "utf8")) as string[];
const versions: (string | null)[] = hosts.map(() => null);
let next = 0;

async function worker(): Promise<void> {
  while (next < hosts.length) {
    const index = next++;
    try {
      const probe = await svc.healthProbe({ host: hosts[index] }, { timeoutMs: 1000 });
      versions[index] = probe.status === "ok" ? probe.version : null;
    } catch {
      versions[index] = null;
    }
  }
}

await Promise.all([worker(), worker(), worker()]);
const up = hosts.flatMap((host, i) => (versions[i] === null ? [] : [{ host, version: versions[i] }]));
const down = hosts.filter((_, i) => versions[i] === null);
console.log(JSON.stringify({ up, down }));
