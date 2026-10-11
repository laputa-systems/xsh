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
    build_long_double(&root);
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

// Match the C compiler and target flags selected by the repository build driver.
fn build_long_double(root: &Path) {
    use std::process::Command;
    let target = env::var("TARGET").expect("TARGET set");
    let normalized = target.replace('-', "_");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set"));
    let setting = |name: &str| {
        let names = [format!("{name}_{target}"), format!("{name}_{normalized}"),
            format!("{name}_{}", normalized.to_uppercase()), format!("TARGET_{name}"), name.to_string()];
        for key in names {
            println!("cargo:rerun-if-env-changed={key}");
            if let Ok(value) = env::var(&key) { return Some(value); }
        }
        None
    };
    let run = |tool: String, args: Vec<String>| {
        let words = c_build_words(&tool);
        let executable = words.first().expect("C build tool must not be empty");
        let status = Command::new(executable).args(&words[1..]).args(args).status()
            .unwrap_or_else(|error| panic!("cannot execute {executable}: {error}"));
        assert!(status.success(), "C numeric boundary build failed: {executable}");
    };
    let compiler = setting("CC").unwrap_or_else(|| "cc".to_string());
    let archiver = setting("AR").unwrap_or_else(|| "ar".to_string());
    let mut flags: Vec<String> = c_build_words(&setting("CFLAGS").unwrap_or_default());
    flags.extend(["-std=c11".into(), "-O2".into(), "-fPIC".into(), "-c".into(),
        root.join("src/modules/numeric/long_double.c").display().to_string(), "-o".into(), out.join("long_double.o").display().to_string()]);
    run(compiler, flags);
    run(archiver, vec!["crs".into(), out.join("libxsh_numeric.a").display().to_string(), out.join("long_double.o").display().to_string()]);
    println!("cargo:rerun-if-changed=src/modules/numeric/long_double.c");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=xsh_numeric");
    println!("cargo:rustc-link-lib=m");
}


fn c_build_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for character in input.chars() {
        if escaped { word.push(character); escaped = false; }
        else if character == '\\' && quote != Some('\'') { escaped = true; started = true; }
        else if quote == Some(character) { quote = None; }
        else if quote.is_none() && matches!(character, '\'' | '"') { quote = Some(character); started = true; }
        else if quote.is_none() && character.is_whitespace() {
            if started { words.push(std::mem::take(&mut word)); started = false; }
        } else { word.push(character); started = true; }
    }
    assert!(!escaped && quote.is_none(), "unterminated quote or escape in C build setting");
    if started { words.push(word); }
    words
}
