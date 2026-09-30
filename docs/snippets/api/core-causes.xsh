error SourceCauseError = Missing(message: Str) : NotFound
error BuildCauseError = CompileFailed(package: Str)

pure translate_build_error(failure: SourceCauseError) -> Result[Str, BuildCauseError] {
  Err(BuildCauseError.CompileFailed(package: "demo"), cause: failure)
}

let translated = translate_build_error(SourceCauseError.Missing(message: "source missing"))
if let Err(BuildCauseError.CompileFailed {package}) = translated {
  print $package
} else {
  print "unexpected success"
}
