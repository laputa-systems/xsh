import { readFileSync } from "node:fs";
import * as svc from "./svc.mts";

const DELAYS = [100, 200, 400];
const TRANSIENT = new Set(["Timeout", "Unavailable", "RateLimited"]);
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function lookup(repo: string): Promise<string> {
  for (let attempt = 0; ; attempt++) {
    try {
      return (await svc.ciLatest({ repo })).state;
    } catch (error) {
      const transient = error instanceof svc.ServiceError && TRANSIENT.has(error.kind);
      if (!transient || attempt >= DELAYS.length) return "missing";
      await sleep(DELAYS[attempt]);
    }
  }
}

const repos = readFileSync("/work/repos.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
const states = await Promise.all(repos.map(lookup));
repos.forEach((repo, i) => console.log(JSON.stringify({ repo, state: states[i] })));
