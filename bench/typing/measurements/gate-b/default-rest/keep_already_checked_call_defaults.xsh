pure next() -> Int { 4 }
pure explicit(jobs: Int = next()) -> Int { jobs }
pure inferred(jobs = next()) -> Int { jobs }
print ${explicit()} ${inferred()} ${inferred(9)}
