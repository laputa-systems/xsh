//! Checking a loaded module against a module contract.
//!
//! The check collects every violation instead of stopping at the first, so one
//! failed `.require(Contract)` tells the author everything to fix. Each
//! violation category has its own error facet; a failure implements the facet
//! of every category it contains.

use super::{Evaluator, LoweredValue, ModuleExportSignature, Name, RuntimeError, Span};
use crate::sema::types::{CallableType, ModuleExportType};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;
use xsh_registry::errors::ErrorFacet;

/// One way a module fails its contract. A category is a variant here, with
/// its facet in [`ContractViolation::facet`] and its wording in
/// [`ContractViolation::describe`].
#[derive(Clone, Debug, Eq, PartialEq)]
enum ContractViolation {
    /// A required export is absent.
    Missing { expected: String },
    /// An export is present with another kind or signature.
    Mismatched {
        expected: String,
        found: String,
        reason: String,
    },
}

impl ContractViolation {
    fn facet(&self) -> ErrorFacet {
        match self {
            Self::Missing { .. } => ErrorFacet::MissingExport,
            Self::Mismatched { .. } => ErrorFacet::MismatchedExport,
        }
    }

    fn describe(&self, name: Name) -> String {
        match self {
            Self::Missing { expected } => {
                format!("missing export `{name}`: expected `{expected}`")
            }
            Self::Mismatched {
                expected,
                found,
                reason,
            } => format!(
                "mismatched export `{name}`: expected `{expected}`, found `{found}` ({reason})"
            ),
        }
    }
}

/// Every violation of `contract` by `module`, in export-name order. Empty
/// means the module satisfies the contract.
fn contract_violations(
    evaluator: &Evaluator,
    module: &BTreeMap<Arc<str>, LoweredValue>,
    contract: &BTreeMap<Name, ModuleExportType>,
) -> Vec<(Name, ContractViolation)> {
    let mut violations = Vec::new();
    for (name, expected) in contract {
        let Some(value) = module.get::<str>(name.as_str().as_str()) else {
            if !expected.optional() {
                violations.push((
                    *name,
                    ContractViolation::Missing {
                        expected: render_contract_entry(*name, expected),
                    },
                ));
            }
            continue;
        };
        if let Some(reason) = export_mismatch(evaluator, value, expected) {
            violations.push((
                *name,
                ContractViolation::Mismatched {
                    expected: render_contract_entry(*name, expected),
                    found: render_found_export(evaluator, *name, value),
                    reason,
                },
            ));
        }
    }
    violations
}

/// Why a present export does not satisfy its contract entry, or `None` when
/// it does. A callable whose signature was not captured at load satisfies any
/// entry of its own kind.
fn export_mismatch(
    evaluator: &Evaluator,
    value: &LoweredValue,
    expected: &ModuleExportType,
) -> Option<String> {
    match (expected, value) {
        (ModuleExportType::Value { ty, .. }, value) => {
            if matches!(value, LoweredValue::Proc(_) | LoweredValue::Pure(_)) {
                Some("the contract declares a value, the module exports a callable".to_string())
            } else if super::lowered_value_matches_static_type(value, ty) {
                None
            } else {
                Some("the value type differs".to_string())
            }
        }
        (
            ModuleExportType::Proc { sig, .. } | ModuleExportType::Pure { sig, .. },
            LoweredValue::Proc(function) | LoweredValue::Pure(function),
        ) => {
            let captured = evaluator.lookup_module_export_signature(*function);
            // The captured declaration decides purity when there is one; the
            // value's own kind stands in for a callable without a capture.
            let found_pure = captured.map_or(matches!(value, LoweredValue::Pure(_)), |captured| {
                captured.pure
            });
            match (matches!(expected, ModuleExportType::Pure { .. }), found_pure) {
                (false, true) => Some(
                    "the contract declares a proc, the module exports a pure function".to_string(),
                ),
                (true, false) => Some(
                    "the contract declares a pure function, the module exports a proc".to_string(),
                ),
                _ => captured.and_then(|captured| signature_mismatch(&captured.sig, sig)),
            }
        }
        (ModuleExportType::Proc { .. } | ModuleExportType::Pure { .. }, _) => {
            Some("the contract declares a callable, the module exports a value".to_string())
        }
    }
}

/// The first difference between an export's signature and its contract
/// entry: parameter count, each parameter's rest flag and type, the declared
/// effects (which must be the same set), then the return type.
fn signature_mismatch(found: &CallableType, expected: &CallableType) -> Option<String> {
    if found.params.len() != expected.params.len() {
        return Some(format!(
            "the contract declares {}, the export takes {}",
            parameter_count(expected.params.len()),
            parameter_count(found.params.len()),
        ));
    }
    for (index, (found, expected)) in found.params.iter().zip(&expected.params).enumerate() {
        let position = index + 1;
        if found.rest != expected.rest {
            return Some(format!(
                "parameter {position} is a rest parameter on one side only"
            ));
        }
        if !found.ty.matches_expected(&expected.ty) {
            return Some(format!(
                "parameter {position} has type {}, the contract declares {}",
                found.ty, expected.ty
            ));
        }
    }
    let same_effects = match (&found.effects, &expected.effects) {
        (None, None) => true,
        (Some(found), Some(expected)) => {
            found.iter().all(|effect| expected.contains(effect))
                && expected.iter().all(|effect| found.contains(effect))
        }
        _ => false,
    };
    if !same_effects {
        return Some(format!(
            "the effects are {}, the contract declares {}",
            render_effects(found).unwrap_or_else(|| "undeclared".to_string()),
            render_effects(expected).unwrap_or_else(|| "none".to_string()),
        ));
    }
    if !found.return_ty.matches_expected(&expected.return_ty) {
        return Some(format!(
            "the return type is {}, the contract declares {}",
            found.return_ty, expected.return_ty
        ));
    }
    None
}

fn parameter_count(count: usize) -> String {
    if count == 1 {
        "1 parameter".to_string()
    } else {
        format!("{count} parameters")
    }
}

fn render_effects(sig: &CallableType) -> Option<String> {
    sig.effects.as_ref().map(|effects| {
        format!(
            "[{}]",
            effects
                .iter()
                .map(|effect| effect.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn render_callable(keyword: &str, name: Name, sig: &CallableType) -> String {
    let mut text = format!("export {keyword} {name}(");
    for (index, param) in sig.params.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        if param.rest {
            text.push_str("...");
        }
        let _ = write!(text, "{}: {}", param.name, param.ty);
    }
    text.push(')');
    if let Some(effects) = render_effects(sig) {
        let _ = write!(text, " {effects}");
    }
    let _ = write!(text, " -> {}", sig.return_ty);
    text
}

fn render_contract_entry(name: Name, entry: &ModuleExportType) -> String {
    match entry {
        ModuleExportType::Value { ty, .. } => format!("export let {name}: {ty}"),
        ModuleExportType::Proc { sig, .. } => render_callable("proc", name, sig),
        ModuleExportType::Pure { sig, .. } => render_callable("pure", name, sig),
    }
}

/// The export as the module declares it, as far as the loaded value records:
/// a callable's captured signature, or a value's runtime type.
fn render_found_export(evaluator: &Evaluator, name: Name, value: &LoweredValue) -> String {
    let (value_keyword, function) = match value {
        LoweredValue::Proc(function) => ("proc", function),
        LoweredValue::Pure(function) => ("pure", function),
        value => return format!("export let {name}: {}", value.type_name()),
    };
    match evaluator.lookup_module_export_signature(*function) {
        Some(ModuleExportSignature { pure, sig }) => {
            render_callable(if *pure { "pure" } else { "proc" }, name, sig)
        }
        None => format!("export {value_keyword} {name}"),
    }
}

/// Checks `module` against `contract`. The failure is a `schema` error at
/// `path` whose message lists every violation and whose facets are those of
/// the violation categories present.
pub(super) fn require_module_contract(
    evaluator: &Evaluator,
    module: &BTreeMap<Arc<str>, LoweredValue>,
    contract: &BTreeMap<Name, ModuleExportType>,
    path: &str,
    span: Span,
) -> Result<(), RuntimeError> {
    let violations = contract_violations(evaluator, module, contract);
    if violations.is_empty() {
        return Ok(());
    }
    let mut facets: Vec<String> = Vec::new();
    for (_, violation) in &violations {
        let facet = violation.facet().name();
        if !facets.iter().any(|known| known == facet) {
            facets.push(facet.to_string());
        }
    }
    let details = violations
        .iter()
        .map(|(name, violation)| violation.describe(*name))
        .collect::<Vec<_>>()
        .join("; ");
    let mut error = RuntimeError::new(
        "schema",
        format!("schema check failed at {path}: module does not satisfy its contract: {details}"),
    )
    .with_span(span);
    error.facets = facets;
    Err(error)
}
