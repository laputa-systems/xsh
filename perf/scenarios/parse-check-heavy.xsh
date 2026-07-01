type Package = {
  name: Str,
  version: Int,
  language: Str,
  optional: Bool,
  deps: List[Str],
  metadata: Record,
}

type PackageScore = {
  name: Str,
  language: Str,
  score: Int,
  label: Str,
}

type Summary = {
  key: Str,
  count: Int,
  total: Int,
  first: Str,
}

pure package_for(index: Int) -> Package {
  let language = if index % 6 == 0 {
    "xsh"
  } else if index % 6 == 1 {
    "rust"
  } else if index % 6 == 2 {
    "python"
  } else if index % 6 == 3 {
    "typescript"
  } else if index % 6 == 4 {
    "shell"
  } else {
    "markdown"
  }

  return {
    name: f"pkg-${index % 257}",
    version: index % 31,
    language,
    optional: index % 11 == 0,
    deps: [f"dep-${index % 13}", f"dep-${(index + 5) % 17}", language],
    metadata: {
      source: f"src/${language}/file-${index % 101}.xsh",
      owner: if index % 2 == 0 { "core" } else { "tools" },
      generated: index % 29 == 0,
    },
  }
}

pure package_score(pkg: Package) -> PackageScore {
  let base = pkg.version * 23 + pkg.deps.len() * 17
  let optional_penalty = if pkg.optional { 5 } else { 0 }
  let generated_bonus = if pkg.metadata.generated { 19 } else { 0 }

  return {
    name: pkg.name,
    language: pkg.language,
    score: base + generated_bonus - optional_penalty,
    label: f"${pkg.metadata.owner}:${pkg.metadata.source}:${pkg.deps[0]}",
  }
}

pure summarize(scores: List[PackageScore]) -> List[Summary] {
  return scores
    |> group-by .language
    |> map { |bucket|
      {
        key: bucket.key,
        count: bucket.items.len(),
        total: bucket.items |> map .score |> sum,
        first: bucket.items[0].label,
      }
    }
    |> sort-by .key
    |> collect()
}

let packages: List[Package] = range(0, 7000)
  |> map { |index|
    package_for(index)
  }
  |> collect()

let scores: List[PackageScore] = packages
  |> where ! .optional or .version > 20
  |> map { |pkg|
    package_score(pkg)
  }
  |> sort-by .score
  |> collect()

let summary = summarize(scores)
var checksum = 0

for row in summary {
  checksum += row.key.byte_len()
  checksum += row.count * 29
  checksum += row.total % 12289
  checksum += row.first.count_chars()
}

print ${checksum % 100000}
