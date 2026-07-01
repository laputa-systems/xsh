# Chapter 6: Text, Bytes, And Hashes

Text and bytes are different kinds of data. XSH keeps that boundary visible
with `Str`, `Bytes`, and conversion APIs that return `Result` when data is not
valid in the requested shape.

By the end of this chapter, you will have the pieces for a manifest-style
script: clean text when it is text, copy and inspect bytes when bytes matter,
hash file contents, and encode the final manifest only after the structured
work is done.

## Clean Text Deliberately

Configuration, logs, and command output are usually text. `Str` methods cover
the chores that appear around them: fields, splitting, joining, replacement,
counting, reversing, translating, deleting, squeezing, wrapping, and parsing.

```xsh
let sample = """alpha,beta,,gamma
last line"""

let fields = " alpha  beta\tgamma ".fields()
let csv = "alpha,beta,,gamma".split(",") |> where . != ""
let split = "ab".split("")
let joined = csv.join("|")
let rewritten = sample.replace("beta", "B")
let reversed = "stressed".reverse()
let slug = "alpha beta_gamma".translate(" _", "--")
let cleaned = "a-b_c".delete("-_")
let squeezed = "nooo   way".squeeze(chars: " o")
let wrapped = "alpha beta gamma".wrap(10)
let utf8_text = "h\u{e9}!"
print fields[1] csv[2] split[1] $joined
print rewritten.count_lines() sample.count_words() "h\u{e9}".count_chars() "h\u{e9}".count_bytes() $reversed
print $slug $cleaned $squeezed wrapped[1]
print utf8_text.byte_len() utf8_text.byte_at(1) utf8_text.byte_slice(1, 2) utf8_text.find("!")
```

This example is intentionally broad. It shows the small transformations that
normally hide inside shell pipelines. In XSH, separators and character sets are
named at the call site, and parsing failures can stay in the `Result` path with
helpers such as `.parse_int()?`.

Why XSH shines here: a cleanup step can remain readable without turning into a
chain of opaque flags.

Compared with bash and CLI tools: `tr`, `cut`, `sed`, and `awk` are excellent
at text filters. XSH is better when those filters sit inside a larger script
that also needs typed paths, checked errors, JSON, or records.

Common trap: character count and byte count are not the same thing. The example
uses Unicode so `count_chars` and `count_bytes` produce different answers.
Byte-indexed helpers such as `byte_at`, `byte_slice`, and `find` are for
ASCII-oriented scanners; use character-oriented methods for display text.

Do not use `Str` methods for byte protocols, archives, compressed data, or
unknown encodings. Decode only when you know the bytes are text.

## Keep Binary Work Binary

Some data should not become text: fixed-size chunks, encoded blobs, copied
blocks, NUL bytes, and invalid UTF-8.

```xsh
let encoded = b"\0hello\xff".base64()
let roundtrip = encoded.base64_decode()?

let spaced = """Y
WJj""".base64_decode()?

let b32 = b"foobar".base32()
let b32_roundtrip = "mzxw6ytboi======".base32_decode()?
let data = b"\0hello marker-one\0xx marker-two!!\xff"
let header = data.slice(1, 5)
let markers = data.strings(7)
let header_dump = header.dump("hex-u8")
let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let source = fp"${root}/source.bin"
let dest = fp"${root}/dest.bin"
fs.write(source, b"0123456789abcdef")
let copied = bytes.copy(source, dest, block_size: 3, count: 2, skip: 1, seek: 0, overwrite: false)?
let copied_data = dest.read_bytes()?
let decoded = b"alpha\nbeta".utf8()?
let lines = decoded.lines().collect()
let same = b"abc".compare(b"abc")
let comparison = b"abc\nxyz".compare(b"abc\nxqz")
let eof = b"abc".compare(b"abcd")
let invalid = b"\xff".utf8()
let invalid_base64 = "%%%".base64_decode()
let invalid_base32 = "M!".base32_decode()

match invalid {
  Err(e) => {
    match invalid_base64 {
      Err(b64) => {
        match invalid_base32 {
          Err(b32_err) => {
            print $encoded ${roundtrip == b"\0hello\xff"} ${spaced == b"abc"}
            print $b32 ${b32_roundtrip == b"foobar"}
            print data.len() ${header == b"hello"} markers[0] markers[1]
            print $header_dump
            print $copied.bytes $copied.blocks ${copied_data == b"345678"}
            print lines[1] $same.equal $comparison.equal $comparison.byte $comparison.line $comparison.left $comparison.right $eof.byte $eof.left $eof.right $e.message $b64.message $b32_err.message
          }
        }
      }
    }
  }
}
```

The example round-trips base64 and base32, extracts strings from binary data,
prints a byte dump, copies part of a file in fixed-size blocks, compares byte
sequences, and checks invalid encodings as ordinary `Err` values.

Why XSH shines here: binary data keeps its type until a conversion is
requested. If conversion fails, the error is structured instead of becoming
partial text.

Compared with bash and CLI tools: commands like `base64`, `dd`, and `cmp` are
still useful process boundaries. XSH's value is that binary results do not have
to masquerade as shell strings before the next check.

## Build A Hash Manifest

Hashing often combines everything from the last two sections: paths, byte
reads, digest formatting, sorted records, and JSON output.

```xsh
let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
fs.write(fp"${root}/a.txt", b"abc")?
fs.write(fp"${root}/b.txt", b"")?

let manifest = fs.children(root)
  |> where .kind == "file"
  |> map { |entry|
    let data = entry.path.read_bytes()?
    {path: entry.path.strip_prefix(root)?.display(), sha256: data.sha256().hex(), size: entry.size}
  }
  |> sort-by .path

let manifest_json = json.encode(manifest)?
hash.verify_file(fp"${root}/a.txt", sha256: b"abc".sha256().hex())
let parsed = hash.parse_check_line(f"${manifest[0].sha256}  ${manifest[0].path}")?
print ${manifest |> count()} manifest[0].path manifest[0].sha256
print $parsed.path $parsed.binary ${"\"b.txt\"" in manifest_json}
```

The manifest pipeline walks a temporary tree, reads bytes from each file,
hashes those bytes, records relative paths and sizes, sorts by path, and
encodes the result as JSON.

Why XSH shines here: paths, bytes, digests, and JSON records stay separate
until the exact boundary where each representation is needed.

Do not hash display strings when the contract is file content. Read bytes and
hash bytes; format the digest only after the hash is computed.

## What You Know Now

Use `Str` when the data is known UTF-8 text. Use `Bytes` when the data is
binary or may be invalid text. Use hashes over bytes, then format the digest at
the boundary where another tool or person needs to read it. The next chapter
uses the same discipline for JSON and network data.
