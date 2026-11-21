import { createHash } from "node:crypto";
import { copyFileSync, lstatSync, mkdirSync, readdirSync, readFileSync, unlinkSync } from "node:fs";
import { dirname, join } from "node:path";

type Copy = { rel: string; size: number; digest: string };

const ROOT = "/work/photos";
const DEST = "/work/duplicates";

function byteOrder(a: string, b: string): number {
  return Buffer.compare(Buffer.from(a), Buffer.from(b));
}

function collect(rel: string, found: Copy[]): void {
  for (const name of readdirSync(join(ROOT, rel))) {
    const child = rel === "" ? name : `${rel}/${name}`;
    const info = lstatSync(join(ROOT, child));
    if (info.isDirectory()) collect(child, found);
    else if (info.isFile() && info.size > 0) {
      const digest = createHash("sha256").update(readFileSync(join(ROOT, child))).digest("hex");
      found.push({ rel: child, size: info.size, digest });
    }
  }
}

const copies: Copy[] = [];
collect("", copies);
copies.sort((a, b) => byteOrder(a.rel, b.rel));

const byDigest = new Map<string, Copy[]>();
for (const copy of copies) {
  const same = byDigest.get(copy.digest) ?? [];
  same.push(copy);
  byDigest.set(copy.digest, same);
}

const groups: { kept: string; moved: string[] }[] = [];
let movedBytes = 0;
for (const same of byDigest.values()) {
  if (same.length < 2) continue;
  const extra = same.slice(1);
  for (const copy of extra) {
    const target = join(DEST, copy.rel);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(join(ROOT, copy.rel), target);
    unlinkSync(join(ROOT, copy.rel));
    movedBytes += copy.size;
  }
  groups.push({ kept: same[0].rel, moved: extra.map((c) => c.rel) });
}
groups.sort((a, b) => byteOrder(a.kept, b.kept));

console.log(JSON.stringify({ groups, moved_bytes: movedBytes }));
