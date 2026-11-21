# Notes

## Documentation defect: "positional-only" arguments that the checker requires by name

`xsht api api:csv.encode` (and most module functions with defaulted parameters, such as
`csv.write`, `fs.walk`, `Path.rename`, `archive.compress`) says: "Function arguments are
positional-only; parameters marked `= default` may be omitted, but cannot be supplied as
`name = value`." The checker rejects the positional form for `csv.encode`:

```xsh
let rows = [{sku: "A1", name: "Widget", price: "1.00"}]
let text = csv.encode(rows, true, ",", ["sku", "name", "price"])?
```

```text
err[check.named-arg]: this argument is passed by position; it takes the label `headers`
help: pass `headers` by name -> headers:
```

(and the same for `delimiter` and `columns`). The form that checks is
`csv.encode(rows, columns: ["sku", "name", "price"])`. A model that follows the API text
literally writes the rejected form; the check feedback recovers it, at the cost of one
repair turn.

## Judge limit

The judge detects an in-place rewrite of `export/catalog.csv` by its unchanged inode. A
program that removes the old file and then writes a new one can be given the freed inode
back by the filesystem and pass that check, although it is not a one-step replacement.
