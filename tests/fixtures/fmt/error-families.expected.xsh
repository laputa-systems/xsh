error Short = Missing(file: Path) : NotFound | Invalid(file: Path, message: Str)

error Wide {
    Missing(file: Path) : NotFound
    Invalid(file: Path, message: Str)
    Rejected(url: Str, status: Int) : PermissionDenied, Timeout
    Usage
}

error Single {
    Usage
}

error Spaced {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

error Commented {
  # Why the request never left.
  Usage  # bad arguments
  Offline
}

error Trailing = Usage | Offline # a note

proc show(error: Error) {
  match Err(error) {
    Err(Wide.Rejected {url, status}) => print f"{url} {status}"
    Err(Spaced.Offline {message}) => print f"offline: {message}"
    Err(Single.Usage) => print "single"
    Err(Commented.Offline {}) => print "commented offline"
    Err(is NotFound) => print "missing"
    _ => print $error.message
  }
}

show(Short.Missing(p"conf"))
show(Wide.Rejected("https://example.test", 503))
show(Wide.Usage("wide usage"))
show(Single.Usage())
show(Spaced.Offline("no route"))
show(Commented.Usage("commented"))
show(Commented.Offline())
show(Trailing.Offline())
