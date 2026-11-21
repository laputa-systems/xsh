##! Package catalog.
use types as kinds

## Renders a remote name nobody uses.
export pure with_remote_name(name: Str) -> Str {
  return f"remote {name}"
}

## Loads a catalog from a root path.
export proc load(root: Path) [fs, error] -> Result[kinds.Catalog, Error] {
  return kinds.Catalog(name: root.display())
}
