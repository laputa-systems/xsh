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
