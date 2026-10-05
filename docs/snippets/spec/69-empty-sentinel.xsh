# begin example
pure describe(titles: Map[Str], key: Str) -> Str {
  if let Ok(title) = titles.get(key) {
    return f"title: {title}"
  }

  "untitled"
}

# end example

print describe({draft: ""}, "draft")
