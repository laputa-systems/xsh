# Notes

## Suspected defect: `sort-by(desc: true)` reverses equal keys

`xsht api language:stream.sort-by` says: "The sort is stable, so items with equal keys
keep their source order and the two-pass idiom (sort by the secondary key first, then by
the primary key) produces a reliable compound ordering." With `desc: true`, items with
equal keys come out in reverse source order, so the documented two-pass idiom gives the
wrong secondary order for a descending primary key:

```xsh
let rows = [{name: "a", n: 1}, {name: "b", n: 1}, {name: "c", n: 2}]
let ordered = rows |> sort-by(desc: true) .n
print ([r.name for r in ordered].join(" "))
```

prints `c b a` on the frozen toolchain; a stable descending sort gives `c a b`.

This task sorts people by page count, most first, then by name; the natural XSH program
(`|> sort-by .person |> sort-by(desc: true) .pages`) reverses the name order among equal
counts, and the `flaky` and `quiet` variants reject it. A correct XSH program exists: the
reference sorts once by the record key `{first: 0 - p.pages, second: p.person}`.
