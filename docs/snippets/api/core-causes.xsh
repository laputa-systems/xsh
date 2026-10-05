error SourceCauseError = Missing : NotFound

error BuildCauseError = CompileFailed(package: Str)

pure translate_build_error(failure: SourceCauseError) -> Result[Str, BuildCauseError] {
  Err(.CompileFailed(package: "demo"), cause: failure)
}

let translated = translate_build_error(.Missing("source missing"))

if let Err(.CompileFailed {package: package}) = translated {
  print $package
} else {
  print "unexpected success"
}
