# begin example
nominal type Package = {id: Str, version: Str}

type Listing = {id: Str, version: Str}

# Only the constructor builds one from parts.
pure pinned(id: Str, version: Str) -> Package {
  Package(id:, version:)
}

pure install_name(pkg: Package) -> Str {
  f"{pkg.id}-{pkg.version}"
}

# A Package is read as the record it is wherever a record is expected.
pure summary(listing: Listing) -> Str {
  f"{listing.id} {listing.version}"
}

proc resolve(manifest: Str) [error] -> Result[Str] {
  # `.require` is the conversion: it checks the fields and returns a Package.
  let pkg = json.decode(manifest)?.require(Package)?
  let newer = pinned(pkg.id, "2.0")
  # A spread copies fields; the result is an ordinary record.
  let listing = {...newer, version: "2.1"}
  Ok(f"{install_name(pkg)} {install_name(newer)} {summary(newer)} {summary(listing)}")
}

# end example

print (resolve("""{"id": "xsh", "version": "1.0"}""")?)
