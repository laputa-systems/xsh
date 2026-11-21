import { readFileSync, writeFileSync } from "node:fs";
import * as svc from "./svc.mts";

const warehouse = readFileSync("/work/warehouse.txt", "utf8").trim();
const placed: { sku: string; order: string }[] = [];
const skipped: string[] = [];
const failed: { sku: string; kind: string }[] = [];
try {
  for (const row of await svc.inventoryList({ warehouse })) {
    if (row.on_hand >= row.reorder_at) continue;
    if ((await svc.ordersOpen({ warehouse, sku: row.sku })).length > 0) {
      skipped.push(row.sku);
      continue;
    }
    const order = await svc.ordersPlace({ warehouse, sku: row.sku, quantity: row.target - row.on_hand });
    placed.push({ sku: row.sku, order: order.id });
  }
} catch (error) {
  if (error instanceof svc.ServiceError) failed.push({ sku: "?", kind: error.kind });
}
writeFileSync("/work/restock.json", JSON.stringify({ placed, skipped: skipped.sort(), failed }));
