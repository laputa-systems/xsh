use svc

type Placement = {sku: Str, order: Str}

type Failure = {sku: Str, kind: Str}

let warehouse = p"/work/warehouse.txt".read_text()?.trim()
var placed: List[Placement] = []
var skipped: List[Str] = []
var failed: List[Failure] = []

for row in svc.inventory_list(warehouse)? {
  if row.on_hand >= row.reorder_at {
    continue
  }

  if ! svc.orders_open(warehouse, row.sku)?.is_empty() {
    skipped += [row.sku]
    continue
  }

  let attempt = retry [10ms, 10ms] { svc.orders_place(warehouse, row.sku, row.target - row.on_hand)? }
  match attempt {
    Ok(order) => placed += [Placement(sku: row.sku, order: order.id)]
    Err(error) => failed += [Failure(sku: row.sku, kind: "Unavailable")]
  }
}

let report = {placed: placed |> sort-by .sku, skipped: skipped |> sort(), failed: failed |> sort-by .sku}
p"/work/restock.json".write(json.encode(report)?)
