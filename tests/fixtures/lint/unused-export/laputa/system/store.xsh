##! Store shapes.
const nul = ""

## The key for a store entry.
export pure store_key(name: Str) -> Str {
  return name
}

## The digest for a store entry.
export pure store_digest(name: Str) -> Str {
  return name
}

## The store format.
export const STORE_FORMAT = "v1"

## One store entry.
export type StoreEntry = {key: Str}

## One store receipt.
export type StoreReceipt = {key: Str, digest: Str}

## Publishes one store entry.
export proc publish_store(key: Str) [fs, error] -> Result[Unit] {
  return Ok()
}
