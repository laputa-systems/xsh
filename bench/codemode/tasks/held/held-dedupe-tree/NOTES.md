# Notes

## Suspected defect: `Path.relative_to` does not fail outside its base

`xsht api method:Path.relative_to` says "The result is an error when the path is not
below the requested base", but the signature is `Path.relative_to(base: Path) -> Path`
and a path outside the base comes back unchanged, with no error:

```xsh
let inside = p"/work/photos/a/b.jpg".relative_to(p"/work/photos")
print f"inside: {inside}"
let outside = p"/srv/other/b.jpg".relative_to(p"/work/photos")
print f"outside: {outside}"
```

```text
inside: a/b.jpg
outside: /srv/other/b.jpg
```

(frozen toolchain, exit 0). It matters here because `fs.walk` yields absolute paths
even for a relative root: `fs.walk(p"photos")` returns `/work/photos/...`. So
`entry.path.relative_to(p"photos")` quietly returns absolute paths, and a program
that builds `fp"{dest}/{rel}"` from them writes the wrong targets. The reference
avoids this by walking the absolute root `/work/photos`.

## Diagnostic quality

Naming a record `Group` (a standard record) gives `check.duplicate-name` and then a
second, confusing error at each use: `expected Record, found Record`.
