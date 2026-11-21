import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const listing = (dir: string) => {
  try {
    return readdirSync(dir).sort().join(" ");
  } catch (error) {
    return `unreadable: ${(error as Error).message}`;
  }
};
const reachable = async (url: string) => {
  try {
    await fetch(url, { signal: AbortSignal.timeout(2000) });
    return true;
  } catch {
    return false;
  }
};
console.log(`/ = ${listing("/")}`);
console.log(`/task = ${listing("/task")}`);
console.log(`/work = ${listing("/work")}`);
console.log(`/home = ${listing("/home")}`);
console.log(`/root = ${listing("/root")}`);
console.log(`external network reachable: ${await reachable("http://1.1.1.1/")}`);
console.log(`loopback service reachable: ${await reachable("http://127.0.0.1:8080/")}`);
console.log(`python3 runs: ${spawnSync("python3", ["-c", "print(1)"]).status === 0}`);
const mounts = readFileSync("/proc/self/mountinfo", "utf8").split("\n").filter((l) => / \/(task|work)/.test(l)).map((l) => l.split(" ")[4]);
console.log(`bind mounts: ${mounts.join(" ")}`);
console.log(`environment: ${Object.keys(process.env).sort().join(" ")}`);
let writable = true;
try {
  writeFileSync("/etc/probe", "x");
} catch {
  writable = false;
}
console.log(`root filesystem writable: ${writable}`);
