use crate::errors::builtin_error_families;
use crate::records::record_schemas;
use crate::signature::api_spec;
use crate::types::Type;
use crate::{CORE_BUILTIN_SYMBOLS, FIXED_SEMANTIC_SYMBOLS};
use std::collections::BTreeSet;

pub fn preloaded_symbol_names() -> Vec<String> {
    let mut symbols = Vec::new();
    let mut seen = BTreeSet::new();
    for symbol in CORE_BUILTIN_SYMBOLS {
        insert_symbol(&mut symbols, &mut seen, symbol);
    }
    for symbol in semantic_symbol_names() {
        insert_symbol(&mut symbols, &mut seen, &symbol);
    }
    symbols
}

pub fn semantic_symbol_names() -> BTreeSet<String> {
    let mut symbols = BTreeSet::new();
    for symbol in FIXED_SEMANTIC_SYMBOLS {
        symbols.insert((*symbol).to_string());
    }
    collect_api_symbols(&mut symbols);
    collect_record_symbols(&mut symbols);
    collect_error_symbols(&mut symbols);
    symbols
}

fn collect_api_symbols(output: &mut BTreeSet<String>) {
    let spec = api_spec();
    for module in &spec.modules {
        output.insert(module.name.to_string());
        for function in &module.sig.functions {
            output.insert(function.name.to_string());
            for overload in &function.overloads {
                for param in &overload.params {
                    output.insert(param.name.to_string());
                    collect_type_symbols(&param.ty, output);
                }
                collect_type_symbols(&overload.return_ty, output);
            }
        }
    }
    for receiver in &spec.methods {
        for method in &receiver.methods {
            output.insert(method.name.to_string());
            for overload in &method.overloads {
                for param in &overload.sig.params {
                    output.insert(param.name.to_string());
                    collect_type_symbols(&param.ty, output);
                }
                collect_type_symbols(&overload.sig.return_ty, output);
                if let crate::signature::MethodReturn::Type(ty) = &overload.return_ty {
                    collect_type_symbols(ty, output);
                }
            }
        }
    }
}

fn collect_record_symbols(output: &mut BTreeSet<String>) {
    for (name, ty) in record_schemas() {
        output.insert(name.to_string());
        collect_type_symbols(&ty, output);
    }
}

fn collect_error_symbols(output: &mut BTreeSet<String>) {
    for family in builtin_error_families() {
        output.insert(family.name.to_string());
        for field in family.fields {
            output.insert(field.name.to_string());
            collect_type_symbols(&field.ty, output);
        }
        for variant in family.variants {
            output.insert(variant.name.to_string());
            for facet in variant.facets {
                output.insert((*facet).to_string());
            }
        }
    }
}

fn collect_type_symbols(ty: &Type, output: &mut BTreeSet<String>) {
    match ty {
        Type::List(inner) | Type::Map(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            collect_type_symbols(inner, output);
        }
        Type::Record(fields) | Type::Module(fields) => {
            for (name, ty) in fields {
                output.insert(name.clone());
                collect_type_symbols(ty, output);
            }
        }
        Type::Result(ok, err) => {
            collect_type_symbols(ok, output);
            collect_type_symbols(err, output);
        }
        Type::Any
        | Type::Unknown
        | Type::Invalid
        | Type::Null
        | Type::Bool
        | Type::Int
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Digest
        | Type::Regex
        | Type::Path
        | Type::Status
        | Type::EnvPathList
        | Type::Error
        | Type::ProcessError
        | Type::Pure
        | Type::Proc
        | Type::Command
        | Type::ProcessHandle
        | Type::Unit => {}
    }
}

fn insert_symbol(symbols: &mut Vec<String>, seen: &mut BTreeSet<String>, symbol: &str) {
    assert!(!symbol.is_empty(), "preloaded symbol cannot be empty");
    assert!(
        symbol.is_ascii(),
        "preloaded symbol `{symbol}` must be ASCII"
    );
    if seen.insert(symbol.to_string()) {
        symbols.push(symbol.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::{preloaded_symbol_names, semantic_symbol_names};
    use crate::errors::builtin_error_families;
    use crate::records::record_schemas;
    use crate::signature::api_spec;
    use crate::types::{BUILTIN_TYPE_NAMES, Type};
    use crate::{CORE_BUILTIN_SYMBOLS, FIXED_SEMANTIC_SYMBOLS};
    use std::collections::BTreeSet;

    #[test]
    fn preloaded_symbols_are_unique_nonempty_ascii() {
        let symbols = preloaded_symbol_names();
        let mut seen = BTreeSet::new();
        for symbol in &symbols {
            assert!(!symbol.is_empty());
            assert!(symbol.is_ascii(), "`{symbol}` must be ASCII");
            assert!(seen.insert(symbol), "`{symbol}` was preloaded twice");
        }
    }

    #[test]
    fn preloaded_symbols_start_with_fixed_core_symbols() {
        let symbols = preloaded_symbol_names();
        assert!(symbols.len() >= CORE_BUILTIN_SYMBOLS.len());
        for (actual, expected) in symbols.iter().zip(CORE_BUILTIN_SYMBOLS) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn preloaded_symbols_cover_builtin_type_names() {
        let symbols = preloaded_set();
        for name in BUILTIN_TYPE_NAMES {
            assert!(symbols.contains(name.as_str()));
        }
    }

    #[test]
    fn semantic_symbols_cover_registry_surfaces() {
        let symbols = semantic_symbol_names();
        for name in FIXED_SEMANTIC_SYMBOLS {
            assert!(symbols.contains(*name));
        }
        assert_api_symbols_are_present(&symbols);
        assert_record_symbols_are_present(&symbols);
        assert_error_symbols_are_present(&symbols);
    }

    fn preloaded_set() -> BTreeSet<String> {
        preloaded_symbol_names().into_iter().collect()
    }

    fn assert_api_symbols_are_present(symbols: &BTreeSet<String>) {
        let spec = api_spec();
        for module in &spec.modules {
            assert!(symbols.contains(module.name));
            for function in &module.sig.functions {
                assert!(symbols.contains(function.name));
                for overload in &function.overloads {
                    for param in &overload.params {
                        assert!(symbols.contains(param.name));
                        assert_type_symbols_are_present(symbols, &param.ty);
                    }
                    assert_type_symbols_are_present(symbols, &overload.return_ty);
                }
            }
        }
        for receiver in &spec.methods {
            for method in &receiver.methods {
                assert!(symbols.contains(method.name));
                for overload in &method.overloads {
                    for param in &overload.sig.params {
                        assert!(symbols.contains(param.name));
                        assert_type_symbols_are_present(symbols, &param.ty);
                    }
                    assert_type_symbols_are_present(symbols, &overload.sig.return_ty);
                    if let crate::signature::MethodReturn::Type(ty) = &overload.return_ty {
                        assert_type_symbols_are_present(symbols, ty);
                    }
                }
            }
        }
    }

    fn assert_record_symbols_are_present(symbols: &BTreeSet<String>) {
        for (name, ty) in record_schemas() {
            assert!(symbols.contains(name));
            assert_type_symbols_are_present(symbols, &ty);
        }
    }

    fn assert_error_symbols_are_present(symbols: &BTreeSet<String>) {
        for family in builtin_error_families() {
            assert!(symbols.contains(family.name));
            for field in family.fields {
                assert!(symbols.contains(field.name));
                assert_type_symbols_are_present(symbols, &field.ty);
            }
            for variant in family.variants {
                assert!(symbols.contains(variant.name));
                for facet in variant.facets {
                    assert!(symbols.contains(*facet));
                }
            }
        }
    }

    fn assert_type_symbols_are_present(symbols: &BTreeSet<String>, ty: &Type) {
        match ty {
            Type::List(inner) | Type::Map(inner) | Type::Stream(inner) | Type::Optional(inner) => {
                assert_type_symbols_are_present(symbols, inner);
            }
            Type::Record(fields) | Type::Module(fields) => {
                for (name, ty) in fields {
                    assert!(symbols.contains(name));
                    assert_type_symbols_are_present(symbols, ty);
                }
            }
            Type::Result(ok, err) => {
                assert_type_symbols_are_present(symbols, ok);
                assert_type_symbols_are_present(symbols, err);
            }
            Type::Any
            | Type::Unknown
            | Type::Invalid
            | Type::Null
            | Type::Bool
            | Type::Int
            | Type::Float
            | Type::Duration
            | Type::Str
            | Type::Bytes
            | Type::Digest
            | Type::Regex
            | Type::Path
            | Type::Status
            | Type::EnvPathList
            | Type::Error
            | Type::ProcessError
            | Type::Pure
            | Type::Proc
            | Type::Command
            | Type::ProcessHandle
            | Type::Unit => {}
        }
    }
}
