import { readFileSync } from "node:fs";
import { execFile } from "node:child_process";

const files = readFileSync("/work/files.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
const rows = await Promise.all(files.map((file) => new Promise<any>((resolve) => {
  execFile("/work/bin/slowsum", [file], { encoding: "utf8" }, (error, stdout, stderr) => {
    resolve(error === null ? { file, sha256: stdout.trim() } : { file, error: stderr.split("\n")[0].trim() });
  });
})));
for (const row of rows) console.log(JSON.stringify(row));
process.exit(rows.every((r) => "sha256" in r) ? 0 : 4);
