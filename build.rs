use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use xsh_registry::CORE_BUILTIN_SYMBOLS;
use xsh_registry::symbols::preloaded_symbol_names;

fn main() {
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/lib.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/errors.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/records.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/runtime_op.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/signature/mod.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/signature/modules.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/signature/methods.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/signature/builders.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/signature/streams.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/symbols.rs");
    println!("cargo:rerun-if-changed=crates/xsh-registry/src/types.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set"));
    write_symbols(&preloaded_symbol_names(), &root);
}

fn write_symbols(symbols: &[String], root: &Path) {
    let mut blob = Vec::new();
    let mut ranges = Vec::new();
    for symbol in symbols {
        let start = u32::try_from(blob.len()).expect("symbol blob exceeds u32 offsets");
        let len = u16::try_from(symbol.len()).expect("symbol exceeds u16 length");
        blob.extend_from_slice(symbol.as_bytes());
        ranges.push((start, len));
    }

    let mut output = String::new();
    output.push_str("#[repr(align(64))]\n");
    output
        .push_str("pub(crate) struct AlignedSymbolBytes<const N: usize>(pub(crate) [u8; N]);\n\n");
    output.push_str(&format!(
        "pub(crate) const CORE_BUILTIN_COUNT: u32 = {};\n",
        CORE_BUILTIN_SYMBOLS.len()
    ));
    output.push_str(&format!(
        "pub(crate) const PRELOADED_SYMBOL_COUNT: u32 = {};\n\n",
        symbols.len()
    ));
    output.push_str(&format!(
        "pub(crate) const PRELOADED_SYMBOL_TEXT: AlignedSymbolBytes<{}> = AlignedSymbolBytes(*b\"",
        blob.len()
    ));
    for byte in &blob {
        match *byte {
            b'\\' => output.push_str("\\\\"),
            b'"' => output.push_str("\\\""),
            byte if byte.is_ascii_graphic() || byte == b' ' => output.push(byte as char),
            byte => output.push_str(&format!("\\x{byte:02x}")),
        }
    }
    output.push_str("\");\n\n");
    output.push_str(&format!(
        "pub(crate) const PRELOADED_SYMBOL_RANGES: [(u32, u16); {}] = [\n",
        ranges.len()
    ));
    for (symbol, (start, len)) in symbols.iter().zip(ranges) {
        output.push_str(&format!("    ({start}, {len}), // {symbol}\n"));
    }
    output.push_str("];\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"));
    let path = out_dir.join("preloaded_symbols.rs");
    fs::write(&path, output)
        .unwrap_or_else(|err| panic!("failed to write '{}': {err}", path.display()));

    let _ = root;
}
