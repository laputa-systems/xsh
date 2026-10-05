nominal type Package = {id: Str, version: Str}
nominal type Tagged = Str # error: check.schema

type Listing = {id: Str, version: Str}
type Either = Union[Package, Listing] # error: check.union-type

pure consume_package(pkg: Package) -> Str {
  pkg.id
}

proc install(id: Str, version: Str, listing: Listing, raw: Any) [io] {
  print consume_package({id, version}) # error: check.type-mismatch
  print consume_package(listing) # error: check.type-mismatch
  print (raw is Package) # error: check.pattern-type
  print (listing is Package) # error: check.pattern-type
  print consume_package(Package(id:, version:))
}
