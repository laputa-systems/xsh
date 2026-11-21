import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

type Plan = { release: string; waves: string[][] };
type Outcome = { host: string; started: svc.Started | null; kind: string | null };

const plan = JSON.parse(readFileSync("/work/rollout.json", "utf8")) as Plan;

async function deployWave(hosts: string[]): Promise<Outcome[]> {
  const outcomes: Outcome[] = new Array(hosts.length);
  let next = 0;
  async function worker(): Promise<void> {
    while (next < hosts.length) {
      const index = next++;
      const host = hosts[index];
      try {
        const started = await svc.deployStart({ host, release: plan.release }, { timeoutMs: 1000 });
        outcomes[index] = { host, started, kind: null };
      } catch (error) {
        const kind = error instanceof svc.ServiceError ? error.kind : "Failed";
        outcomes[index] = { host, started: null, kind };
      }
    }
  }
  await Promise.all(hosts.map(() => worker()));
  return outcomes;
}

const seen = new Set<string>();
const deployed: { host: string; revision: number }[] = [];
const failed: { host: string; kind: string }[] = [];
const skipped: string[] = [];

for (const wave of plan.waves) {
  const hosts: string[] = [];
  for (const host of wave) {
    if (!seen.has(host)) {
      seen.add(host);
      hosts.push(host);
    }
  }
  if (failed.length > 0) {
    skipped.push(...hosts);
    continue;
  }
  for (const outcome of await deployWave(hosts)) {
    if (outcome.started !== null) deployed.push({ host: outcome.host, revision: outcome.started.revision });
    else failed.push({ host: outcome.host, kind: outcome.kind ?? "Failed" });
  }
}

console.log(JSON.stringify({ deployed, failed, skipped }));
process.exitCode = failed.length > 0 ? 4 : 0;
