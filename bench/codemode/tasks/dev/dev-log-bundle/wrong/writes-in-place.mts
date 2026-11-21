import { existsSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";

const names = readFileSync("/work/bundle.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
const tar = spawnSync("tar", ["-czf", "/work/out/logs.tar.gz", "-C", "/work", ...names.map((n) => `logs/${n}`)], { encoding: "utf8" });
if (tar.status !== 0) {
  const missing = names.find((n) => !existsSync(`/work/logs/${n}`));
  rmSync("/work/out/logs.tar.gz", { force: true });
  console.error(`missing log file: ${missing}`);
  process.exit(4);
}
console.log(names.length);
