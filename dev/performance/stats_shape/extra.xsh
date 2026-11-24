type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Int], extra: Int}
var total = 0
var kept: List[Stats] = []
for i in range(200000) {
  let record = Stats(blanks: i, code: i, comments: i, blobs: {}, extra: 0)
  total += record.blanks + record.code + record.comments
  if i % 1000 == 0 { kept += [record] }
}
print $total kept.len()
