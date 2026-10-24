type Row = {name: Str}
proc configured(row) [] -> Unit { [row] |> table.print(columns: ["name"]) }
proc defaults(row) [] -> Unit { [row] |> table.print() }
proc missing(row) [] -> Unit { [row] |> table.print(columns: ["absent"]) }
proc from_source(rows) [] -> Unit { rows |> table.print(columns: ["name"]) }
stream streamed() [] -> Stream[Row] { defer { print "closed" }; yield {name: "stream"} }

configured({name: "alpha", extra: [1, 2]})
defaults({name: "beta"})
missing({name: "unused"})
from_source([{name: "gamma", other: true}])
from_source([{name: "delta", other: "independent"}])
from_source(streamed())
print "table stages preserved"
