import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

const TRANSIENT = new Set(["Timeout", "Unavailable", "RateLimited"]);

async function lookup(repo: string): Promise<string> {
  for (let attempt = 0; ; attempt++) {
    try {
      return (await svc.ciLatest({ repo })).state;
    } catch (error) {
      const transient = error instanceof svc.ServiceError && TRANSIENT.has(error.kind);
      if (!transient || attempt >= 3) return "missing";
    }
  }
}

const repos = readFileSync("/work/repos.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
for (const repo of repos) console.log(JSON.stringify({ repo, state: await lookup(repo) }));
