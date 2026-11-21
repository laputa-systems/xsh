import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

const hosts = JSON.parse(readFileSync("/work/hosts.json", "utf8")) as string[];
const versions = await Promise.all(hosts.map(async (host) => {
  try {
    const probe = await svc.healthProbe({ host }, { timeoutMs: 1000 });
    return probe.status === "ok" ? probe.version : null;
  } catch {
    return null;
  }
}));
const up = hosts.flatMap((host, i) => (versions[i] === null ? [] : [{ host, version: versions[i] }]));
console.log(JSON.stringify({ up, down: hosts.filter((_, i) => versions[i] === null) }));
