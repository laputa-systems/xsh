# Notes

The `flat` variant passes the relative operand `data`. `fs.walk(p"data")` yields
absolute paths (`/work/data/...`), and `Path.relative_to(p"data")` then returns them
unchanged instead of failing as its contract says (see the reproducer in
`tasks/held/held-dedupe-tree/NOTES.md`). The reference resolves the operand first
(`dir.resolve()?`). With `--depth 0` only the `.` line is printed, so the defect does not
change this variant's expected output; it would on a deeper relative run.

The `deep` and `public` variants sort equal totals by path. `sort-by(desc: true)` does
not keep equal keys in source order (see `tasks/session/session-oncall-report/NOTES.md`),
so the reference sorts once by the key `{first: 0 - total, second: path}`.
