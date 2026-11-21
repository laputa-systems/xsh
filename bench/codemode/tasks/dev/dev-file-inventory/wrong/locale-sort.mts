import { lstatSync, readdirSync, readlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = "/work/data";
const rows: any[] = [];
function visit(relative: string): void {
  for (const entry of readdirSync(join(root, relative), { withFileTypes: true })) {
    const path = relative === "" ? entry.name : `${relative}/${entry.name}`;
    if (entry.isSymbolicLink()) rows.push({ path, kind: "symlink", target: readlinkSync(join(root, path), "utf8") });
    else if (entry.isDirectory()) visit(path);
    else rows.push({ path, kind: "file", size: lstatSync(join(root, path)).size });
  }
}
visit("");
rows.sort((a, b) => a.path.localeCompare(b.path));
for (const row of rows) console.log(JSON.stringify(row));
const files = rows.filter((r) => r.kind === "file");
writeFileSync("/work/summary.json", JSON.stringify({ files: files.length, symlinks: rows.length - files.length, bytes: files.reduce((s, r) => s + r.size, 0) }));
