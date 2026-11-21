import { renameSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import * as svc from "./svc.mts";

const LIMIT = 20;
const WAITS = [100, 300];
const TRANSIENT = new Set(["Timeout", "Unavailable", "RateLimited"]);

async function readPage(offset: number): Promise<svc.Product[]> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await svc.catalogList({ offset, limit: LIMIT }, { timeoutMs: 400 });
    } catch (error) {
      const transient = error instanceof svc.ServiceError && TRANSIENT.has(error.kind);
      if (!transient || attempt >= WAITS.length) throw error;
      await sleep(WAITS[attempt]);
    }
  }
}

function field(text: string): string {
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function price(cents: number): string {
  return `${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}

const chosen = new Map<string, svc.Product>();
for (let offset = 0; ; offset += LIMIT) {
  let page: svc.Product[];
  try {
    page = await readPage(offset);
  } catch (error) {
    console.error(`reading the catalog page at offset ${offset} failed: ${String(error)}`);
    process.exit(4);
  }
  for (const item of page.filter((p) => p.active)) {
    const current = chosen.get(item.sku);
    if (current === undefined || item.version > current.version) chosen.set(item.sku, item);
  }
  if (page.length < LIMIT) break;
}

const products = [...chosen.values()]
  .filter((item) => item.active)
  .sort((a, b) => Buffer.compare(Buffer.from(a.sku), Buffer.from(b.sku)));
const lines = ["sku,name,price", ...products.map((p) => [p.sku, p.name, price(p.price_cents)].map(field).join(","))];

const target = "/work/export/catalog.csv";
const temporary = `/work/export/.catalog.csv.${process.pid}.tmp`;
writeFileSync(temporary, lines.join("\n") + "\n");
renameSync(temporary, target);
console.log(`exported ${products.length} products`);
