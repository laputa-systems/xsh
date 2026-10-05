error FetchError {
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
}

pure describe(error: FetchError) -> Str {
  match Err(error) {
    Err(FetchError.Usage {message}) => f"usage: {message}"
    Err(FetchError.Offline) => "offline"
    Err(FetchError.Rejected {url, status}) => f"{url} answered {status}"
    else => error.message
  }
}

# begin example
print FetchError.Usage("not a URL").message
print FetchError.Usage().message
print describe(FetchError.Usage("not a URL"))
print describe(FetchError.Offline("no route"))
print FetchError.Rejected("https://example.test", 503).message
# end example
