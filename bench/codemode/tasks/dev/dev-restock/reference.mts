import { readFileSync, writeFileSync } from "node:fs";
import * as svc from "./svc.mts";

const warehouse = readFileSync("/work/warehouse.txt", "utf8").trim();
const placed: { sku: string; order: string }[] = [];
const skipped: string[] = [];
const failed: { sku: string; kind: string }[] = [];

for (const row of await svc.inventoryList({ warehouse })) {
  if (row.on_hand >= row.reorder_at) continue;
  if ((await svc.ordersOpen({ warehouse, sku: row.sku })).length > 0) {
    skipped.push(row.sku);
    continue;
  }
  try {
    const order = await svc.ordersPlace({ warehouse, sku: row.sku, quantity: row.target - row.on_hand });
    placed.push({ sku: row.sku, order: order.id });
  } catch (error) {
    if (!(error instanceof svc.ServiceError)) throw error;
    failed.push({ sku: row.sku, kind: error.kind });
  }
}

const bySku = <T extends { sku: string }>(a: T, b: T) => (a.sku < b.sku ? -1 : a.sku > b.sku ? 1 : 0);
writeFileSync("/work/restock.json", JSON.stringify({
  placed: placed.sort(bySku),
  skipped: skipped.sort(),
  failed: failed.sort(bySku),
}));
