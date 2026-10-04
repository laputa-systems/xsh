//! Methods the generator calls with reference semantics, and the registry
//! check that keeps their signatures honest.
//!
//! The table names a signature shape in terms of the receiver's element,
//! key, and value parameters. [`verify_against_registry`] derives each shape
//! from the standard API registry, so a registry change that the generator
//! does not know about fails loudly instead of producing ill-typed programs.

use xsh_registry::signature::{MethodReceiver, api_spec};
use xsh_registry::types::{BuiltinTypeParameter, Type};

/// A type in a method shape. `T` is the List element, `K`/`V` the Map key and
/// value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    Int,
    Float,
    Str,
    Bool,
    Bytes,
    T,
    K,
    V,
    ListT,
    ListK,
    ListV,
    ListStr,
    MapKV,
    ResT,
    ResV,
}

#[derive(Clone, Copy, Debug)]
pub struct OracleMethod {
    pub receiver: MethodReceiver,
    pub name: &'static str,
    /// The parameters the generator passes: a prefix of the declared ones
    /// that covers every required parameter.
    pub params: &'static [Shape],
    pub ret: Shape,
}

const fn method(
    receiver: MethodReceiver,
    name: &'static str,
    params: &'static [Shape],
    ret: Shape,
) -> OracleMethod {
    OracleMethod {
        receiver,
        name,
        params,
        ret,
    }
}

pub const ORACLE_METHODS: &[OracleMethod] = &[
    method(MethodReceiver::Str, "upper", &[], Shape::Str),
    method(MethodReceiver::Str, "lower", &[], Shape::Str),
    method(MethodReceiver::Str, "trim", &[], Shape::Str),
    method(MethodReceiver::Str, "reverse", &[], Shape::Str),
    method(MethodReceiver::Str, "count_chars", &[], Shape::Int),
    method(MethodReceiver::Str, "byte_len", &[], Shape::Int),
    method(
        MethodReceiver::Str,
        "starts_with",
        &[Shape::Str],
        Shape::Bool,
    ),
    method(MethodReceiver::Str, "ends_with", &[Shape::Str], Shape::Bool),
    method(
        MethodReceiver::Str,
        "replace",
        &[Shape::Str, Shape::Str],
        Shape::Str,
    ),
    method(MethodReceiver::Str, "split", &[Shape::Str], Shape::ListStr),
    method(MethodReceiver::Int, "float", &[], Shape::Float),
    method(MethodReceiver::Float, "abs", &[], Shape::Float),
    method(MethodReceiver::List, "len", &[], Shape::Int),
    method(MethodReceiver::List, "get", &[Shape::Int], Shape::ResT),
    method(MethodReceiver::List, "join", &[Shape::Str], Shape::Str),
    method(MethodReceiver::List, "push", &[Shape::T], Shape::ListT),
    method(
        MethodReceiver::List,
        "extend",
        &[Shape::ListT],
        Shape::ListT,
    ),
    method(MethodReceiver::Map, "len", &[], Shape::Int),
    method(MethodReceiver::Map, "get", &[Shape::K], Shape::ResV),
    method(MethodReceiver::Map, "keys", &[], Shape::ListK),
    method(MethodReceiver::Map, "values", &[], Shape::ListV),
    method(
        MethodReceiver::Map,
        "set",
        &[Shape::K, Shape::V],
        Shape::MapKV,
    ),
    method(MethodReceiver::Map, "remove", &[Shape::K], Shape::MapKV),
    method(MethodReceiver::Bytes, "len", &[], Shape::Int),
    method(
        MethodReceiver::Bytes,
        "starts_with",
        &[Shape::Bytes],
        Shape::Bool,
    ),
];

fn shape_of(ty: &Type) -> Option<Shape> {
    use BuiltinTypeParameter::{Element, Key, Receiver, Value};
    Some(match ty {
        Type::Int => Shape::Int,
        Type::Float => Shape::Float,
        Type::Str => Shape::Str,
        Type::Bool => Shape::Bool,
        Type::Bytes => Shape::Bytes,
        Type::BuiltinParameter(Element) => Shape::T,
        Type::BuiltinParameter(Key) => Shape::K,
        Type::BuiltinParameter(Value) => Shape::V,
        Type::BuiltinParameter(Receiver) => Shape::MapKV,
        Type::List(inner) => match shape_of(inner)? {
            Shape::T => Shape::ListT,
            Shape::K => Shape::ListK,
            Shape::V => Shape::ListV,
            Shape::Str => Shape::ListStr,
            _ => return None,
        },
        Type::Map(key, value)
            if **key == Type::BuiltinParameter(Key) && **value == Type::BuiltinParameter(Value) =>
        {
            Shape::MapKV
        }
        Type::Result(ok, err) if **err == Type::Error => match shape_of(ok)? {
            Shape::T => Shape::ResT,
            Shape::V => Shape::ResV,
            _ => return None,
        },
        _ => return None,
    })
}

/// Checks every oracle method against the registry: it must exist, be pure,
/// and have exactly the required parameters and result the table states.
pub fn verify_against_registry() -> Result<(), String> {
    let spec = api_spec();
    for entry in ORACLE_METHODS {
        let receiver = spec
            .methods
            .iter()
            .find(|receiver| receiver.receiver == entry.receiver)
            .ok_or_else(|| format!("registry has no {:?} receiver", entry.receiver))?;
        let named = receiver
            .methods
            .iter()
            .find(|method| method.name == entry.name)
            .ok_or_else(|| format!("registry has no {:?}.{}", entry.receiver, entry.name))?;
        let matched = named.overloads.iter().any(|overload| {
            let sig = &overload.sig;
            // The table supplies a prefix of the parameters that covers
            // every required one; later defaulted parameters are omitted.
            let required = sig.params.iter().filter(|param| !param.defaulted).count();
            sig.pure
                && entry.params.len() >= required
                && entry.params.len() <= sig.params.len()
                && sig
                    .params
                    .iter()
                    .zip(entry.params)
                    .all(|(param, expected)| shape_of(&param.ty) == Some(*expected))
                && shape_of(&sig.return_ty) == Some(entry.ret)
        });
        if !matched {
            return Err(format!(
                "registry signature of {:?}.{} no longer matches the generator's {:?} -> {:?}: {:?}",
                entry.receiver,
                entry.name,
                entry.params,
                entry.ret,
                named
                    .overloads
                    .iter()
                    .map(|overload| (&overload.sig.params, &overload.sig.return_ty))
                    .collect::<Vec<_>>()
            ));
        }
    }
    Ok(())
}
