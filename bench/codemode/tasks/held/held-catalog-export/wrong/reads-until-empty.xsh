use svc

const limit = 20

type Row = {sku: Str, name: Str, price: Str}

proc read_page(offset: Int) -> Result[List[svc.Product]] {
  retry [100ms, 300ms] on (svc.ServiceError.Timeout | svc.ServiceError.Unavailable | svc.ServiceError.RateLimited) {
    svc.catalog_list(offset, limit, timeout: 400ms)?
  }
}

pure price(cents: Int) -> Str {
  f"{cents / 100}.{cents % 100:02}"
}

var chosen: Map[Str, svc.Product] = {}
var offset = 0

while true {
  let page = match read_page(offset) {
    Ok(page) => page
    Err(failure) => {
      eprint f"reading the catalog page at offset {offset} failed: {failure.message}"
      exit 4
    }
  }

  for item in page {
    let newer = match chosen.get(item.sku) {
      Ok(current) => item.version > current.version
      Err(_) => true
    }
    if newer {
      chosen[item.sku] = item
    }
  }

  if page.is_empty() {
    break
  }

  offset += limit
}

let rows = chosen.values()
  |> where .active
  |> sort-by .sku
  |> map { |item| Row(sku: item.sku, name: item.name, price: price(item.price_cents)) }

let text = csv.encode(rows, columns: ["sku", "name", "price"])?
p"/work/export/catalog.csv".write_atomic(text)?
print f"exported {rows.len()} products"
