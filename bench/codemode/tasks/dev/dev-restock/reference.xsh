use svc

type Placement = {sku: Str, order: Str}

type Failure = {sku: Str, kind: Str}

pure kind_of(error: Error) -> Str {
  match error {
    svc.ServiceError.Timeout {..} => "Timeout"
    svc.ServiceError.Unavailable {..} => "Unavailable"
    svc.ServiceError.RateLimited {..} => "RateLimited"
    svc.ServiceError.NotFound {..} => "NotFound"
    svc.ServiceError.PermissionDenied {..} => "PermissionDenied"
    svc.ServiceError.Invalid {..} => "Invalid"
    svc.ServiceError.Conflict {..} => "Conflict"
    else => "Failed"
  }
}

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

  match svc.orders_place(warehouse, row.sku, row.target - row.on_hand) {
    Ok(order) => placed += [Placement(sku: row.sku, order: order.id)]
    Err(error) => failed += [Failure(sku: row.sku, kind: kind_of(error))]
  }
}

let report = {
  placed: placed |> sort-by .sku,
  skipped: skipped |> sort(),
  failed: failed |> sort-by .sku,
}
p"/work/restock.json".write(json.encode(report)?)
