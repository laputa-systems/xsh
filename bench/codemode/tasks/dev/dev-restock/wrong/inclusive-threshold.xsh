use svc

type Placement = {sku: Str, order: Str}

type Failure = {sku: Str, kind: Str}

let warehouse = p"/work/warehouse.txt".read_text()?.trim()
var placed: List[Placement] = []
var skipped: List[Str] = []
var failed: List[Failure] = []

for row in svc.inventory_list(warehouse)? {
  if row.on_hand > row.reorder_at {
    continue
  }

  let open = svc.orders_open(warehouse, row.sku) ?? []
  if ! open.is_empty() {
    skipped += [row.sku]
    continue
  }

  match svc.orders_place(warehouse, row.sku, row.target - row.on_hand) {
    Ok(order) => placed += [Placement(sku: row.sku, order: order.id)]
    Err(_) => failed += [Failure(sku: row.sku, kind: "Failed")]
  }
}

let report = {placed: placed |> sort-by .sku, skipped: skipped |> sort(), failed: failed}
p"/work/restock.json".write(json.encode(report)?)
