# regex-lite 0.1.9, vendored

Upstream's `std` build seeds its capture-name map from `getrandom`. When
`getrandom` returns `EIO`, an upstream consumer panics with status 101; this
copy completes and makes no `getrandom` call. Upstream 0.1.9 cannot disable
`std`. The two differences from upstream are the capture-name map and a
`usize::MAX` spelling that builds with warnings denied.

A standalone consumer (release opt3, thin LTO, five alternating cold builds)
measured build CPU 5.24 s against upstream's 5.21 s, peak RSS 298,180 against
299,848 KiB, and stripped size 529,600 against 538,416 bytes, all within the
spread. Licensing stays MIT or Apache-2.0 and no dependencies are added.

Drop this copy when upstream compiles regexes without an entropy source in a
supported configuration.
