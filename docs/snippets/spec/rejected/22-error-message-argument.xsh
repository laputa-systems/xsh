error FetchError {
    Usage
    Rejected(url: Str, status: Int)
}

# begin example
let named = FetchError.Usage(message: "not a URL")  # error: check.error-constructor
let extra = FetchError.Usage("not a URL", "again")  # error: check.arity
# end example
