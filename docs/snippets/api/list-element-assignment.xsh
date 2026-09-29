var rows = [{name: "first", count: 1}, {name: "second", count: 2}]
let earlier = rows
rows[1].count += 3
rows[0] = {name: "updated", count: 7}
print ${rows[1].count}
print ${earlier[1].count}
