import { readFileSync } from "node:fs";
import { execFile } from "node:child_process";

const files = readFileSync("/work/files.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
let failed = false;
let next = 0;
async function worker(): Promise<void> {
  while (next < files.length) {
    const file = files[next++];
    const row = await new Promise<any>((resolve) => {
      execFile("/work/bin/slowsum", [file], { encoding: "utf8" }, (error, stdout, stderr) => {
        resolve(error === null ? { file, sha256: stdout.trim() } : { file, error: stderr.split("\n")[0].trim() });
      });
    });
    if ("error" in row) failed = true;
    console.log(JSON.stringify(row));
  }
}
await Promise.all([worker(), worker(), worker()]);
process.exit(failed ? 4 : 0);
