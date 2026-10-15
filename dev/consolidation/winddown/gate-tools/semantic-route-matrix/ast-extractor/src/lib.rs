use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Block, Expr, File, ImplItemFn, ItemFn, Pat, Stmt, Token};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagBranch {
    pub tag: String,
    pub pattern: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackBranch {
    pub pattern: String,
    pub outcome: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatementInventory {
    pub owner: String,
    pub match_line: usize,
    pub schema_tags: Vec<String>,
    pub branches: Vec<TagBranch>,
    pub fallbacks: Vec<FallbackBranch>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarSchemaRow {
    pub family: String,
    pub kind: String,
    pub view: String,
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarViewArm {
    pub family: String,
    pub variant: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarRouteCheck {
    pub family: String,
    pub line: usize,
    pub statement_index: usize,
    pub wrapper: String,
    pub result_variant: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarFrameGroup {
    pub tags: Vec<String>,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarFallback {
    pub applies_to: Vec<String>,
    pub scope: String,
    pub outcome: String,
    pub source: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScalarWrongUnion {
    pub family: String,
    pub outcome: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedScalarInventory {
    pub schema_rows: Vec<ScalarSchemaRow>,
    pub classifiers: BTreeMap<String, Vec<TagBranch>>,
    pub normal_checks: Vec<ScalarRouteCheck>,
    pub forced_entry: ScalarRouteCheck,
    pub view_arms: Vec<ScalarViewArm>,
    pub frame_groups: Vec<ScalarFrameGroup>,
    pub fallbacks: Vec<ScalarFallback>,
    pub wrong_union_errors: Vec<ScalarWrongUnion>,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomSchemaRow {
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub effects: String,
    pub class: String,
    pub strategy: String,
    pub classification: Option<String>,
    pub fields: Vec<EagerAtomSchemaField>,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomSchemaField {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomRoute {
    pub mode: String,
    pub line: usize,
    pub helper_line: usize,
    pub statement_index: usize,
    pub generic_line: usize,
    pub generic_statement_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomViewArm {
    pub variant: String,
    pub tag: String,
    pub input: String,
    pub transformation: String,
    pub output: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomFallback {
    pub scope: String,
    pub outcome: String,
    pub source: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomHelperBoundary {
    pub kind: String,
    pub tags: Vec<String>,
    pub outcome: String,
    pub references: Vec<EagerAtomSourceRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomSourceRef {
    pub source: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomValueContract {
    pub decode_error: String,
    pub path_error: String,
    pub slot_bounds: String,
    pub freeze: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomFreezeContract {
    pub line: usize,
    pub shared_threshold: String,
    pub threshold_line: usize,
    pub list_take_line: usize,
    pub shared_list_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EagerAtomInventory {
    pub schema_rows: Vec<EagerAtomSchemaRow>,
    pub default_false_line: usize,
    pub eager_true_line: usize,
    pub expansion_line: usize,
    pub normal_route: EagerAtomRoute,
    pub forced_route: EagerAtomRoute,
    pub helper_arms: Vec<EagerAtomViewArm>,
    pub helper_fallback: EagerAtomFallback,
    pub explicit_error_group: EagerAtomFallback,
    pub helper_boundaries: Vec<EagerAtomHelperBoundary>,
    pub slot_decode_line: usize,
    pub slot_decode_bounds_line: usize,
    pub freeze_contract: EagerAtomFreezeContract,
    pub value_contract: EagerAtomValueContract,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameConstructorSchemaRow {
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub effects: String,
    pub class: String,
    pub strategy: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameConstructorRoute {
    pub mode: String,
    pub line: usize,
    pub helper_line: usize,
    pub statement_index: usize,
    pub fallback_line: usize,
    pub fallback_statement_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameConstructorArm {
    pub helper: String,
    pub variant: String,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameConstructorFallback {
    pub helper: String,
    pub outcome: String,
    pub scope: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameConstructorInventory {
    pub schema_rows: Vec<FrameConstructorSchemaRow>,
    pub classifiers: BTreeMap<String, Vec<TagBranch>>,
    pub normal_route: FrameConstructorRoute,
    pub forced_route: FrameConstructorRoute,
    pub child_arms: Vec<FrameConstructorArm>,
    pub value_arms: Vec<FrameConstructorArm>,
    pub helper_fallbacks: Vec<FrameConstructorFallback>,
    pub explicit_error_group: FrameConstructorFallback,
    pub value_decode_line: usize,
    pub value_boundaries: BTreeMap<String, usize>,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionDispatchArm {
    pub tag: String,
    pub variant: String,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionFallback {
    pub helper: String,
    pub outcome: String,
    pub scope: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionForcedRoute {
    pub frame_apply_line: usize,
    pub frame_apply_statement_index: usize,
    pub view_kind_line: usize,
    pub view_kind_statement_index: usize,
    pub eval_expr_view_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionConstructionInventory {
    pub schema_rows: Vec<FrameConstructorSchemaRow>,
    pub frame_apply_classifier_line: usize,
    pub frame_apply_classifier_tags: Vec<String>,
    pub normal_arms: Vec<CollectionDispatchArm>,
    pub forced_arms: Vec<CollectionDispatchArm>,
    pub forced_route: CollectionForcedRoute,
    pub normal_view_fallbacks: Vec<CollectionFallback>,
    pub normal_fallback: CollectionFallback,
    pub helper_fallbacks: Vec<CollectionFallback>,
    pub explicit_error_group: CollectionFallback,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultConstructorSchemaRow {
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub effects: String,
    pub class: String,
    pub strategy: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultConstructorArm {
    pub tag: String,
    pub variant: String,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultConstructorFallback {
    pub helper: String,
    pub outcome: String,
    pub scope: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultConstructorRoute {
    pub frame_apply_line: usize,
    pub frame_apply_statement_index: usize,
    pub view_kind_line: usize,
    pub view_kind_statement_index: usize,
    pub eval_expr_view_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultConstructorInventory {
    pub schema_rows: Vec<ResultConstructorSchemaRow>,
    pub normal_arms: Vec<ResultConstructorArm>,
    pub normal_view_fallbacks: Vec<ResultConstructorFallback>,
    pub normal_fallback: ResultConstructorFallback,
    pub forced_arms: Vec<ResultConstructorArm>,
    pub forced_view_fallbacks: Vec<ResultConstructorFallback>,
    pub explicit_error_group: ResultConstructorFallback,
    pub forced_route: ResultConstructorRoute,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFrameSchemaRow {
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub effects: String,
    pub class: String,
    pub strategy: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFrameArm {
    pub tag: String,
    pub variant: String,
    pub view_group: Vec<String>,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFrameFallback {
    pub helper: String,
    pub outcome: String,
    pub scope: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFrameRoute {
    pub scheduled_predicate_line: usize,
    pub frame_schedule_line: usize,
    pub frame_helper_line: usize,
    pub inner_helper_line: usize,
    pub view_kind_line: usize,
    pub eval_expr_view_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallFrameInventory {
    pub schema_rows: Vec<CallFrameSchemaRow>,
    pub normal_route: CallFrameRoute,
    pub forced_arms: Vec<CallFrameArm>,
    pub typed_view_fallbacks: Vec<CallFrameFallback>,
    pub explicit_error_group: CallFrameFallback,
    pub normal_fallback: CallFrameFallback,
    pub source_paths: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexSchemaRow {
    pub variant: String,
    pub tag: String,
    pub ordinal: u16,
    pub effects: String,
    pub class: String,
    pub strategy: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexArm {
    pub tag: String,
    pub variant: String,
    pub behavior: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexFallback {
    pub helper: String,
    pub outcome: String,
    pub scope: String,
    pub line: usize,
    pub arm_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexRoute {
    pub frame_apply_line: usize,
    pub frame_apply_statement_index: usize,
    pub view_kind_line: usize,
    pub view_kind_statement_index: usize,
    pub eval_expr_view_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexNormalRoute {
    pub scheduled_predicate_line: usize,
    pub frame_schedule_line: usize,
    pub frame_helper_line: usize,
    pub inner_helper_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIndexInventory {
    pub schema_rows: Vec<FieldIndexSchemaRow>,
    pub normal_route: FieldIndexNormalRoute,
    pub frame_apply_classifier_line: usize,
    pub normal_arms: Vec<FieldIndexArm>,
    pub normal_view_fallbacks: Vec<FieldIndexFallback>,
    pub normal_fallback: FieldIndexFallback,
    pub forced_arms: Vec<FieldIndexArm>,
    pub forced_view_fallbacks: Vec<FieldIndexFallback>,
    pub forced_route: FieldIndexRoute,
    pub explicit_error_group: FieldIndexFallback,
    pub source_paths: BTreeMap<String, String>,
}

const INT_TAGS: &[&str] = &[
    "IntInt",
    "IntSlot",
    "IntBinary",
    "IntStrByteLenSlot",
    "IntStrCountLinesSlot",
    "IntStrByteAtSlot",
];

const BOOL_TAGS: &[&str] = &[
    "BoolBool",
    "BoolSlot",
    "BoolNot",
    "BoolAnd",
    "BoolOr",
    "BoolIntCompare",
    "BoolStrPredicateSlot",
    "BoolContainsSlot",
    "BoolStrContainsSlot",
    "BoolTrimEmptySlot",
    "BoolTrimStrPredicateSlot",
    "BoolLiteralCompareSlot",
];

const EAGER_ATOM_ROWS: &[(&str, &str, u16)] = &[
    ("Null", "ExprNull", 18),
    ("Unit", "ExprUnit", 19),
    ("Int", "ExprInt", 20),
    ("Float", "ExprFloat", 21),
    ("Duration", "ExprDuration", 22),
    ("Bool", "ExprBool", 23),
    ("Str", "ExprStr", 24),
    ("Bytes", "ExprBytes", 25),
    ("PreparedRegex", "ExprPreparedRegex", 26),
    ("PreparedConstant", "ExprPreparedConstant", 27),
    ("Path", "ExprPath", 28),
    ("Param", "ExprParam", 31),
];

const EAGER_VIEW_VARIANTS: &[&str] = &[
    "Null",
    "Unit",
    "Int",
    "Float",
    "Duration",
    "Bool",
    "Str",
    "Bytes",
    "PreparedRegex",
    "PreparedConstant",
    "Path",
    "Param",
];

fn expected_eager_schema_fields(variant: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match variant {
        "Null" | "Unit" => Some(&[]),
        "Int" => Some(&[("value", "i64")]),
        "Float" => Some(&[("value", "FloatValue")]),
        "Duration" => Some(&[("value", "DurationValue")]),
        "Bool" => Some(&[("value", "bool")]),
        "Str" => Some(&[("value", "Arc<str>")]),
        "Bytes" => Some(&[("value", "Arc<[u8]>")]),
        "PreparedRegex" => Some(&[("value", "RegexValue")]),
        "PreparedConstant" => Some(&[("value", "PreparedConstantValue")]),
        "Path" => Some(&[("value", "PathValue")]),
        "Param" => Some(&[("slot", "usize")]),
        _ => None,
    }
}

fn parse_eager_schema_fields(row: &ParsedExprSchemaRow) -> Result<Vec<EagerAtomSchemaField>, String> {
    let Some(payload) = &row.payload else {
        return Ok(Vec::new());
    };
    let mut fields = Vec::new();
    let mut field_tokens = Vec::new();
    let mut angle_depth = 0usize;
    for token in payload.clone() {
        if punct_is(&token, '<') {
            angle_depth += 1;
        } else if punct_is(&token, '>') {
            angle_depth = angle_depth.saturating_sub(1);
        }
        if punct_is(&token, ',') && angle_depth == 0 {
            push_eager_schema_field(&mut fields, std::mem::take(&mut field_tokens), &row.variant)?;
        } else {
            field_tokens.push(token);
        }
    }
    push_eager_schema_field(&mut fields, field_tokens, &row.variant)?;
    Ok(fields)
}

fn push_eager_schema_field(
    fields: &mut Vec<EagerAtomSchemaField>,
    tokens: Vec<TokenTree>,
    variant: &str,
) -> Result<(), String> {
    if tokens.is_empty() {
        return Ok(());
    }
    if tokens.len() < 3 || !punct_is(&tokens[1], ':') {
        return Err(format!("EagerAtom schema row {variant} has an unrecognized field"));
    }
    let name = ident_value(tokens.first())
        .ok_or_else(|| format!("EagerAtom schema row {variant} has an unnamed field"))?;
    let ty = compact_tokens(&TokenStream::from_iter(tokens.into_iter().skip(2)));
    if ty.is_empty() {
        return Err(format!("EagerAtom schema field {variant}.{name} has no type"));
    }
    fields.push(EagerAtomSchemaField { name, ty });
    Ok(())
}

const FRAME_CONSTRUCTOR_ROWS: &[(&str, &str, u16)] = &[
    ("FunctionRef", "ExprFunctionRef", 29),
    ("PathFrom", "ExprPathFrom", 30),
    ("EmptyMap", "ExprEmptyMap", 49),
];

const COLLECTION_CONSTRUCTION_ROWS: &[(&str, &str, u16, &str)] = &[
    ("Record", "ExprRecord", 44, "FrameControl"),
    ("MapLiteral", "ExprMapLiteral", 45, "FrameControl"),
    ("RecordUpdate", "ExprRecordUpdate", 46, "FrameControl"),
    ("List", "ExprList", 47, "FrameApply"),
    ("ListBuild", "ExprListBuild", 48, "FrameApply"),
];

const RESULT_CONSTRUCTOR_ROWS: &[(&str, &str, u16)] = &[
    ("Ok", "ExprOk", 104),
    ("Err", "ExprErr", 105),
];

const CALL_FRAME_ROWS: &[(&str, &str, u16, &str)] = &[
    ("Call", "ExprCall", 108, "EFFECT_TRACE"),
    ("DirectPureCall", "ExprDirectPureCall", 109, "EFFECT_TRACE"),
    ("ExternalCall", "ExprExternalCall", 110, "EFFECT_TRACE"),
    ("DynamicCall", "ExprDynamicCall", 111, "EFFECT_DYNAMIC_CALL|EFFECT_TRACE"),
];

const FIELD_INDEX_ROWS: &[(&str, &str, u16)] = &[
    ("Field", "ExprField", 56),
    ("Index", "ExprIndex", 57),
    ("IndexFromEnd", "ExprIndexFromEnd", 58),
];

const FRAME_APPLY_TAGS: &[&str] = &[
    "ExprFunctionRef",
    "ExprPathFrom",
    "ExprEmptyMap",
    "ExprRange",
    "ExprTag",
    "ExprSlice",
    "ExprStrByteLen",
    "ExprStrByteAt",
    "ExprStrPredicate",
    "ExprGlob",
    "ExprLastStatus",
    "ExprAbort",
    "ExprFail",
    "ExprError",
    "ExprProcessCommandArgv",
    "ExprFsFiles",
    "ExprFsWalk",
    "ExprFsList",
    "ExprFsTempDir",
    "ExprFsMkdir",
    "ExprFsRemove",
    "ExprPathReadText",
    "ExprPathReadBytes",
    "ExprPathExists",
    "ExprPathExecutable",
    "ExprPathDu",
    "ExprPathMetadata",
    "ExprPathReadlink",
    "ExprPathResolve",
    "ExprPathWrite",
    "ExprPathMkdir",
    "ExprPathRemove",
    "ExprArchiveTarCreate",
];

const FRAME_PREPARED_OPERAND_TAGS: &[&str] = &[
    "ExprFsFiles",
    "ExprFsWalk",
    "ExprFsList",
    "ExprFsMkdir",
    "ExprFsRemove",
    "ExprPathWrite",
    "ExprPathMkdir",
    "ExprPathRemove",
    "ExprArchiveTarCreate",
];

pub fn extract_typed_scalar_inventory(
    schema_source: &str,
    indexed_source: &str,
    values_source: &str,
    explicit_source: &str,
    schema_path: &str,
    indexed_path: &str,
    values_path: &str,
    explicit_path: &str,
) -> Result<TypedScalarInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let values = syn::parse_file(values_source)
        .map_err(|error| format!("parse typed scalar values: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;

    let schema_rows = typed_scalar_schema_rows(&schema)?;
    let classifiers = BTreeMap::from([
        (
            "Int".to_string(),
            parse_typed_classifier(&indexed, "is_typed_int_tag", "Int", INT_TAGS)?,
        ),
        (
            "Bool".to_string(),
            parse_typed_classifier(&indexed, "is_typed_bool_tag", "Bool", BOOL_TAGS)?,
        ),
    ]);
    validate_scalar_route_checks(&indexed)?;
    let view_arms = typed_scalar_view_arms(&values, &schema_rows)?;
    let (forced_entry, frame_groups, fallbacks) = explicit_scalar_groups(&explicit)?;
    let wrong_union_errors = typed_union_errors(&values)?;

    Ok(TypedScalarInventory {
        schema_rows,
        classifiers,
        normal_checks: scalar_route_checks(&indexed)?,
        forced_entry,
        view_arms,
        frame_groups,
        fallbacks,
        wrong_union_errors,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("typed_values".into(), values_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
        ]),
    })
}

pub fn extract_eager_atom_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    slot_codec_source: &str,
    freeze_helper_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
    slot_codec_path: &str,
    freeze_helper_path: &str,
) -> Result<EagerAtomInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;
    let slot_codec = syn::parse_file(slot_codec_source)
        .map_err(|error| format!("parse slot codec source: {error}"))?;
    let freeze_helper = syn::parse_file(freeze_helper_source)
        .map_err(|error| format!("parse slot freeze helper source: {error}"))?;

    let (schema_rows, default_false_line, eager_true_line, expansion_line) =
        eager_atom_schema_and_macro(&schema)?;
    let normal_route = normal_eager_atom_route(&indexed)?;
    let forced_route = forced_eager_atom_route(&explicit)?;
    let (helper_arms, helper_fallback, mut helper_boundaries, value_contract) =
        eager_atom_helper_inventory(&indexed)?;
    let explicit_error_group = eager_atom_explicit_error_group(&explicit)?;
    let (slot_decode_line, slot_decode_bounds_line) = eager_slot_decode_contract(&slot_codec)?;
    let freeze_contract = eager_slot_freeze_contract(&freeze_helper)?;
    let param_boundary = helper_boundaries
        .iter_mut()
        .find(|boundary| boundary.kind == "ExprParam slot access and freeze")
        .ok_or_else(|| "ExprParam boundary is missing from the eager inventory".to_string())?;
    param_boundary.references.extend([
        EagerAtomSourceRef {
            source: "slot_codec".into(),
            line: slot_decode_line,
        },
        EagerAtomSourceRef {
            source: "slot_codec".into(),
            line: slot_decode_bounds_line,
        },
        EagerAtomSourceRef {
            source: "freeze_helper".into(),
            line: freeze_contract.line,
        },
        EagerAtomSourceRef {
            source: "freeze_helper".into(),
            line: freeze_contract.threshold_line,
        },
        EagerAtomSourceRef {
            source: "freeze_helper".into(),
            line: freeze_contract.list_take_line,
        },
        EagerAtomSourceRef {
            source: "freeze_helper".into(),
            line: freeze_contract.shared_list_line,
        },
    ]);

    Ok(EagerAtomInventory {
        schema_rows,
        default_false_line,
        eager_true_line,
        expansion_line,
        normal_route,
        forced_route,
        helper_arms,
        helper_fallback,
        explicit_error_group,
        helper_boundaries,
        slot_decode_line,
        slot_decode_bounds_line,
        freeze_contract,
        value_contract,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
            ("slot_codec".into(), slot_codec_path.into()),
            ("freeze_helper".into(), freeze_helper_path.into()),
        ]),
    })
}

pub fn extract_frame_constructor_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
) -> Result<FrameConstructorInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;

    let expression_rows = expression_schema_rows(&schema)?;
    let schema_rows = frame_constructor_schema_rows(&expression_rows)?;
    let known_view_variants = expression_rows
        .iter()
        .map(|row| row.variant.clone())
        .collect::<BTreeSet<_>>();
    let classifiers = BTreeMap::from([
        (
            "FrameApply".into(),
            parse_typed_classifier(
                &indexed,
                "indexed_frame_apply_tag",
                "FrameApply",
                FRAME_APPLY_TAGS,
            )?,
        ),
        (
            "prepared_operands".into(),
            parse_typed_classifier(
                &indexed,
                "indexed_frame_apply_prepares_operands",
                "prepared operands",
                FRAME_PREPARED_OPERAND_TAGS,
            )?,
        ),
    ]);
    if classifiers["prepared_operands"]
        .iter()
        .any(|branch| FRAME_CONSTRUCTOR_ROWS.iter().any(|(_, tag, _)| *tag == branch.tag))
    {
        return Err("frame constructor rows unexpectedly prepare host operands".into());
    }

    let normal_route = frame_constructor_normal_route(&indexed)?;
    let forced_route = frame_constructor_forced_route(&explicit)?;
    let child_arms = frame_constructor_view_arms(
        &indexed,
        "indexed_frame_apply_child",
        "indexed_frame_apply_child",
        &known_view_variants,
    )?;
    let (value_arms, helper_fallback, value_decode_line) =
        frame_constructor_value_arms(&indexed, &known_view_variants)?;
    let explicit_error_group = frame_constructor_explicit_error_group(&explicit)?;
    let helper_fallbacks = vec![
        frame_constructor_helper_fallback(
            &indexed,
            "indexed_frame_apply_child",
            &known_view_variants,
        )?,
        helper_fallback,
    ];

    let value_boundaries = value_arms
        .iter()
        .map(|arm| (arm.variant.clone(), arm.line))
        .collect::<BTreeMap<_, _>>();

    Ok(FrameConstructorInventory {
        schema_rows,
        classifiers,
        normal_route,
        forced_route,
        child_arms,
        value_arms,
        helper_fallbacks,
        explicit_error_group,
        value_decode_line,
        value_boundaries,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
        ]),
    })
}

pub fn extract_collection_construction_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    operands_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
    operands_path: &str,
) -> Result<CollectionConstructionInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;
    let operands = syn::parse_file(operands_source)
        .map_err(|error| format!("parse indexed operand helpers: {error}"))?;

    let schema_rows = collection_construction_schema_rows(&expression_schema_rows(&schema)?)?;
    let frame_apply_classifier = parse_typed_classifier(
        &indexed,
        "indexed_frame_apply_tag",
        "FrameApply",
        FRAME_APPLY_TAGS,
    )?;
    if frame_apply_classifier.iter().any(|branch| {
        COLLECTION_CONSTRUCTION_ROWS
            .iter()
            .any(|(_, tag, _, _)| *tag == branch.tag)
    }) {
        return Err("collection constructors unexpectedly use the generic FrameApply helper".into());
    }
    let normal_arms = collection_normal_arms(&indexed)?;
    let normal_view_fallbacks = collection_normal_view_fallbacks(&indexed, &normal_arms)?;
    let (forced_arms, explicit_error_group) = collection_forced_arms(&explicit)?;
    let helper_fallbacks = vec![
        collection_view_helper_fallback(&explicit, "indexed_list_literal_view", "List", "ListBuild")?,
        collection_view_helper_fallback(&explicit, "indexed_map_literal_view", "MapLiteral", "")?,
        collection_view_helper_fallback(&explicit, "indexed_record_literal_view", "Record", "")?,
        collection_record_update_fallback(&operands)?,
    ];
    let normal_fallback = collection_normal_fallback(&indexed)?;
    if normal_arms.iter().any(|arm| arm.arm_index >= normal_fallback.arm_index) {
        return Err("normal collection constructor arm occurs after the unsupported-tag fallback".into());
    }
    let forced_route = collection_forced_route(&explicit)?;

    Ok(CollectionConstructionInventory {
        schema_rows,
        frame_apply_classifier_line: frame_apply_classifier.first().map_or(0, |branch| branch.line),
        frame_apply_classifier_tags: frame_apply_classifier.iter().map(|branch| branch.tag.clone()).collect(),
        normal_arms,
        forced_arms,
        forced_route,
        normal_view_fallbacks,
        normal_fallback,
        helper_fallbacks,
        explicit_error_group,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
            ("operand_helpers".into(), operands_path.into()),
        ]),
    })
}

pub fn extract_result_constructor_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
) -> Result<ResultConstructorInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;

    let schema_rows = result_constructor_schema_rows(&expression_schema_rows(&schema)?)?;
    let normal_arms = result_constructor_normal_arms(&indexed)?;
    let normal_view_fallbacks = result_constructor_normal_view_fallbacks(&indexed, &normal_arms)?;
    let normal_fallback = collection_normal_fallback(&indexed)?;
    if normal_arms.iter().any(|arm| arm.arm_index >= normal_fallback.arm_index) {
        return Err("normal result constructor arm occurs after the unsupported-tag fallback".into());
    }
    let (forced_arms, forced_view_fallbacks, explicit_error_group) =
        result_constructor_forced_arms(&explicit)?;
    let route = collection_forced_route(&explicit)?;
    let forced_route = ResultConstructorRoute {
        frame_apply_line: route.frame_apply_line,
        frame_apply_statement_index: route.frame_apply_statement_index,
        view_kind_line: route.view_kind_line,
        view_kind_statement_index: route.view_kind_statement_index,
        eval_expr_view_line: route.eval_expr_view_line,
    };

    Ok(ResultConstructorInventory {
        schema_rows,
        normal_arms,
        normal_view_fallbacks,
        normal_fallback: ResultConstructorFallback {
            helper: normal_fallback.helper,
            outcome: normal_fallback.outcome,
            scope: normal_fallback.scope,
            line: normal_fallback.line,
            arm_index: normal_fallback.arm_index,
        },
        forced_arms,
        forced_view_fallbacks,
        explicit_error_group,
        forced_route,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
        ]),
    })
}

pub fn extract_call_frame_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
) -> Result<CallFrameInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;

    let schema_rows = call_frame_schema_rows(&expression_schema_rows(&schema)?)?;
    let mut normal_route = call_frame_normal_route(&indexed)?;
    let (forced_arms, typed_view_fallbacks, explicit_error_group, view_kind_line, eval_expr_view_line) =
        call_frame_forced_arms(&indexed, &explicit)?;
    normal_route.view_kind_line = view_kind_line;
    normal_route.eval_expr_view_line = eval_expr_view_line;
    let normal_fallback = collection_normal_fallback(&indexed)?;

    Ok(CallFrameInventory {
        schema_rows,
        normal_route,
        forced_arms,
        typed_view_fallbacks,
        explicit_error_group,
        normal_fallback: CallFrameFallback {
            helper: normal_fallback.helper,
            outcome: normal_fallback.outcome,
            scope: normal_fallback.scope,
            line: normal_fallback.line,
            arm_index: normal_fallback.arm_index,
        },
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
        ]),
    })
}

pub fn extract_field_index_inventory(
    schema_source: &str,
    indexed_source: &str,
    explicit_source: &str,
    schema_path: &str,
    indexed_path: &str,
    explicit_path: &str,
) -> Result<FieldIndexInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let indexed = syn::parse_file(indexed_source)
        .map_err(|error| format!("parse indexed dispatch: {error}"))?;
    let explicit = syn::parse_file(explicit_source)
        .map_err(|error| format!("parse explicit frame dispatch: {error}"))?;

    let schema_rows = field_index_schema_rows(&expression_schema_rows(&schema)?)?;
    let normal_route = field_index_normal_route(&indexed)?;
    let frame_apply_classifier = parse_typed_classifier(
        &indexed,
        "indexed_frame_apply_tag",
        "FrameApply",
        FRAME_APPLY_TAGS,
    )?;
    if frame_apply_classifier.iter().any(|branch| {
        FIELD_INDEX_ROWS.iter().any(|(_, tag, _)| *tag == branch.tag)
    }) {
        return Err("field and index rows are intercepted by the generic FrameApply helper".into());
    }
    let normal_arms = field_index_normal_arms(&indexed)?;
    let normal_view_fallbacks = field_index_normal_view_fallbacks(&indexed, &normal_arms)?;
    let normal_fallback = field_index_normal_fallback(&indexed)?;
    if normal_arms.iter().any(|arm| arm.arm_index >= normal_fallback.arm_index) {
        return Err("normal field/index arm occurs after the unsupported-tag fallback".into());
    }
    let (forced_arms, forced_view_fallbacks, explicit_error_group) =
        field_index_forced_arms(&explicit)?;
    let route = collection_forced_route(&explicit)?;

    Ok(FieldIndexInventory {
        schema_rows,
        normal_route,
        frame_apply_classifier_line: frame_apply_classifier.first().map_or(0, |branch| branch.line),
        normal_arms,
        normal_view_fallbacks,
        normal_fallback,
        forced_arms,
        forced_view_fallbacks,
        forced_route: FieldIndexRoute {
            frame_apply_line: route.frame_apply_line,
            frame_apply_statement_index: route.frame_apply_statement_index,
            view_kind_line: route.view_kind_line,
            view_kind_statement_index: route.view_kind_statement_index,
            eval_expr_view_line: route.eval_expr_view_line,
        },
        explicit_error_group,
        source_paths: BTreeMap::from([
            ("schema".into(), schema_path.into()),
            ("normal_dispatch".into(), indexed_path.into()),
            ("forced_dispatch".into(), explicit_path.into()),
        ]),
    })
}

fn field_index_normal_route(file: &File) -> Result<FieldIndexNormalRoute, String> {
    let wrapper = find_function(file, "eval_indexed_expr")?;
    let scheduled = local_binding_expression(wrapper, "frame_scheduled")?;
    let mut explicit_tags = MacroTokenFinder { name: "matches", tokens: Vec::new() };
    explicit_tags.visit_expr(scheduled);
    let mentions_field_index = FIELD_INDEX_ROWS.iter().any(|(_, tag, _)| {
        explicit_tags.tokens.iter().any(|tokens| tokens.split(|character: char| !character.is_alphanumeric() && character != '_')
            .any(|identifier| identifier == *tag))
    });
    if mentions_field_index {
        return Err("normal field/index wrapper explicitly schedules a cohort tag".into());
    }
    let route = call_frame_normal_route(file)
        .map_err(|error| format!("normal field/index wrapper route changed: {error}"))?;
    Ok(FieldIndexNormalRoute {
        scheduled_predicate_line: route.scheduled_predicate_line,
        frame_schedule_line: route.frame_schedule_line,
        frame_helper_line: route.frame_helper_line,
        inner_helper_line: route.inner_helper_line,
    })
}

fn call_frame_schema_rows(
    expression_rows: &[ParsedExprSchemaRow],
) -> Result<Vec<CallFrameSchemaRow>, String> {
    let expected = CALL_FRAME_ROWS
        .iter()
        .map(|(variant, tag, ordinal, effects)| (*variant, *tag, *ordinal, *effects))
        .collect::<BTreeSet<_>>();
    let actual = expression_rows
        .iter()
        .filter(|row| CALL_FRAME_ROWS.iter().any(|(_, tag, _, _)| *tag == row.tag))
        .map(|row| (
            row.variant.as_str(),
            row.tag.as_str(),
            row.ordinal,
            row.effects.as_str(),
        ))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "call frame schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    CALL_FRAME_ROWS
        .iter()
        .map(|(variant, tag, ordinal, effects)| {
            let row = expression_rows
                .iter()
                .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
                .ok_or_else(|| format!("missing call frame schema row {tag}"))?;
            if row.effects != *effects
                || row.class != "Scheduled"
                || row.strategy != "FrameControl"
                || row.classification.is_some()
            {
                return Err(format!(
                    "call frame schema row {tag} must be Scheduled FrameControl with its exact effects"
                ));
            }
            Ok(CallFrameSchemaRow {
                variant: row.variant.clone(),
                tag: row.tag.clone(),
                ordinal: row.ordinal,
                effects: row.effects.clone(),
                class: row.class.clone(),
                strategy: row.strategy.clone(),
                line: row.line,
            })
        })
        .collect()
}

fn local_binding_expression<'ast>(block: &'ast Block, name: &str) -> Result<&'ast Expr, String> {
    let mut finder = LocalBindingFinder { expected: name, matches: Vec::new() };
    finder.visit_block(block);
    match finder.matches.as_slice() {
        [Some(expression)] => Ok(expression),
        _ => Err(format!("expected one initialized {name} binding, found {}", finder.matches.len())),
    }
}

fn top_level_local_binding_expression<'ast>(
    block: &'ast Block,
    name: &str,
) -> Result<(usize, &'ast Expr), String> {
    let matches = block.stmts.iter().enumerate().filter_map(|(index, statement)| {
        let Stmt::Local(local) = statement else { return None; };
        if !matches!(&local.pat, Pat::Ident(binding) if binding.ident == name) {
            return None;
        }
        local.init.as_ref().map(|initializer| (index, initializer.expr.as_ref()))
    }).collect::<Vec<_>>();
    match matches.as_slice() {
        [(index, expression)] => Ok((*index, *expression)),
        _ => Err(format!("expected one top-level initialized {name} binding, found {}", matches.len())),
    }
}

fn statement_contains_return(statement: &Stmt) -> bool {
    struct ReturnFinder(bool);
    impl<'ast> Visit<'ast> for ReturnFinder {
        fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
            self.0 = true;
            visit::visit_expr_return(self, expression);
        }
    }
    let mut finder = ReturnFinder(false);
    finder.visit_stmt(statement);
    finder.0
}

fn call_frame_normal_route(file: &File) -> Result<CallFrameRoute, String> {
    let block = find_function(file, "eval_indexed_expr")?;
    let scheduled = local_binding_expression(block, "frame_scheduled")?;
    let predicate_calls = method_call_nodes_in_expr(scheduled, "is_scheduled_frame_control_expr");
    let instruction_calls = method_call_nodes_in_expr(scheduled, "instruction_id");
    let map_calls = method_call_nodes_in_expr(scheduled, "map");
    if predicate_calls.len() != 1
        || !path_is_ident(&predicate_calls[0].receiver, "tag")
        || !predicate_calls[0].args.is_empty()
        || instruction_calls.len() != 1
        || !path_is_ident(&instruction_calls[0].receiver, "execution")
        || instruction_calls[0].args.len() != 1
        || !path_is_ident(&instruction_calls[0].args[0], "instruction")
        || map_calls.len() != 1
        || map_calls[0].args.len() != 1
    {
        return Err("normal call scheduling no longer uses the schema FrameControl predicate".into());
    }
    let Expr::Closure(schedule_closure) = &map_calls[0].args[0] else {
        return Err("normal call scheduling predicate is not bound from the instruction tag".into());
    };
    let closure_tag = matches!(schedule_closure.inputs.first(), Some(Pat::Tuple(tuple))
        if tuple.elems.len() == 2
            && matches!(tuple.elems.first(), Some(Pat::Ident(binding)) if binding.ident == "tag")
            && matches!(tuple.elems.iter().nth(1), Some(Pat::Wild(_))));
    let closure_predicates = method_call_nodes_in_expr(&schedule_closure.body, "is_scheduled_frame_control_expr");
    if !closure_tag
        || closure_predicates.len() != 1
        || !std::ptr::eq(closure_predicates[0], predicate_calls[0])
    {
        return Err("normal call scheduling predicate no longer uses the instruction tag".into());
    }

    let (route_index, route) = top_level_local_binding_expression(block, "result")?;
    if block.stmts[..route_index].iter().any(statement_contains_return) {
        return Err("normal call route can be bypassed before the scheduling branch".into());
    }
    let Expr::If(route) = route else {
        return Err("normal call scheduling result is not a route branch".into());
    };
    if !is_force_or_scheduled(&route.cond) {
        return Err("normal call scheduling no longer selects frames for forced or scheduled expressions".into());
    }
    let then_call = block_tail_method_call(&route.then_branch)
        .ok_or_else(|| "normal scheduled call branch has no frame evaluator".to_string())?;
    let else_call = route.else_branch.as_ref()
        .and_then(|(_, expression)| match expression.as_ref() {
            Expr::Block(block) => block_tail_method_call(&block.block),
            _ => None,
        })
        .ok_or_else(|| "normal unscheduled call branch has no indexed evaluator".to_string())?;
    validate_route_call(then_call, "eval_indexed_expr_with_frames")?;
    validate_route_call(else_call, "eval_indexed_expr_inner")?;

    let inner = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(inner)?;
    let mut direct_tags = Vec::new();
    for arm in &dispatch.arms {
        if matches!(arm.pat, Pat::Wild(_)) {
            continue;
        }
        collect_exact_tag_patterns(&arm.pat, &mut direct_tags)?;
    }
    if direct_tags.iter().any(|(tag, _)| CALL_FRAME_ROWS.iter().any(|(_, expected, _, _)| *expected == tag)) {
        return Err("scheduled call rows have been intercepted by the direct indexed evaluator".into());
    }

    Ok(CallFrameRoute {
        scheduled_predicate_line: predicate_calls[0].method.span().start().line,
        frame_schedule_line: route.if_token.span().start().line,
        frame_helper_line: then_call.method.span().start().line,
        inner_helper_line: else_call.method.span().start().line,
        view_kind_line: 0,
        eval_expr_view_line: 0,
    })
}

fn is_force_or_scheduled(expression: &Expr) -> bool {
    matches!(expression, Expr::Binary(binary)
        if matches!(binary.op, syn::BinOp::Or(_))
            && matches!(&*binary.left, Expr::Field(field)
                if path_is_ident(&field.base, "self")
                    && matches!(&field.member, syn::Member::Named(name) if name == "force_frames"))
            && path_is_ident(&binary.right, "frame_scheduled"))
}

fn block_tail_method_call(block: &Block) -> Option<&syn::ExprMethodCall> {
    match block.stmts.as_slice() {
        [Stmt::Expr(Expr::MethodCall(call), None)] => Some(call),
        _ => None,
    }
}

fn validate_route_call(call: &syn::ExprMethodCall, method: &str) -> Result<(), String> {
    if call.method != method
        || !path_is_ident(&call.receiver, "self")
        || call.args.len() != 4
        || !["execution", "instruction", "slots", "call_span"]
            .iter()
            .zip(&call.args)
            .all(|(expected, actual)| path_is_ident(actual, expected))
    {
        return Err(format!("normal call route changed its {method} arguments"));
    }
    Ok(())
}

fn call_frame_forced_route(indexed: &File, explicit: &File) -> Result<(usize, usize), String> {
    let frame_apply_classifier = parse_typed_classifier(
        indexed,
        "indexed_frame_apply_tag",
        "FrameApply",
        FRAME_APPLY_TAGS,
    )?;
    if frame_apply_classifier.iter().any(|branch| {
        CALL_FRAME_ROWS.iter().any(|(_, tag, _, _)| *tag == branch.tag)
    }) {
        return Err("forced call rows are intercepted by the FrameApply classifier".into());
    }

    let block = find_function(explicit, "eval_expr")?;
    let (_, in_place_initializer) = top_level_local_binding_expression(block, "in_place")?;
    if !is_force_guarded_in_place_initializer(in_place_initializer) {
        return Err("forced call route no longer derives its in-place guard from the evaluator and schema mark".into());
    }
    let (_, strategy_initializer) = top_level_local_binding_expression(block, "strategy")?;
    if !matches!(strategy_initializer, Expr::MethodCall(call)
        if call.method == "frame_strategy"
            && call.args.is_empty()
            && path_is_ident(&call.receiver, "tag"))
    {
        return Err("forced call route no longer uses the instruction frame strategy".into());
    }

    let (view_kind_index, view_kind_initializer) =
        top_level_local_binding_expression(block, "view_kind")?;
    let view_kind_call = exact_call_view_kind(view_kind_initializer)
        .ok_or_else(|| "forced call route no longer derives view_kind from tag.expr_view_kind".to_string())?;
    let tail_index = block.stmts.len().checked_sub(1)
        .ok_or_else(|| "eval_expr has no tail expression".to_string())?;
    if view_kind_index + 1 != tail_index {
        return Err("forced call route can be bypassed between ExprViewKind selection and its tail dispatch".into());
    }
    let Some(Stmt::Expr(Expr::MethodCall(eval_expr_view), None)) = block.stmts.get(tail_index) else {
        return Err("forced call route does not end in eval_expr_view".into());
    };
    if !validate_eval_expr_view_tail(eval_expr_view)
        || eval_expr_view.method.span().start().line <= view_kind_call.method.span().start().line
    {
        return Err("forced call route tail does not pass the selected ExprViewKind to eval_expr_view".into());
    }
    let mut calls = MethodCallNodeFinder { name: "eval_expr_view", calls: Vec::new() };
    calls.visit_block(block);
    if calls.calls.len() != 1 || !std::ptr::eq(calls.calls[0], eval_expr_view) {
        return Err("forced call route contains another eval_expr_view path".into());
    }

    let mut early_returns = BTreeSet::new();
    for statement in &block.stmts[..view_kind_index] {
        if !statement_contains_return(statement) {
            continue;
        }
        let Stmt::Expr(Expr::If(branch), _) = statement else {
            return Err("forced call route has an unclassified early return before ExprViewKind dispatch".into());
        };
        let kind = if is_in_place_short_circuit_condition(&branch.cond) {
            "in_place"
        } else if is_eager_atom_condition(&branch.cond) {
            "eager_atom"
        } else if is_typed_frame_apply_condition(&branch.cond) {
            "typed_scalar"
        } else if is_indexed_frame_apply_condition(&branch.cond) {
            "frame_apply"
        } else {
            return Err("forced call route has an unreviewed early return before ExprViewKind dispatch".into());
        };
        if !early_returns.insert(kind) {
            return Err("forced call route duplicates an early-return branch".into());
        }
    }
    if early_returns != BTreeSet::from(["in_place", "eager_atom", "typed_scalar", "frame_apply"]) {
        return Err("forced call route has incomplete early-return exclusion checks".into());
    }

    Ok((view_kind_call.method.span().start().line, eval_expr_view.method.span().start().line))
}

fn is_force_guarded_in_place_initializer(expression: &Expr) -> bool {
    let Expr::Binary(binary) = expression else { return false; };
    if !matches!(binary.op, syn::BinOp::And(_)) {
        return false;
    }
    let forced = matches!(binary.left.as_ref(), Expr::Unary(unary)
        if matches!(unary.op, syn::UnOp::Not(_))
            && matches!(unary.expr.as_ref(), Expr::Field(force)
                if matches!(&force.member, syn::Member::Named(name) if name == "force_frames")
                    && matches!(&*force.base, Expr::Field(evaluator)
                        if matches!(&evaluator.member, syn::Member::Named(name) if name == "evaluator")
                            && path_is_ident(&evaluator.base, "self"))));
    let in_place = matches!(binary.right.as_ref(), Expr::MethodCall(call)
        if call.method == "is_in_place"
            && call.args.len() == 1
            && path_is_ident(&call.args[0], "instruction")
            && path_is_ident(&call.receiver, "execution"));
    forced && in_place
}

fn is_in_place_short_circuit_condition(expression: &Expr) -> bool {
    match expression {
        Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
            path_is_ident(&binary.left, "in_place")
                || is_in_place_short_circuit_condition(&binary.left)
        }
        _ => false,
    }
}

fn exact_call_view_kind(expression: &Expr) -> Option<&syn::ExprMethodCall> {
    let Expr::Try(try_expression) = expression else { return None; };
    let Expr::MethodCall(ok_or_else) = try_expression.expr.as_ref() else { return None; };
    if ok_or_else.method != "ok_or_else" || ok_or_else.args.len() != 1 {
        return None;
    }
    let Expr::MethodCall(view_kind) = ok_or_else.receiver.as_ref() else { return None; };
    if view_kind.method != "expr_view_kind"
        || !view_kind.args.is_empty()
        || !path_is_ident(&view_kind.receiver, "tag")
    {
        return None;
    }
    let Expr::Closure(error) = &ok_or_else.args[0] else { return None; };
    let Some(error_call) = exact_call(error.body.as_ref(), &["frame_strategy_error"]) else {
        return None;
    };
    if !error.inputs.is_empty()
        || error_call.args.len() != 2
        || !path_is_ident(&error_call.args[0], "tag")
        || !path_is_ident(&error_call.args[1], "span")
    {
        return None;
    }
    Some(view_kind)
}

fn validate_eval_expr_view_tail(call: &syn::ExprMethodCall) -> bool {
    call.method == "eval_expr_view"
        && path_is_ident(&call.receiver, "self")
        && call.args.len() == 8
        && path_is_ident(&call.args[0], "index")
        && path_is_ident(&call.args[1], "instruction")
        && path_is_ident(&call.args[2], "tag")
        && path_is_ident(&call.args[3], "view_kind")
        && matches!(&call.args[4], Expr::Reference(reference)
            if reference.mutability.is_none() && path_is_ident(&reference.expr, "execution"))
        && path_is_ident(&call.args[5], "payload")
        && path_is_ident(&call.args[6], "span")
        && path_is_ident(&call.args[7], "next")
}

fn call_frame_forced_arms(
    indexed: &File,
    file: &File,
) -> Result<(Vec<CallFrameArm>, Vec<CallFrameFallback>, CallFrameFallback, usize, usize), String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder { family: "ExprViewKind", matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!("eval_expr_view must have one ExprViewKind match, found {}", finder.matches.len()));
    }
    let expected_groups = BTreeMap::from([
        ("Call", BTreeSet::from(["Call", "SelfCall", "DirectPureCall"])),
        ("DirectPureCall", BTreeSet::from(["Call", "SelfCall", "DirectPureCall"])),
        ("ExternalCall", BTreeSet::from(["ExternalCall"])),
        ("DynamicCall", BTreeSet::from(["DynamicCall", "TypedCall"])),
    ]);
    let expected_variants = expected_groups.keys().copied().collect::<BTreeSet<_>>();
    let mut found = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    let mut error_group = None;
    for (arm_index, arm) in finder.matches[0].arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded ExprViewKind call dispatch arm at line {}", arm.pat.span().start().line));
        }
        if matches!(arm.pat, Pat::Wild(_) | Pat::Ident(_)) {
            return Err("eval_expr_view call dispatch has a catch-all arm".into());
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
        let variant_set = variants.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if is_frame_strategy_error_return(&arm.body) {
            if !variant_set.is_disjoint(&expected_variants) {
                return Err("call view kind is intercepted by frame_strategy_error".into());
            }
            if error_group.is_some() {
                return Err("ExprViewKind frame_strategy_error group is split".into());
            }
            error_group = Some(CallFrameFallback {
                helper: "eval_expr_view".into(),
                outcome: "frame_strategy_error(tag, span)".into(),
                scope: "valid ExprViewKind values without dedicated frame handlers".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
            continue;
        }
        let Some(selected) = variants.iter().find(|variant| expected_variants.contains(variant.as_str())) else {
            continue;
        };
        let expected_group = expected_groups.get(selected.as_str()).expect("selected call view is closed");
        if variant_set != *expected_group {
            return Err(format!("forced {selected} call view group changed: {variant_set:?}"));
        }
        let (behavior, fallback_line, fallback_message) = call_frame_forced_behavior(selected, &arm.body)?;
        let group = variants.clone();
        for variant in variants.iter().filter(|variant| expected_variants.contains(variant.as_str())) {
            if found.insert(variant.clone(), CallFrameArm {
                tag: CALL_FRAME_ROWS.iter().find(|(candidate, _, _, _)| *candidate == variant)
                    .expect("call view maps to a fixed tag").1.into(),
                variant: variant.clone(),
                view_group: group.clone(),
                behavior: behavior.clone(),
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate forced call view {variant}"));
            }
            fallbacks.insert(variant.clone(), CallFrameFallback {
                helper: "eval_expr_view".into(),
                outcome: format!("unreachable!: {fallback_message}"),
                scope: format!("{variant} view-kind arm decodes a different ExprView"),
                line: fallback_line,
                arm_index,
            });
        }
    }
    if found.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected_variants {
        return Err(format!("forced call view coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    let error_group = error_group.ok_or_else(|| "ExprViewKind frame_strategy_error group is missing".to_string())?;
    if found.values().any(|arm| arm.arm_index >= error_group.arm_index) {
        return Err("call handler occurs after the frame_strategy_error group".into());
    }
    let (view_kind_line, eval_expr_view_line) = call_frame_forced_route(indexed, file)?;
    let arms = CALL_FRAME_ROWS.iter()
        .map(|(variant, _, _, _)| found.get(*variant).expect("validated call view coverage").clone())
        .collect();
    let view_fallbacks = CALL_FRAME_ROWS.iter()
        .map(|(variant, _, _, _)| fallbacks.get(*variant).expect("validated call fallback").clone())
        .collect();
    Ok((
        arms,
        view_fallbacks,
        CallFrameFallback {
            helper: "eval_expr_view".into(),
            outcome: error_group.outcome,
            scope: error_group.scope,
            line: error_group.line,
            arm_index: error_group.arm_index,
        },
        view_kind_line,
        eval_expr_view_line,
    ))
}

fn call_frame_forced_behavior(
    variant: &str,
    body: &Expr,
) -> Result<(String, usize, String), String> {
    let (expected_views, message, behavior) = match variant {
        "Call" | "DirectPureCall" => (
            BTreeSet::from(["Call", "DirectPureCall", "SelfCall"].map(str::to_string)),
            "static call tag has a typed call view",
            "decode_static_call_and_schedule_arguments",
        ),
        "ExternalCall" => (
            BTreeSet::from(["ExternalCall".to_string()]),
            "external call tag has a typed call view",
            "resolve_external_kind_then_schedule_arguments",
        ),
        "DynamicCall" => (
            BTreeSet::from(["DynamicCall", "TypedCall"].map(str::to_string)),
            "dynamic call tag has a typed call view",
            "evaluate_callee_before_scheduling_dynamic_arguments",
        ),
        _ => return Err(format!("unknown call view kind {variant}")),
    };
    let decode = call_nodes_in_expr(body, "decode");
    if decode.len() != 1 || !path_has_segments(&decode[0].func, &["ExprView", "decode"]) {
        return Err(format!("forced {variant} call handler does not decode its ExprView once"));
    }
    let mut view_matches = ViewMatchFinder { matches: Vec::new() };
    view_matches.visit_expr(body);
    let mut let_else = ExprViewLetElseFinder { candidates: Vec::new() };
    let_else.visit_expr(body);
    if variant == "ExternalCall" {
        if !view_matches.matches.is_empty() || let_else.candidates.len() != 1 {
            return Err("forced ExternalCall handler must use one typed-view let-else".into());
        }
        let candidate = &let_else.candidates[0];
        let mut actual = Vec::new();
        collect_named_view_variants(candidate.pattern, "ExprView", &mut actual)?;
        if actual != [variant] || !is_unreachable_block(candidate.fallback, message) {
            return Err("forced ExternalCall typed-view fallback changed".into());
        }
    } else {
        if view_matches.matches.len() != 1 || !let_else.candidates.is_empty() {
            return Err(format!("forced {variant} call handler must match its decoded view once"));
        }
        let view_match = view_matches.matches[0];
        let expected_patterns = match variant {
            "Call" | "DirectPureCall" => vec![
                BTreeSet::from(["Call".to_string(), "DirectPureCall".to_string()]),
                BTreeSet::from(["SelfCall".to_string()]),
            ],
            "DynamicCall" => vec![
                BTreeSet::from(["DynamicCall".to_string()]),
                BTreeSet::from(["TypedCall".to_string()]),
            ],
            _ => unreachable!("only static and dynamic call matches are handled above"),
        };
        let mut actual = BTreeSet::new();
        let mut actual_patterns = Vec::new();
        let mut has_wildcard = false;
        for arm in &view_match.arms {
            if arm.guard.is_some() {
                return Err(format!("forced {variant} call view match has a guard"));
            }
            if matches!(arm.pat, Pat::Wild(_)) {
                if !is_unconditional_unreachable(&arm.body, message) {
                    return Err(format!(
                        "forced {variant} typed-view fallback is not an unconditional unreachable!"
                    ));
                }
                has_wildcard = true;
                continue;
            }
            let mut variants = Vec::new();
            collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
            let pattern = variants.into_iter().collect::<BTreeSet<_>>();
            if !expected_patterns.contains(&pattern)
                || actual_patterns.contains(&pattern)
                || pattern.iter().any(|view| !actual.insert(view.clone()))
            {
                return Err(format!("forced {variant} call view match has duplicate or unexpected groups"));
            }
            actual_patterns.push(pattern);
        }
        if actual != expected_views || actual_patterns.len() != expected_patterns.len() || !has_wildcard {
            return Err(format!("forced {variant} typed call views differ: {actual:?}"));
        }
    }
    let fallback_line = find_unreachable_macro(body, message)?.span().start().line;
    match variant {
        "Call" | "DirectPureCall" => {
            if method_call_nodes_in_expr(body, "preflight").len() != 1
                || method_call_nodes_in_expr(body, "key").len() != 1
                || method_call_nodes_in_expr(body, "function_kind").len() != 1
                || method_call_nodes_in_expr(body, "push_static_arguments").len() != 1
            {
                return Err(format!("forced {variant} call handler no longer schedules static arguments"));
            }
        }
        "ExternalCall" => {
            if method_call_nodes_in_expr(body, "preflight").len() != 1
                || method_call_nodes_in_expr(body, "external_kind").len() != 1
                || method_call_nodes_in_expr(body, "push_static_arguments").len() != 1
            {
                return Err("forced ExternalCall handler no longer resolves and schedules its external function".into());
            }
        }
        "DynamicCall" => {
            let mut push_finder = MethodCallNodeFinder { name: "push_expr", calls: Vec::new() };
            push_finder.visit_expr(body);
            let push = push_finder.calls;
            if method_call_nodes_in_expr(body, "preflight").len() != 1
                || push.len() != 1
                || push[0].args.len() != 4
                || !path_is_ident(&push[0].args[0], "index")
                || !path_is_ident(&push[0].args[1], "callee")
                || !path_is_ident(&push[0].args[2], "value_span")
                || !is_struct_variant_expr(&push[0].args[3], "FrameContinuation", "DynamicCallee")
            {
                return Err("forced DynamicCall handler no longer evaluates its callee before arguments".into());
            }
            if !dynamic_call_callee_flows_from_view(body, push[0])? {
                return Err("forced DynamicCall callee no longer flows from its decoded typed view".into());
            }
        }
        _ => unreachable!("closed call variants were handled above"),
    }
    Ok((behavior.into(), fallback_line, message.into()))
}

fn is_struct_variant_expr(expression: &Expr, enum_name: &str, variant: &str) -> bool {
    matches!(expression, Expr::Struct(value)
        if path_ends_with_segments(&value.path, &[enum_name, variant]))
}

fn is_unconditional_unreachable(expression: &Expr, message: &str) -> bool {
    is_exact_unreachable(expression, message)
        || is_unreachable_block(expression, message)
}

fn dynamic_call_callee_flows_from_view(
    body: &Expr,
    push: &syn::ExprMethodCall,
) -> Result<bool, String> {
    let mut view_matches = ViewMatchFinder { matches: Vec::new() };
    view_matches.visit_expr(body);
    let [view_match] = view_matches.matches.as_slice() else {
        return Ok(false);
    };
    if !path_is_ident(&view_match.expr, "view") {
        return Ok(false);
    }

    let view_initializer = local_binding_expression(
        match body {
            Expr::Block(block) => &block.block,
            _ => return Ok(false),
        },
        "view",
    )?;
    let view_decode = match view_initializer {
        Expr::Try(try_expression) => match try_expression.expr.as_ref() {
            Expr::MethodCall(map_err) if map_err.method == "map_err" => match map_err.receiver.as_ref() {
                Expr::Call(decode)
                    if matches!(&*decode.func, Expr::Path(path)
                        if path_ends_with_segments(&path.path, &["ExprView", "decode"])) => decode,
                _ => return Ok(false),
            },
            _ => return Ok(false),
        },
        _ => return Ok(false),
    };
    if view_decode.args.len() != 3
        || !matches!(&view_decode.args[0], Expr::Reference(reference)
            if reference.mutability.is_none() && path_is_ident(&reference.expr, "execution"))
        || !path_is_ident(&view_decode.args[1], "tag")
        || !path_is_ident(&view_decode.args[2], "payload")
    {
        return Ok(false);
    }

    let Expr::Block(body_block) = body else { return Ok(false); };
    let mut tuple_bindings = CallTupleMatchBindingFinder { matches: Vec::new() };
    tuple_bindings.visit_block(&body_block.block);
    let [tuple_binding] = tuple_bindings.matches.as_slice() else { return Ok(false); };
    let Some(tuple_binding_index) = body_block.block.stmts.iter().position(|statement| {
        match statement {
            Stmt::Local(local) => std::ptr::eq(local, *tuple_binding),
            _ => false,
        }
    }) else { return Ok(false); };
    let Some(push_index) = body_block.block.stmts.iter().position(|statement| {
        match statement {
            Stmt::Expr(Expr::MethodCall(call), Some(_)) => std::ptr::eq(call, push),
            _ => false,
        }
    }) else { return Ok(false); };
    if tuple_binding_index >= push_index {
        return Ok(false);
    }
    if !tuple_binding_has_immutable_callee(tuple_binding)
        || (tuple_binding_index + 1..push_index)
            .any(|statement_index| statement_writes_callee(&body_block.block.stmts[statement_index]))
    {
        return Ok(false);
    }
    let mut callee_bindings = LocalNameBindingFinder { name: "callee", count: 0 };
    callee_bindings.visit_block(&body_block.block);
    if callee_bindings.count != 1 {
        return Ok(false);
    }
    let Some(initializer) = tuple_binding.init.as_ref() else { return Ok(false); };
    let Expr::Match(callee_match) = initializer.expr.as_ref() else { return Ok(false); };
    if !std::ptr::eq(callee_match, *view_match) {
        return Ok(false);
    }

    let mut seen = BTreeSet::new();
    for arm in &view_match.arms {
        if matches!(arm.pat, Pat::Wild(_)) {
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
        let Some(variant) = variants.first() else { continue; };
        if !matches!(variant.as_str(), "DynamicCall" | "TypedCall") {
            continue;
        }
        if variants.len() != 1
            || !pattern_struct_field_binds(&arm.pat, "ExprView", variant, "callee", "callee")
            || !tuple_first_is_raw_binding(&arm.body, "callee")
            || !seen.insert(variant.clone())
        {
            return Ok(false);
        }
    }
    Ok(seen == BTreeSet::from(["DynamicCall".into(), "TypedCall".into()]))
}

fn tuple_binding_has_immutable_callee(local: &syn::Local) -> bool {
    let Pat::Tuple(tuple) = &local.pat else { return false; };
    matches!(tuple.elems.first(), Some(Pat::Ident(binding))
        if binding.ident == "callee"
            && binding.by_ref.is_none()
            && binding.mutability.is_none()
            && binding.subpat.is_none())
}

fn statement_writes_callee(statement: &Stmt) -> bool {
    let mut writes = CalleeWriteFinder { writes: 0 };
    writes.visit_stmt(statement);
    writes.writes != 0
}

struct CalleeWriteFinder {
    writes: usize,
}

impl<'ast> Visit<'ast> for CalleeWriteFinder {
    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        if expression_place_is_rooted_in_ident(&assignment.left, "callee") {
            self.writes += 1;
        }
        visit::visit_expr_assign(self, assignment);
    }

    fn visit_expr_binary(&mut self, binary: &'ast syn::ExprBinary) {
        if matches!(&binary.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_))
            && expression_place_is_rooted_in_ident(&binary.left, "callee")
        {
            self.writes += 1;
        }
        visit::visit_expr_binary(self, binary);
    }

    fn visit_expr_reference(&mut self, reference: &'ast syn::ExprReference) {
        if reference.mutability.is_some()
            && expression_place_is_rooted_in_ident(&reference.expr, "callee")
        {
            self.writes += 1;
        }
        visit::visit_expr_reference(self, reference);
    }
}

fn expression_place_is_rooted_in_ident(expression: &Expr, name: &str) -> bool {
    match expression {
        Expr::Path(_) => path_is_ident(expression, name),
        Expr::Field(field) => expression_place_is_rooted_in_ident(&field.base, name),
        Expr::Index(index) => expression_place_is_rooted_in_ident(&index.expr, name),
        Expr::Paren(paren) => expression_place_is_rooted_in_ident(&paren.expr, name),
        Expr::Group(group) => expression_place_is_rooted_in_ident(&group.expr, name),
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Deref(_)) => {
            expression_place_is_rooted_in_ident(&unary.expr, name)
        }
        _ => false,
    }
}

fn pattern_struct_field_binds(
    pattern: &Pat,
    enum_name: &str,
    variant: &str,
    field_name: &str,
    binding_name: &str,
) -> bool {
    let Pat::Struct(pattern) = pattern else { return false; };
    if !path_ends_with_segments(&pattern.path, &[enum_name, variant]) {
        return false;
    }
    pattern.fields.iter().any(|field| {
        matches!(&field.member, syn::Member::Named(name) if name == field_name)
            && matches!(&*field.pat, Pat::Ident(binding)
                if binding.ident == binding_name
                    && binding.by_ref.is_none()
                    && binding.mutability.is_none()
                    && binding.subpat.is_none())
    })
}

fn tuple_first_is_raw_binding(expression: &Expr, binding: &str) -> bool {
    let Expr::Tuple(tuple) = expression else { return false; };
    matches!(tuple.elems.first(), Some(Expr::MethodCall(call))
        if call.method == "raw"
            && call.args.is_empty()
            && path_is_ident(&call.receiver, binding))
}

struct CallTupleMatchBindingFinder<'ast> {
    matches: Vec<&'ast syn::Local>,
}

impl<'ast> Visit<'ast> for CallTupleMatchBindingFinder<'ast> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if matches!(&local.pat, Pat::Tuple(tuple)
            if matches!(tuple.elems.first(), Some(Pat::Ident(binding)) if binding.ident == "callee"))
        {
            self.matches.push(local);
        }
        visit::visit_local(self, local);
    }
}

struct LocalNameBindingFinder {
    name: &'static str,
    count: usize,
}

impl<'ast> Visit<'ast> for LocalNameBindingFinder {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        struct PatternNameFinder<'a> {
            name: &'a str,
            count: usize,
        }
        impl<'ast> Visit<'ast> for PatternNameFinder<'_> {
            fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
                if pattern.ident == self.name {
                    self.count += 1;
                }
                visit::visit_pat_ident(self, pattern);
            }
        }
        let mut pattern_finder = PatternNameFinder { name: self.name, count: 0 };
        pattern_finder.visit_pat(&local.pat);
        self.count += pattern_finder.count;
        visit::visit_local(self, local);
    }
}

struct UnreachableMacroFinder<'ast> {
    expected_message: String,
    matches: Vec<&'ast syn::ExprMacro>,
}

impl<'ast> Visit<'ast> for UnreachableMacroFinder<'ast> {
    fn visit_expr_macro(&mut self, expression: &'ast syn::ExprMacro) {
        if expression.mac.path.segments.last().is_some_and(|segment| segment.ident == "unreachable")
            && syn::parse2::<syn::LitStr>(expression.mac.tokens.clone())
                .is_ok_and(|message| message.value() == self.expected_message)
        {
            self.matches.push(expression);
        }
        visit::visit_expr_macro(self, expression);
    }
}

fn find_unreachable_macro<'ast>(body: &'ast Expr, expected: &str) -> Result<&'ast syn::ExprMacro, String> {
    let mut finder = UnreachableMacroFinder { expected_message: expected.into(), matches: Vec::new() };
    finder.visit_expr(body);
    if finder.matches.len() != 1 {
        return Err(format!("expected one unreachable!({expected:?}), found {}", finder.matches.len()));
    }
    Ok(finder.matches[0])
}

fn field_index_schema_rows(
    expression_rows: &[ParsedExprSchemaRow],
) -> Result<Vec<FieldIndexSchemaRow>, String> {
    let expected = FIELD_INDEX_ROWS.iter().copied().collect::<BTreeSet<_>>();
    let actual = expression_rows
        .iter()
        .filter(|row| FIELD_INDEX_ROWS.iter().any(|(_, tag, _)| *tag == row.tag))
        .map(|row| (row.variant.as_str(), row.tag.as_str(), row.ordinal))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "field/index schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    FIELD_INDEX_ROWS
        .iter()
        .map(|(variant, tag, ordinal)| {
            let row = expression_rows
                .iter()
                .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
                .ok_or_else(|| format!("schema has no exact field/index row {tag}"))?;
            if row.effects != "0" || row.class != "Operands" || row.strategy != "FrameApply" {
                return Err(format!("field/index schema metadata changed for {tag}"));
            }
            Ok(FieldIndexSchemaRow {
                variant: row.variant.clone(),
                tag: row.tag.clone(),
                ordinal: row.ordinal,
                effects: row.effects.clone(),
                class: row.class.clone(),
                strategy: row.strategy.clone(),
                line: row.line,
            })
        })
        .collect()
}

fn result_constructor_schema_rows(
    expression_rows: &[ParsedExprSchemaRow],
) -> Result<Vec<ResultConstructorSchemaRow>, String> {
    let expected = RESULT_CONSTRUCTOR_ROWS.iter().copied().collect::<BTreeSet<_>>();
    let actual_rows = expression_rows
        .iter()
        .filter(|row| RESULT_CONSTRUCTOR_ROWS.iter().any(|(_, tag, _)| *tag == row.tag))
        .collect::<Vec<_>>();
    let actual = actual_rows
        .iter()
        .map(|row| (row.variant.as_str(), row.tag.as_str(), row.ordinal))
        .collect::<BTreeSet<_>>();
    if actual != expected || actual_rows.len() != expected.len() {
        return Err(format!("result constructor schema cohort drift: expected {expected:?}, found {actual:?}"));
    }
    RESULT_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, tag, ordinal)| {
            let row = expression_rows
                .iter()
                .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
                .ok_or_else(|| format!("missing result constructor schema row {tag}"))?;
            if row.effects != "0"
                || row.class != "Operands"
                || row.strategy != "FrameControl"
                || row.classification.is_some()
            {
                return Err(format!("result constructor schema row {tag} has unexpected metadata"));
            }
            Ok(ResultConstructorSchemaRow {
                variant: row.variant.clone(),
                tag: row.tag.clone(),
                ordinal: row.ordinal,
                effects: row.effects.clone(),
                class: row.class.clone(),
                strategy: row.strategy.clone(),
                line: row.line,
            })
        })
        .collect()
}

fn field_index_normal_arms(file: &File) -> Result<Vec<FieldIndexArm>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    let expected = FIELD_INDEX_ROWS
        .iter()
        .map(|(variant, tag, _)| (*tag, *variant))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    for (arm_index, arm) in dispatch.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded indexed expression arm at line {}", arm.pat.span().start().line));
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            continue;
        }
        let mut tags = Vec::new();
        collect_exact_tag_patterns(&arm.pat, &mut tags)?;
        for (tag, line) in tags.iter().filter(|(tag, _)| expected.contains_key(tag.as_str())) {
            if tags.len() != 1 {
                return Err(format!("field/index tag {tag} is grouped with another dispatch tag"));
            }
            let variant = expected[tag.as_str()];
            let behavior = field_index_normal_behavior(variant, &arm.body)?;
            if found.insert(tag.clone(), FieldIndexArm {
                tag: tag.clone(),
                variant: variant.into(),
                behavior,
                line: *line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate normal field/index arm {tag}"));
            }
        }
    }
    if found.len() != expected.len() {
        return Err(format!("normal field/index coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    Ok(FIELD_INDEX_ROWS
        .iter()
        .map(|(_, tag, _)| found.get(*tag).expect("validated field/index arm").clone())
        .collect())
}

fn field_index_normal_behavior(variant: &str, body: &Expr) -> Result<String, String> {
    let typed_view = field_index_view_let_else(body, variant, false)?;
    let expected_children: &[&str] = match variant {
        "Field" | "IndexFromEnd" => &["base"],
        "Index" => &["base", "index"],
        _ => return Err(format!("unknown field/index variant {variant}")),
    };
    let mut child_matches = ChildEvaluationMatchFinder { matches: Vec::new() };
    child_matches.visit_expr(body);
    if child_matches.matches.len() != expected_children.len() {
        return Err(format!("normal {variant} arm evaluates {} children, expected {}", child_matches.matches.len(), expected_children.len()));
    }
    let Expr::Block(block) = body else {
        return Err(format!("normal {variant} arm must bind child results in a block"));
    };
    for (child_match, child) in child_matches.matches.iter().zip(expected_children) {
        let Expr::Try(try_expression) = &*child_match.expr else {
            return Err(format!("normal {variant} child evaluation is not a method call"));
        };
        let Expr::MethodCall(call) = try_expression.expr.as_ref() else {
            return Err(format!("normal {variant} child evaluation is not a method call"));
        };
        if call.method != "eval_indexed_expr"
            || !path_is_ident(&call.receiver, "self")
            || call.args.len() != 4
            || !path_is_ident(&call.args[0], "execution")
            || !path_is_ident(&call.args[1], child)
            || !path_is_ident(&call.args[2], "slots")
            || !path_is_ident(&call.args[3], "span")
        {
            return Err(format!("normal {variant} child order or source changed"));
        }
        validate_normal_child_flow(child_match, variant)?;
        if local_child_match_binding(&block.block, child_match) != Some((*child).to_string()) {
            return Err(format!("normal {variant} child result no longer flows to its lookup binding"));
        }
    }
    let lookup_name = match variant {
        "Field" => "indexed_field_value",
        "Index" => "lowered_index_value",
        "IndexFromEnd" => "lowered_index_from_end_value",
        _ => return Err(format!("unknown field/index variant {variant}")),
    };
    normal_child_results_are_live(
        &block.block,
        &child_matches.matches,
        expected_children,
        body,
        lookup_name,
        variant,
    )?;
    if !top_level_local_is_raw_field(body, "base", "base") {
        return Err(format!("normal {variant} base no longer comes from its typed view"));
    }
    let expected_behavior = match variant {
        "Field" => {
            if !top_level_local_is_method(body, "name", "name", "as_ref")
                || !normal_field_access_dataflow(body)
            {
                return Err("normal field arm changed its record fast path or field lookup".into());
            }
            "try_record_field_chain_then_evaluate_base_and_lookup_field"
        }
        "Index" => {
            if !top_level_local_is_raw_field(body, "index", "index")
                || !normal_index_access_dataflow(body)
            {
                return Err("normal index arm changed its base/index evaluation or lookup".into());
            }
            "evaluate_base_then_index_and_lookup"
        }
        "IndexFromEnd" => {
            if !normal_index_from_end_dataflow(body) {
                return Err("normal index-from-end arm changed its base or distance lookup".into());
            }
            "evaluate_base_then_lookup_from_end_distance"
        }
        _ => unreachable!("field/index variants were closed above"),
    };
    let _ = typed_view;
    Ok(expected_behavior.into())
}

fn validate_normal_child_flow(child_match: &syn::ExprMatch, variant: &str) -> Result<(), String> {
    let breaks = child_match.arms.iter().filter(|arm| {
        matches!(&arm.pat, Pat::TupleStruct(pattern)
            if path_ends_with_segments(&pattern.path, &["ControlFlow", "Break"])
                && pattern.elems.len() == 1)
    }).collect::<Vec<_>>();
    if breaks.len() != 1 {
        return Err(format!("normal {variant} child does not propagate a break"));
    }
    let Pat::TupleStruct(pattern) = &breaks[0].pat else { unreachable!() };
    let Some(Pat::Ident(binding)) = pattern.elems.first() else {
        return Err(format!("normal {variant} break value is not bound"));
    };
    if !return_control_flow_breaks(&breaks[0].body, &binding.ident.to_string()) {
        return Err(format!("normal {variant} child break is swallowed"));
    }
    let (value, continue_arm) = continue_value_arm(child_match)?;
    if !path_is_ident(&continue_arm.body, &value) {
        return Err(format!("normal {variant} child value is discarded"));
    }
    Ok(())
}

fn local_child_match_binding(block: &syn::Block, target: &syn::ExprMatch) -> Option<String> {
    child_match_binding(block, target).map(|(_, name)| name)
}

fn child_match_binding(block: &syn::Block, target: &syn::ExprMatch) -> Option<(usize, String)> {
    let mut found = None;
    for (index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Local(local) = statement else { continue; };
        let Pat::Ident(binding) = &local.pat else { continue; };
        let Some(initializer) = local.init.as_ref() else { continue; };
        let mut finder = ChildEvaluationMatchFinder { matches: Vec::new() };
        finder.visit_expr(&initializer.expr);
        if finder.matches.iter().any(|candidate| std::ptr::eq(*candidate, target)) {
            if found.is_some() {
                return None;
            }
            found = Some((index, binding.ident.to_string()));
        }
    }
    found
}

fn has_assignment_or_local_shadow(statements: &[Stmt], name: &str) -> bool {
    struct MutationFinder<'name> {
        name: &'name str,
        found: bool,
    }
    impl<'ast> Visit<'ast> for MutationFinder<'_> {
        fn visit_local(&mut self, local: &'ast syn::Local) {
            if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == self.name) {
                self.found = true;
            }
            visit::visit_local(self, local);
        }

        fn visit_expr_assign(&mut self, expression: &'ast syn::ExprAssign) {
            if path_is_ident(&expression.left, self.name) {
                self.found = true;
            }
            visit::visit_expr_assign(self, expression);
        }
    }
    let mut finder = MutationFinder { name, found: false };
    for statement in statements {
        finder.visit_stmt(statement);
    }
    finder.found
}

fn normal_lookup_is_tail(body: &Expr, expected_name: &str) -> bool {
    let Expr::Block(block) = body else { return false; };
    let Some(Stmt::Expr(expression, None)) = block.block.stmts.last() else { return false; };
    let Expr::Call(continue_call) = expression else { return false; };
    if !matches!(&*continue_call.func, Expr::Path(path)
        if path_ends_with_segments(&path.path, &["ControlFlow", "Continue"]))
        || continue_call.args.len() != 1
    {
        return false;
    }
    let Expr::Try(value) = &continue_call.args[0] else { return false; };
    match value.expr.as_ref() {
        Expr::Call(call) => matches!(&*call.func, Expr::Path(path)
            if path.path.segments.last().is_some_and(|segment| segment.ident == expected_name)),
        Expr::MethodCall(call) => call.method == expected_name,
        _ => false,
    }
}

fn normal_child_results_are_live(
    block: &syn::Block,
    child_matches: &[&syn::ExprMatch],
    expected_children: &[&str],
    body: &Expr,
    lookup_name: &str,
    variant: &str,
) -> Result<(), String> {
    if !normal_lookup_is_tail(body, lookup_name) {
        return Err(format!("normal {variant} lookup is not the final successful result"));
    }
    let lookup_index = block.stmts.len().saturating_sub(1);
    let child_binding_indices = child_matches.iter()
        .filter_map(|child_match| child_match_binding(block, child_match).map(|(index, _)| index))
        .collect::<BTreeSet<_>>();
    if variant != "Field"
        && child_binding_indices.iter().min().is_some_and(|first_child| {
            block.stmts.iter().take(*first_child).any(statement_contains_return)
        })
    {
        return Err(format!("normal {variant} arm can return before evaluating its children"));
    }
    if variant == "Field" {
        let first_child = child_binding_indices.iter().min().copied()
            .ok_or_else(|| "normal Field arm has no child result binding".to_string())?;
        let fast_paths = block.stmts.iter().take(first_child)
            .filter(|statement| statement_contains_return(statement))
            .collect::<Vec<_>>();
        if fast_paths.len() != 1 || !normal_field_fast_path_is_valid(fast_paths[0]) {
            return Err("normal Field fast path does not return its looked-up record value".into());
        }
    }
    for (child_match, child) in child_matches.iter().zip(expected_children) {
        let (binding_index, binding) = child_match_binding(block, child_match)
            .ok_or_else(|| format!("normal {variant} child result has no unique top-level binding"))?;
        if binding != *child {
            return Err(format!("normal {variant} child result binding changed"));
        }
        if has_assignment_or_local_shadow(&block.stmts[binding_index + 1..lookup_index], &binding) {
            return Err(format!("normal {variant} child result is reassigned or shadowed"));
        }
        for (index, statement) in block.stmts.iter().enumerate().take(lookup_index).skip(binding_index + 1) {
            if !child_binding_indices.contains(&index) && statement_contains_return(statement) {
                return Err(format!("normal {variant} arm can return before its lookup"));
            }
        }
    }
    Ok(())
}

fn normal_field_fast_path_is_valid(statement: &Stmt) -> bool {
    struct RecordLookupLetFinder<'ast> {
        matches: Vec<&'ast syn::ExprLet>,
    }
    impl<'ast> Visit<'ast> for RecordLookupLetFinder<'ast> {
        fn visit_expr_let(&mut self, expression: &'ast syn::ExprLet) {
            if let Expr::Call(call) = expression.expr.as_ref() {
                if matches!(&*call.func, Expr::Path(path)
                    if path_ends_with_segments(&path.path, &["lowered_record_field_value"]))
                    && call.args.len() == 2
                    && path_is_ident(&call.args[0], "base")
                    && path_is_ident(&call.args[1], "name")
                {
                    self.matches.push(expression);
                }
            }
            visit::visit_expr_let(self, expression);
        }
    }
    struct ReturnPathFinder<'ast> {
        expected: &'ast str,
        returns: usize,
        matches: usize,
    }
    impl<'ast> Visit<'ast> for ReturnPathFinder<'ast> {
        fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
            self.returns += 1;
            if matches!(expression.expr.as_deref(), Some(Expr::Call(ok))
                if path_is_ident(&ok.func, "Ok") && ok.args.len() == 1
                    && matches!(&ok.args[0], Expr::Call(flow)
                        if matches!(&*flow.func, Expr::Path(path)
                            if path_ends_with_segments(&path.path, &["ControlFlow", "Continue"]))
                        && flow.args.len() == 1 && path_is_ident(&flow.args[0], self.expected)))
            {
                self.matches += 1;
            }
            visit::visit_expr_return(self, expression);
        }
    }
    struct IfPathFinder<'ast> {
        value: &'ast str,
        lookup_let: &'ast syn::ExprLet,
        valid: bool,
        count: usize,
    }
    struct ExprLetPointerFinder<'ast> {
        target: &'ast syn::ExprLet,
        found: bool,
    }
    impl<'ast> Visit<'ast> for ExprLetPointerFinder<'ast> {
        fn visit_expr_let(&mut self, expression: &'ast syn::ExprLet) {
            if std::ptr::eq(expression, self.target) {
                self.found = true;
            }
            visit::visit_expr_let(self, expression);
        }
    }
    impl<'ast> Visit<'ast> for IfPathFinder<'ast> {
        fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
            let mut condition_finder = ExprLetPointerFinder { target: self.lookup_let, found: false };
            condition_finder.visit_expr(&expression.cond);
            let mut finder = ReturnPathFinder { expected: self.value, returns: 0, matches: 0 };
            finder.visit_block(&expression.then_branch);
            if condition_finder.found {
                self.count += 1;
                if expression.else_branch.is_none() && finder.returns == 1 && finder.matches == 1 {
                    self.valid = true;
                }
            }
            visit::visit_expr_if(self, expression);
        }
    }

    let mut let_finder = RecordLookupLetFinder { matches: Vec::new() };
    let_finder.visit_stmt(statement);
    let [lookup_let] = let_finder.matches.as_slice() else { return false; };
    let Pat::TupleStruct(pattern) = lookup_let.pat.as_ref() else { return false; };
    let Some(Pat::Ident(value)) = pattern.elems.first() else { return false; };
    if !path_ends_with_segments(&pattern.path, &["Some"]) || pattern.elems.len() != 1 {
        return false;
    }
    let value = value.ident.to_string();
    let mut if_finder = IfPathFinder { value: &value, lookup_let, valid: false, count: 0 };
    if_finder.visit_stmt(statement);
    if_finder.count == 1 && if_finder.valid && return_count_in_stmt(statement) == 1
}

fn return_count_in_stmt(statement: &Stmt) -> usize {
    struct ReturnCounter(usize);
    impl<'ast> Visit<'ast> for ReturnCounter {
        fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
            self.0 += 1;
            visit::visit_expr_return(self, expression);
        }
    }
    let mut counter = ReturnCounter(0);
    counter.visit_stmt(statement);
    counter.0
}

fn return_control_flow_breaks(expression: &Expr, value: &str) -> bool {
    matches!(expression, Expr::Return(returned)
        if matches!(returned.expr.as_deref(), Some(Expr::Call(ok))
            if path_is_ident(&ok.func, "Ok") && ok.args.len() == 1
                && matches!(&ok.args[0], Expr::Call(result)
                    if matches!(&*result.func, Expr::Path(path)
                        if path_ends_with_segments(&path.path, &["ControlFlow", "Break"]))
                    && result.args.len() == 1 && path_is_ident(&result.args[0], value))))
}

fn field_index_view_let_else<'ast>(
    body: &'ast Expr,
    variant: &str,
    explicit: bool,
) -> Result<ExprViewLetElse<'ast>, String> {
    let mut finder = ExprViewLetElseFinder { candidates: Vec::new() };
    finder.visit_expr(body);
    let [candidate] = finder.candidates.as_slice() else {
        return Err(format!("{variant} route must have one typed-view let-else"));
    };
    let span_binding = if explicit { "value_span" } else { "span" };
    let index_binding = if explicit { "instruction" } else { "index" };
    let expected_fields: &[(&str, &str)] = match variant {
        "Field" => &[("base", "base"), ("name", "name"), ("span", span_binding)],
        "Index" => &[("base", "base"), ("index", index_binding), ("span", span_binding)],
        "IndexFromEnd" => &[("base", "base"), ("distance", "distance"), ("span", span_binding)],
        _ => return Err(format!("unknown field/index variant {variant}")),
    };
    let enum_path = if explicit { "ExprView" } else { "ExprView" };
    if !struct_view_pattern_matches(candidate.pattern, enum_path, variant, expected_fields) {
        return Err(format!("{variant} route decodes an unexpected typed view"));
    }
    if !is_unconditional_unreachable(candidate.fallback, field_index_unreachable_message(variant)) {
        return Err(format!("{variant} typed-view fallback is not unconditional"));
    }
    let decodes = call_nodes_in_expr(body, "decode");
    if decodes.len() != 1
        || !matches!(&*decodes[0].func, Expr::Path(path)
            if path_ends_with_segments(&path.path, &["ExprView", "decode"]))
        || decodes[0].args.len() != 3
        || !is_reference_to(&decodes[0].args[0], "execution")
        || !path_is_ident(&decodes[0].args[1], "tag")
        || !path_is_ident(&decodes[0].args[2], "payload")
    {
        return Err(format!("{variant} route does not decode its typed view once"));
    }
    Ok(*candidate)
}

fn struct_view_pattern_matches(
    pattern: &Pat,
    enum_name: &str,
    variant: &str,
    expected_fields: &[(&str, &str)],
) -> bool {
    let Pat::Struct(pattern) = pattern else { return false; };
    if !path_ends_with_segments(&pattern.path, &[enum_name, variant])
        || pattern.rest.is_some()
        || pattern.fields.len() != expected_fields.len()
    {
        return false;
    }
    expected_fields.iter().all(|(field_name, binding_name)| {
        pattern.fields.iter().any(|field| {
            matches!(&field.member, syn::Member::Named(name) if name == field_name)
                && matches!(&*field.pat, Pat::Ident(binding)
                    if binding.ident == *binding_name
                        && binding.by_ref.is_none()
                        && binding.mutability.is_none()
                        && binding.subpat.is_none())
        })
    })
}

fn field_index_unreachable_message(variant: &str) -> &'static str {
    match variant {
        "Field" => "field tag has a field view",
        "Index" => "index tag has an index view",
        "IndexFromEnd" => "index-from-end tag has an index-from-end view",
        _ => "unknown field/index view",
    }
}

fn top_level_local_is_raw_field(body: &Expr, binding: &str, source: &str) -> bool {
    let Expr::Block(block) = body else { return false; };
    let initializers = block.block.stmts.iter().filter_map(|statement| match statement {
        Stmt::Local(local) if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == binding) =>
            local.init.as_ref().map(|init| init.expr.as_ref()),
        _ => None,
    }).collect::<Vec<_>>();
    matches!(initializers.first(), Some(Expr::MethodCall(call))
        if call.method == "raw" && call.args.is_empty() && path_is_ident(&call.receiver, source))
}

fn top_level_local_is_method(body: &Expr, binding: &str, source: &str, method: &str) -> bool {
    let Expr::Block(block) = body else { return false; };
    let initializers = block.block.stmts.iter().filter_map(|statement| match statement {
        Stmt::Local(local) if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == binding) =>
            local.init.as_ref().map(|init| init.expr.as_ref()),
        _ => None,
    }).collect::<Vec<_>>();
    matches!(initializers.first(), Some(Expr::MethodCall(call))
        if call.method == method && call.args.is_empty() && path_is_ident(&call.receiver, source))
}

fn normal_field_access_dataflow(body: &Expr) -> bool {
    let chain = call_nodes_in_expr(body, "indexed_field_chain_ref");
    let record = call_nodes_in_expr(body, "lowered_record_field_value");
    let lookup = method_call_nodes_in_expr(body, "indexed_field_value");
    chain.len() == 1
        && matches!(&*chain[0].func, Expr::Path(path)
            if path_ends_with_segments(&path.path, &["indexed_field_chain_ref"]))
        && chain[0].args.len() == 4
        && is_reference_to(&chain[0].args[0], "execution")
        && path_is_ident(&chain[0].args[1], "base")
        && path_is_ident(&chain[0].args[2], "slots")
        && path_is_ident(&chain[0].args[3], "span")
        && record.len() == 1
        && record[0].args.len() == 2
        && path_is_ident(&record[0].args[0], "base")
        && path_is_ident(&record[0].args[1], "name")
        && lookup.len() == 1
        && path_is_ident(&lookup[0].receiver, "self")
        && lookup[0].args.len() == 3
        && path_is_ident(&lookup[0].args[0], "base")
        && path_is_ident(&lookup[0].args[1], "name")
        && path_is_ident(&lookup[0].args[2], "span")
}

fn normal_index_access_dataflow(body: &Expr) -> bool {
    let lookups = call_nodes_in_expr(body, "lowered_index_value");
    lookups.len() == 1
        && lookups[0].args.len() == 3
        && path_is_ident(&lookups[0].args[0], "base")
        && path_is_ident(&lookups[0].args[1], "index")
        && path_is_ident(&lookups[0].args[2], "span")
}

fn normal_index_from_end_dataflow(body: &Expr) -> bool {
    let lookups = call_nodes_in_expr(body, "lowered_index_from_end_value");
    lookups.len() == 1
        && lookups[0].args.len() == 3
        && path_is_ident(&lookups[0].args[0], "base")
        && matches!(&lookups[0].args[1], Expr::MethodCall(call)
            if call.method == "get" && call.args.is_empty() && path_is_ident(&call.receiver, "distance"))
        && path_is_ident(&lookups[0].args[2], "span")
}

fn field_index_normal_view_fallbacks(
    file: &File,
    arms: &[FieldIndexArm],
) -> Result<Vec<FieldIndexFallback>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    FIELD_INDEX_ROWS
        .iter()
        .map(|(variant, _, _)| {
            let arm = arms.iter().find(|arm| arm.variant == *variant)
                .ok_or_else(|| format!("normal field/index route has no {variant} arm"))?;
            let source_arm = dispatch.arms.get(arm.arm_index)
                .ok_or_else(|| format!("normal {variant} arm index is absent"))?;
            let candidate = field_index_view_let_else(&source_arm.body, variant, false)?;
            let message = field_index_unreachable_message(variant);
            if !is_unconditional_unreachable(candidate.fallback, message) {
                return Err(format!("normal {variant} typed-view fallback changed"));
            }
            Ok(FieldIndexFallback {
                helper: "eval_indexed_expr_inner".into(),
                outcome: format!("unreachable!: {message}"),
                scope: format!("{variant} tag reaches the normal evaluator with a different typed ExprView"),
                line: candidate.fallback.span().start().line,
                arm_index: arm.arm_index,
            })
        })
        .collect()
}

fn field_index_forced_arms(
    file: &File,
) -> Result<(Vec<FieldIndexArm>, Vec<FieldIndexFallback>, FieldIndexFallback), String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder { family: "ExprViewKind", matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!("eval_expr_view must have one ExprViewKind match, found {}", finder.matches.len()));
    }
    let expected = FIELD_INDEX_ROWS.iter()
        .map(|(variant, tag, _)| (*variant, *tag))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    let mut error_group = None;
    for (arm_index, arm) in finder.matches[0].arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded ExprViewKind field/index arm at line {}", arm.pat.span().start().line));
        }
        if matches!(arm.pat, Pat::Wild(_) | Pat::Ident(_)) {
            return Err("eval_expr_view field/index match has a catch-all arm".into());
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
        let variant_set = variants.iter().cloned().collect::<BTreeSet<_>>();
        if is_frame_strategy_error_return(&arm.body) {
            let expected_variants = expected.keys().map(|variant| (*variant).to_string()).collect::<BTreeSet<_>>();
            if !variant_set.is_disjoint(&expected_variants) {
                return Err("field/index view kind is intercepted by frame_strategy_error".into());
            }
            if error_group.replace(FieldIndexFallback {
                helper: "eval_expr_view".into(),
                outcome: "frame_strategy_error(tag, span)".into(),
                scope: "valid ExprViewKind values without dedicated frame handlers".into(),
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err("ExprViewKind frame_strategy_error group is split".into());
            }
            continue;
        }
        for variant in variants {
            let Some(tag) = expected.get(variant.as_str()) else { continue; };
            if variant_set.len() != 1 {
                return Err(format!("forced {variant} view is grouped with another view kind"));
            }
            let (behavior, message, fallback_line) = field_index_forced_behavior(&variant, &arm.body)?;
            if found.insert(variant.clone(), FieldIndexArm {
                tag: (*tag).into(),
                variant: variant.clone(),
                behavior,
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate forced field/index view {variant}"));
            }
            fallbacks.insert(variant.clone(), FieldIndexFallback {
                helper: "eval_expr_view".into(),
                outcome: format!("unreachable!: {message}"),
                scope: format!("{variant} view-kind arm decodes a different ExprView"),
                line: fallback_line,
                arm_index,
            });
        }
    }
    if found.len() != expected.len() {
        return Err(format!("forced field/index view coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    let error_group = error_group.ok_or_else(|| "ExprViewKind frame_strategy_error group is missing".to_string())?;
    if found.values().any(|arm: &FieldIndexArm| arm.arm_index >= error_group.arm_index) {
        return Err("field/index handler occurs after the frame_strategy_error group".into());
    }
    let arms = FIELD_INDEX_ROWS.iter()
        .map(|(variant, _, _)| found.get(*variant).expect("validated field/index view").clone())
        .collect();
    let typed_fallbacks = FIELD_INDEX_ROWS.iter()
        .map(|(variant, _, _)| fallbacks.get(*variant).expect("validated typed-view fallback").clone())
        .collect();
    Ok((arms, typed_fallbacks, error_group))
}

fn field_index_forced_behavior(
    variant: &str,
    body: &Expr,
) -> Result<(String, &'static str, usize), String> {
    let typed_view = field_index_view_let_else(body, variant, true)?;
    let push = direct_tail_method_call(body, "push_expr")
        .ok_or_else(|| format!("forced {variant} arm does not end by scheduling its child"))?;
    if !path_is_ident(&push.receiver, "self")
        || push.args.len() != 4
        || !path_is_ident(&push.args[0], "index")
        || !(if variant == "Field" {
            path_is_ident(&push.args[1], "base")
        } else {
            method_on_ident(&push.args[1], "base", "raw")
        })
        || !path_is_ident(&push.args[2], "value_span")
    {
        return Err(format!("forced {variant} child scheduling arguments changed"));
    }
    field_index_forced_sources_live(body, variant, typed_view, push)?;
    if variant == "Field" && !top_level_local_is_raw_field(body, "base", "base") {
        return Err(format!("forced {variant} base no longer comes from its typed view"));
    }
    let (behavior, valid_continuation) = match variant {
        "Field" => {
            let name_conversion = top_level_local_is_method(body, "name", "name", "to_string");
            let chain = call_nodes_in_expr(body, "indexed_field_chain_ref");
            let record = call_nodes_in_expr(body, "lowered_record_field_value");
            let fast_push = method_call_nodes_in_expr(body, "push_value");
            let continuation = continuation_struct(&push.args[3], "Field");
            let fast_value = fast_push.first().is_some_and(|call| {
                call.args.get(1).is_some_and(|argument| matches!(argument, Expr::Call(value)
                    if matches!(&*value.func, Expr::Path(path)
                        if path_ends_with_segments(&path.path, &["FrameValue", "Value"]))
                        && value.args.len() == 1 && path_is_ident(&value.args[0], "value")))
            });
            let fast_push_shape = fast_push.len() == 1
                && fast_push[0].args.len() == 3
                && path_is_ident(&fast_push[0].args[0], "index")
                && path_is_ident(&fast_push[0].args[2], "next");
            let continuation_shape = continuation.is_some_and(|fields| {
                struct_expr_has_exact_fields(fields, &[
                    ("name", |value| path_is_ident(value, "name")),
                    ("span", |value| path_is_ident(value, "value_span")),
                    ("next", is_boxed_next),
                ])
            });
            let checks = [
                name_conversion,
                chain.len() == 1,
                chain.first().is_some_and(|call| call.args.len() == 4
                    && is_reference_to(&call.args[0], "execution")
                    && path_is_ident(&call.args[1], "base")
                    && path_is_ident(&call.args[3], "value_span")),
                record.len() == 1,
                record.first().is_some_and(|call| call.args.len() == 2
                    && path_is_ident(&call.args[0], "base")
                    && is_reference_to(&call.args[1], "name")),
                fast_push_shape,
                fast_value,
                continuation_shape,
            ];
            let valid = checks.iter().all(|check| *check);
            if !valid {
                let mut call_names = CallNameFinder::default();
                call_names.visit_expr(body);
                return Err(format!("forced Field fast path or continuation changed: {checks:?}, calls={:?}", call_names.names));
            }
            ("try_record_field_chain_then_schedule_base".to_string(), valid)
        }
        "Index" => {
            let continuation = continuation_struct(&push.args[3], "IndexBase");
            let valid = matches!(continuation, Some(fields)
                if struct_expr_has_exact_fields(fields, &[
                    ("instruction", |value| method_on_ident(value, "instruction", "raw")),
                    ("span", |value| path_is_ident(value, "value_span")),
                    ("next", is_boxed_next),
                ]));
            ("schedule_base_then_index_continuation".to_string(), valid)
        }
        "IndexFromEnd" => {
            let continuation = continuation_struct(&push.args[3], "IndexFromEnd");
            let valid = matches!(continuation, Some(fields)
                if struct_expr_has_exact_fields(fields, &[
                    ("distance", |value| method_on_ident(value, "distance", "get")),
                    ("span", |value| path_is_ident(value, "value_span")),
                    ("next", is_boxed_next),
                ]));
            ("schedule_base_then_from_end_continuation".to_string(), valid)
        }
        _ => return Err(format!("unknown field/index view {variant}")),
    };
    if !valid_continuation {
        return Err(format!("forced {variant} continuation data flow changed"));
    }
    Ok((behavior, field_index_unreachable_message(variant), typed_view.fallback.span().start().line))
}

fn field_index_forced_sources_live(
    body: &Expr,
    variant: &str,
    view: ExprViewLetElse<'_>,
    push: &syn::ExprMethodCall,
) -> Result<(), String> {
    let Expr::Block(block) = body else {
        return Err(format!("forced {variant} handler must be a block"));
    };
    let view_index = block.block.stmts.iter().position(|statement| match statement {
        Stmt::Local(local) => {
            std::ptr::eq(&local.pat, view.pattern)
                && local.init.as_ref().and_then(|initializer| initializer.diverge.as_ref())
                    .is_some_and(|(_, fallback)| std::ptr::eq(fallback.as_ref(), view.fallback))
        }
        _ => false,
    }).ok_or_else(|| format!("forced {variant} typed view is not a top-level binding"))?;
    let push_index = block.block.stmts.len().checked_sub(1)
        .ok_or_else(|| format!("forced {variant} handler has no scheduling statement"))?;
    if !matches!(block.block.stmts.get(push_index), Some(Stmt::Expr(Expr::MethodCall(call), Some(_)))
        if std::ptr::eq(call, push))
    {
        return Err(format!("forced {variant} scheduling is not the final statement"));
    }
    if view_index >= push_index {
        return Err(format!("forced {variant} schedules before decoding its view"));
    }
    let statements = &block.block.stmts[view_index + 1..push_index];
    let protected: &[&str] = match variant {
        "Field" => &["base", "name", "value_span"],
        "Index" => &["base", "instruction", "value_span"],
        "IndexFromEnd" => &["base", "distance", "value_span"],
        _ => return Err(format!("unknown forced field/index variant {variant}")),
    };
    let allowed_local_counts: BTreeMap<&str, usize> = if variant == "Field" {
        BTreeMap::from([("base", 1), ("name", 1)])
    } else {
        BTreeMap::new()
    };
    struct SourceBindings<'name> {
        protected: &'name [&'name str],
        counts: BTreeMap<String, usize>,
        assignments: BTreeSet<String>,
    }
    impl<'ast> Visit<'ast> for SourceBindings<'_> {
        fn visit_local(&mut self, local: &'ast syn::Local) {
            if let Pat::Ident(binding) = &local.pat {
                let name = binding.ident.to_string();
                if self.protected.contains(&name.as_str()) {
                    *self.counts.entry(name).or_default() += 1;
                }
            }
            visit::visit_local(self, local);
        }

        fn visit_expr_assign(&mut self, expression: &'ast syn::ExprAssign) {
            for name in self.protected {
                if path_is_ident(&expression.left, name) {
                    self.assignments.insert((*name).into());
                }
            }
            visit::visit_expr_assign(self, expression);
        }
    }
    let mut sources = SourceBindings {
        protected,
        counts: BTreeMap::new(),
        assignments: BTreeSet::new(),
    };
    let mut bypass = false;
    for statement in statements {
        sources.visit_stmt(statement);
        if statement_contains_return(statement) {
            if variant == "Field" {
                let push_count = method_call_nodes_in_stmt(statement, "push_value").len();
                if push_count == 0 {
                    bypass = true;
                } else if push_count != 1 || !field_fast_path_push_precedes_return(statement) {
                    return Err("forced Field fast path must push before returning".into());
                }
            } else {
                bypass = true;
            }
        }
    }
    if bypass {
        return Err(format!("forced {variant} handler can return before scheduling"));
    }
    if !sources.assignments.is_empty() {
        return Err(format!("forced {variant} source binding is written"));
    }
    if protected.iter().any(|name| {
        sources.counts.get(*name).copied().unwrap_or(0)
            != allowed_local_counts.get(*name).copied().unwrap_or(0)
    }) {
        return Err(format!("forced {variant} source binding is shadowed or written"));
    }
    if variant == "Field"
        && (!top_level_local_is_raw_field(body, "base", "base")
            || !top_level_local_is_method(body, "name", "name", "to_string"))
    {
        return Err("forced Field child IDs no longer derive from the decoded view".into());
    }
    Ok(())
}

fn field_fast_path_push_precedes_return(statement: &Stmt) -> bool {
    struct FastPathFinder {
        valid_branches: usize,
        return_count: usize,
    }
    impl<'ast> Visit<'ast> for FastPathFinder {
        fn visit_expr_return(&mut self, expression: &'ast syn::ExprReturn) {
            self.return_count += 1;
            visit::visit_expr_return(self, expression);
        }

        fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
            if field_fast_path_if_is_valid(expression) {
                self.valid_branches += 1;
            }
            visit::visit_expr_if(self, expression);
        }
    }
    let mut finder = FastPathFinder { valid_branches: 0, return_count: 0 };
    finder.visit_stmt(statement);
    finder.valid_branches == 1 && finder.return_count == 1
}

fn field_fast_path_if_is_valid(expression: &syn::ExprIf) -> bool {
    let [
        Stmt::Expr(Expr::MethodCall(push), Some(_)),
        Stmt::Expr(Expr::Return(returned), Some(_)),
    ] = expression.then_branch.stmts.as_slice() else {
        return false;
    };
    if push.method != "push_value"
        || !path_is_ident(&push.receiver, "self")
        || push.args.len() != 3
        || !path_is_ident(&push.args[0], "index")
        || !path_is_ident(&push.args[2], "next")
    {
        return false;
    }
    let Expr::Call(frame_value) = &push.args[1] else { return false; };
    if !matches!(&*frame_value.func, Expr::Path(path)
        if path_ends_with_segments(&path.path, &["FrameValue", "Value"]))
        || frame_value.args.len() != 1
    {
        return false;
    }
    let Expr::Path(value_path) = &frame_value.args[0] else { return false; };
    let Some(segment) = value_path.path.segments.last() else { return false; };
    if value_path.path.segments.len() != 1 {
        return false;
    }
    let value = segment.ident.to_string();
    field_fast_path_return_matches(returned)
        && field_fast_path_condition_binds(expression, &value)
}

fn field_fast_path_return_matches(returned: &syn::ExprReturn) -> bool {
    matches!(returned.expr.as_deref(), Some(Expr::Call(ok))
        if path_is_ident(&ok.func, "Ok") && ok.args.len() == 1
            && matches!(&ok.args[0], Expr::Tuple(tuple) if tuple.elems.is_empty()))
}

fn field_fast_path_condition_binds(expression: &syn::ExprIf, value: &str) -> bool {
    struct ValueBindingFinder<'name> {
        expected: &'name str,
        matches: usize,
    }
    impl<'ast> Visit<'ast> for ValueBindingFinder<'_> {
        fn visit_expr_let(&mut self, let_expression: &'ast syn::ExprLet) {
            if matches!(&*let_expression.pat, Pat::TupleStruct(pattern)
                if path_ends_with_segments(&pattern.path, &["Some"])
                    && pattern.elems.len() == 1
                    && matches!(pattern.elems.first(), Some(Pat::Ident(binding))
                        if binding.ident == self.expected))
                && matches!(let_expression.expr.as_ref(), Expr::Call(call)
                    if matches!(&*call.func, Expr::Path(path)
                        if path_ends_with_segments(&path.path, &["lowered_record_field_value"]))
                    && call.args.len() == 2
                    && path_is_ident(&call.args[0], "base")
                    && is_reference_to(&call.args[1], "name"))
            {
                self.matches += 1;
            }
            visit::visit_expr_let(self, let_expression);
        }
    }
    let mut finder = ValueBindingFinder { expected: value, matches: 0 };
    finder.visit_expr(&expression.cond);
    finder.matches == 1
}

fn direct_tail_method_call<'a>(body: &'a Expr, method: &str) -> Option<&'a syn::ExprMethodCall> {
    let Expr::Block(block) = body else { return None; };
    match block.block.stmts.last()? {
        Stmt::Expr(Expr::MethodCall(call), Some(_)) if call.method == method => Some(call),
        _ => None,
    }
}

fn method_on_ident(expression: &Expr, receiver: &str, method: &str) -> bool {
    matches!(expression, Expr::MethodCall(call)
        if call.method == method && call.args.is_empty() && path_is_ident(&call.receiver, receiver))
}

fn is_boxed_next(expression: &Expr) -> bool {
    matches!(expression, Expr::Call(call)
        if path_ends_with_segments(match &*call.func {
            Expr::Path(path) => &path.path,
            _ => return false,
        }, &["Box", "new"])
            && call.args.len() == 1 && path_is_ident(&call.args[0], "next"))
}

fn continuation_struct<'a>(
    expression: &'a Expr,
    variant: &str,
) -> Option<&'a syn::punctuated::Punctuated<syn::FieldValue, syn::token::Comma>> {
    let Expr::Struct(value) = expression else { return None; };
    if !path_ends_with_segments(&value.path, &["FrameContinuation", variant]) {
        return None;
    }
    Some(&value.fields)
}

fn struct_expr_has_exact_fields(
    fields: &syn::punctuated::Punctuated<syn::FieldValue, syn::token::Comma>,
    expected: &[(&str, fn(&Expr) -> bool)],
) -> bool {
    fields.len() == expected.len()
        && expected.iter().all(|(name, validate)| {
            fields.iter().any(|field| {
                matches!(&field.member, syn::Member::Named(ident) if ident == name)
                    && validate(&field.expr)
            })
        })
}

fn field_index_normal_fallback(file: &File) -> Result<FieldIndexFallback, String> {
    let fallback = collection_normal_fallback(file)?;
    Ok(FieldIndexFallback {
        helper: fallback.helper,
        outcome: fallback.outcome,
        scope: fallback.scope,
        line: fallback.line,
        arm_index: fallback.arm_index,
    })
}

fn result_constructor_normal_arms(file: &File) -> Result<Vec<ResultConstructorArm>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    let expected = RESULT_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, tag, _)| (*tag, *variant))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    for (arm_index, arm) in dispatch.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded indexed expression arm at line {}", arm.pat.span().start().line));
        }
        let mut tags = Vec::new();
        if matches!(arm.pat, Pat::Wild(_)) {
            continue;
        }
        collect_exact_tag_patterns(&arm.pat, &mut tags)?;
        for (tag, line) in tags.iter().filter(|(tag, _)| expected.contains_key(tag.as_str())) {
            if tags.len() != 1 {
                return Err(format!("result constructor {tag} is grouped with another dispatch tag"));
            }
            let variant = expected[tag.as_str()];
            let behavior = result_constructor_normal_behavior(variant, &arm.body)?;
            if found.insert(tag.clone(), ResultConstructorArm {
                tag: tag.clone(),
                variant: variant.into(),
                behavior,
                line: *line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate normal result constructor arm {tag}"));
            }
        }
    }
    if found.len() != expected.len() {
        return Err(format!("normal result constructor coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    Ok(RESULT_CONSTRUCTOR_ROWS
        .iter()
        .map(|(_, tag, _)| found.get(*tag).expect("validated result constructor branch").clone())
        .collect())
}

fn result_constructor_normal_behavior(variant: &str, body: &Expr) -> Result<String, String> {
    validate_result_constructor_decode(variant, body)?;
    validate_result_constructor_view_pattern(variant, body)?;
    let mut child_matches = ChildEvaluationMatchFinder { matches: Vec::new() };
    child_matches.visit_expr(body);
    let expected_evaluations = if variant == "Ok" { 1 } else { 2 };
    if child_matches.matches.len() != expected_evaluations {
        return Err(format!("{variant} normal arm evaluates {} children, expected {expected_evaluations}", child_matches.matches.len()));
    }

    match variant {
        "Ok" => validate_normal_ok_result(body, &child_matches.matches)?,
        "Err" => validate_normal_err_result(body, &child_matches.matches)?,
        _ => return Err(format!("unknown result constructor {variant}")),
    }
    Ok(match variant {
        "Ok" => "evaluate_child_then_wrap_result_ok",
        "Err" => "evaluate_error_then_optional_cause_before_constructing_error",
        _ => unreachable!("result constructor variants were closed above"),
    }.into())
}

fn validate_result_constructor_decode(variant: &str, body: &Expr) -> Result<(), String> {
    let decode = call_nodes_in_expr(body, "decode");
    if decode.len() != 1
        || !path_has_segments(&decode[0].func, &["ExprView", "decode"])
        || decode[0].args.len() != 3
        || !path_is_ident(&decode[0].args[0], "execution")
        || !path_is_ident(&decode[0].args[1], "tag")
        || !path_is_ident(&decode[0].args[2], "payload")
    {
        return Err(format!("{variant} normal route does not decode its typed view once"));
    }
    Ok(())
}

fn validate_result_constructor_view_pattern(variant: &str, body: &Expr) -> Result<(), String> {
    result_constructor_view_let_else(variant, body).map(|_| ())
}

fn result_constructor_view_let_else<'ast>(
    variant: &str,
    body: &'ast Expr,
) -> Result<ExprViewLetElse<'ast>, String> {
    let mut finder = ExprViewLetElseFinder { candidates: Vec::new() };
    finder.visit_expr(body);
    if finder.candidates.len() != 1 {
        return Err(format!("{variant} route must have one typed-view let-else"));
    }
    let candidate = &finder.candidates[0];
    if !result_constructor_pattern_matches(candidate.pattern, variant) {
        return Err(format!("{variant} route decodes an unexpected typed view"));
    }
    Ok(*candidate)
}

fn result_constructor_pattern_matches(pattern: &Pat, variant: &str) -> bool {
    let view_path = |path: &syn::Path| path_ends_with_segments(path, &["ExprView", variant]);
    match (variant, pattern) {
        ("Ok", Pat::TupleStruct(pattern)) => {
            view_path(&pattern.path)
                && pattern.elems.len() == 1
                && matches!(pattern.elems.first(), Some(Pat::Ident(binding)) if binding.ident == "value")
        }
        ("Err", Pat::Struct(pattern)) => {
            if !view_path(&pattern.path) || pattern.rest.is_some() || pattern.fields.len() != 2 {
                return false;
            }
            let mut fields = BTreeSet::new();
            for field in &pattern.fields {
                let syn::Member::Named(name) = &field.member else { return false; };
                if !matches!(&*field.pat, Pat::Ident(binding) if binding.ident == *name) {
                    return false;
                }
                fields.insert(name.to_string());
            }
            fields == BTreeSet::from(["value".to_string(), "cause".to_string()])
        }
        _ => false,
    }
}

fn result_constructor_normal_view_fallbacks(
    file: &File,
    normal_arms: &[ResultConstructorArm],
) -> Result<Vec<ResultConstructorFallback>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    RESULT_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, _, _)| {
            let arm = normal_arms.iter().find(|arm| arm.variant == *variant)
                .ok_or_else(|| format!("normal result route has no {variant} arm"))?;
            let source_arm = dispatch.arms.get(arm.arm_index)
                .ok_or_else(|| format!("normal {variant} arm index is absent"))?;
            let candidate = result_constructor_view_let_else(variant, &source_arm.body)?;
            let message = result_constructor_unreachable_message(variant);
            if !is_unreachable_block(candidate.fallback, message) {
                return Err(format!("normal {variant} typed-view fallback changed"));
            }
            Ok(ResultConstructorFallback {
                helper: "eval_indexed_expr_inner".into(),
                outcome: format!("unreachable!: {message}"),
                scope: format!("{variant} tag reaches the normal evaluator with a different typed ExprView"),
                line: candidate.fallback.span().start().line,
                arm_index: arm.arm_index,
            })
        })
        .collect()
}

fn result_constructor_unreachable_message(variant: &str) -> &'static str {
    match variant {
        "Ok" => "ok tag has an ok view",
        "Err" => "err tag has an err view",
        _ => unreachable!("result constructor variants were closed above"),
    }
}

fn eval_child_uses_raw_input(child: &syn::ExprMatch, expected: &str) -> bool {
    let calls = method_call_nodes_in_expr(&child.expr, "eval_indexed_expr");
    calls.len() == 1
        && calls[0].method == "eval_indexed_expr"
        && path_is_ident(&calls[0].receiver, "self")
        && calls[0].args.len() == 4
        && path_is_ident(&calls[0].args[0], "execution")
        && is_raw_call_on(&calls[0].args[1], expected)
        && path_is_ident(&calls[0].args[2], "slots")
        && path_is_ident(&calls[0].args[3], "call_span")
}

fn is_raw_call_on(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::MethodCall(call)
        if call.method == "raw" && call.args.is_empty() && path_is_ident(&call.receiver, expected))
}

fn validate_normal_ok_result(body: &Expr, child_matches: &[&syn::ExprMatch]) -> Result<(), String> {
    let Expr::Block(block) = body else {
        return Err("Ok normal arm must bind its child result in a block".into());
    };
    let (value_index, value_initializer, value_match) = direct_block_child_match(&block.block, "value")
        .ok_or_else(|| "Ok normal arm has no direct value child binding".to_string())?;
    let (value, continue_arm) = continue_value_arm(child_matches[0])?;
    let result = call_nodes_in_expr(body, "ResultOk");
    let box_new = result.first().and_then(|call| call.args.first()).and_then(|value| exact_call(value, &["Box", "new"]));
    let final_expression = block.block.stmts.last().and_then(|statement| match statement {
        Stmt::Expr(expression, None) => Some(expression),
        _ => None,
    });
    let final_continue = final_expression.and_then(|expression| exact_call(expression, &["ControlFlow", "Continue"]));
    let final_result = final_continue.and_then(|call| call.args.first()).and_then(|value| exact_call(value, &["LoweredValue", "ResultOk"]));
    if value_index >= block.block.stmts.len()
        || !std::ptr::eq(value_match, child_matches[0])
        || !matches!(value_initializer, Expr::Match(expression) if std::ptr::eq(expression, value_match))
        || !eval_child_uses_raw_input(child_matches[0], "value")
        || !path_is_ident(&continue_arm.body, &value)
        || result.len() != 1
        || box_new.is_none_or(|call| call.args.len() != 1 || !path_is_ident(&call.args[0], &value))
        || final_result.is_none_or(|call| call.args.len() != 1
            || exact_call(&call.args[0], &["Box", "new"]).is_none())
        || final_result.and_then(|call| call.args.first()).and_then(|value| exact_call(value, &["Box", "new"]))
            .is_none_or(|call| call.args.len() != 1 || !path_is_ident(&call.args[0], &value))
    {
        return Err("Ok child result does not flow to ResultOk construction".into());
    }
    Ok(())
}

fn validate_normal_err_result(body: &Expr, child_matches: &[&syn::ExprMatch]) -> Result<(), String> {
    let Expr::Block(block) = body else {
        return Err("Err normal arm must bind its children in a block".into());
    };
    let (value_index, value_initializer, value_match) = direct_block_child_match(&block.block, "value")
        .ok_or_else(|| "Err normal arm has no value child binding".to_string())?;
    let (cause_index, cause_initializer, cause_match) = direct_block_child_match(&block.block, "cause")
        .ok_or_else(|| "Err normal arm has no optional cause child binding".to_string())?;
    let (value, value_continue) = continue_value_arm(child_matches[0])?;
    let (cause_value, cause_continue) = continue_value_arm(child_matches[1])?;
    let into_value = method_call_nodes_in_expr(&value_continue.body, "into_value");
    let optional_cause = method_call_nodes_in_expr(cause_initializer, "map");
    let some = call_nodes_in_expr(&cause_continue.body, "Some");
    let result = call_nodes_in_expr(body, "lowered_err_with_cause");
    let final_expression = block.block.stmts.last().and_then(|statement| match statement {
        Stmt::Expr(expression, None) => Some(expression),
        _ => None,
    });
    let final_continue = final_expression.and_then(|expression| exact_call(expression, &["ControlFlow", "Continue"]));
    if value_index >= cause_index
        || !std::ptr::eq(value_match, child_matches[0])
        || !std::ptr::eq(cause_match, child_matches[1])
        || !matches!(value_initializer, Expr::Match(expression) if std::ptr::eq(expression, value_match))
        || !matches!(cause_initializer, Expr::Match(_))
        || !contains_child_match(cause_initializer, cause_match)
        || !eval_child_uses_raw_input(child_matches[0], "value")
        || !eval_child_uses_path_input(child_matches[1], "cause")
        || into_value.len() != 1
        || !path_is_ident(&into_value[0].receiver, &value)
        || optional_cause.len() != 1
        || !is_option_raw_mapping(&optional_cause[0], "cause")
        || some.len() != 1
        || some[0].args.len() != 1
        || !path_is_ident(&some[0].args[0], &cause_value)
        || result.len() != 1
        || result[0].args.len() != 3
        || !path_is_ident(&result[0].args[0], "value")
        || !path_is_ident(&result[0].args[1], "cause")
        || !path_is_ident(&result[0].args[2], "call_span")
        || final_continue.is_none_or(|call| call.args.len() != 1
            || !matches!(&call.args[0], Expr::Try(expression)
                if matches!(expression.expr.as_ref(), Expr::Call(result_call)
                    if std::ptr::eq(result_call, result[0]))))
    {
        return Err("Err value and optional cause do not flow in source order to error construction".into());
    }
    Ok(())
}

fn is_option_raw_mapping(expression: &syn::ExprMethodCall, expected: &str) -> bool {
    expression.method == "map"
        && path_is_ident(&expression.receiver, expected)
        && expression.args.len() == 1
        && matches!(expression.args.first(), Some(Expr::Closure(closure))
            if closure.inputs.len() == 1
                && matches!(closure.inputs.first(), Some(Pat::Ident(binding)) if binding.ident == expected)
                && is_raw_call_on(&closure.body, expected))
}

fn eval_child_uses_path_input(child: &syn::ExprMatch, expected: &str) -> bool {
    let calls = method_call_nodes_in_expr(&child.expr, "eval_indexed_expr");
    calls.len() == 1
        && path_is_ident(&calls[0].receiver, "self")
        && calls[0].args.len() == 4
        && path_is_ident(&calls[0].args[0], "execution")
        && path_is_ident(&calls[0].args[1], expected)
        && path_is_ident(&calls[0].args[2], "slots")
        && path_is_ident(&calls[0].args[3], "call_span")
}

fn result_constructor_forced_arms(
    file: &File,
) -> Result<(Vec<ResultConstructorArm>, Vec<ResultConstructorFallback>, ResultConstructorFallback), String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder { family: "ExprViewKind", matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!("eval_expr_view must have one ExprViewKind match, found {}", finder.matches.len()));
    }
    let expected = RESULT_CONSTRUCTOR_ROWS.iter()
        .map(|(variant, tag, _)| (*variant, *tag))
        .collect::<BTreeMap<_, _>>();
    let expected_variants = expected.keys().copied().collect::<BTreeSet<_>>();
    let mut found = BTreeMap::new();
    let mut view_fallbacks = Vec::new();
    let mut error_group = None;
    for (arm_index, arm) in finder.matches[0].arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded ExprViewKind dispatch arm at line {}", arm.pat.span().start().line));
        }
        if matches!(arm.pat, Pat::Wild(_) | Pat::Ident(_)) {
            return Err("eval_expr_view result constructor dispatch has a catch-all arm".into());
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
        if is_frame_strategy_error_return(&arm.body) {
            if variants.iter().any(|variant| expected_variants.contains(variant.as_str())) {
                return Err("result constructor is intercepted by frame_strategy_error".into());
            }
            if error_group.is_some() {
                return Err("ExprViewKind frame_strategy_error group is split".into());
            }
            error_group = Some(ResultConstructorFallback {
                helper: "eval_expr_view".into(),
                outcome: "frame_strategy_error(tag, span)".into(),
                scope: "valid ExprViewKind values without dedicated frame handlers".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
            continue;
        }
        for variant in variants.iter().filter(|variant| expected_variants.contains(variant.as_str())) {
            if variants.len() != 1 {
                return Err(format!("result constructor {variant} forced arm is grouped with another view"));
            }
            let behavior = result_constructor_forced_behavior(variant, &arm.body)?;
            if found.insert(variant.clone(), ResultConstructorArm {
                tag: expected[variant.as_str()].into(),
                variant: variant.clone(),
                behavior,
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate forced result constructor arm {variant}"));
            }
            let candidate = result_constructor_view_let_else(variant, &arm.body)?;
            let message = result_constructor_unreachable_message(variant);
            if !is_unreachable_block(candidate.fallback, message) {
                return Err(format!("forced {variant} typed-view fallback changed"));
            }
            view_fallbacks.push(ResultConstructorFallback {
                helper: "eval_expr_view".into(),
                outcome: format!("unreachable!: {message}"),
                scope: format!("{variant} view-kind arm decodes a different ExprView"),
                line: candidate.fallback.span().start().line,
                arm_index,
            });
        }
    }
    if found.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected_variants {
        return Err(format!("forced result constructor coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    let error_group = error_group.ok_or_else(|| "ExprViewKind frame_strategy_error group is missing".to_string())?;
    if found.values().any(|arm| arm.arm_index >= error_group.arm_index) {
        return Err("result constructor handler occurs after the frame_strategy_error group".into());
    }
    Ok((
        RESULT_CONSTRUCTOR_ROWS.iter()
            .map(|(variant, _, _)| found.get(*variant).expect("validated result constructor view").clone())
            .collect(),
        view_fallbacks,
        error_group,
    ))
}

fn result_constructor_forced_behavior(variant: &str, body: &Expr) -> Result<String, String> {
    let candidate = result_constructor_view_let_else(variant, body)?;
    let decode = call_nodes_in_expr(body, "decode");
    let push = method_call_nodes_in_expr(body, "push_expr");
    let decoded = decode.first().is_some_and(|call| {
        matches!(&*call.func, Expr::Path(path) if path_ends_with_segments(&path.path, &["ExprView", "decode"]))
            && call.args.len() == 3
            && matches!(&call.args[0], Expr::Reference(reference)
                if reference.mutability.is_none() && path_is_ident(&reference.expr, "execution"))
            && path_is_ident(&call.args[1], "tag")
            && path_is_ident(&call.args[2], "payload")
    });
    if decode.len() != 1
        || !decoded
        || push.len() != 1
        || push[0].args.len() != 4
        || !path_is_ident(&push[0].receiver, "self")
        || !path_is_ident(&push[0].args[0], "index")
        || !is_raw_call_on(&push[0].args[1], "value")
        || !path_is_ident(&push[0].args[2], "span")
        || !result_constructor_continuation_matches(variant, &push[0].args[3])
        || !is_unreachable_block(candidate.fallback, result_constructor_unreachable_message(variant))
    {
        return Err(format!("forced {variant} handler does not push its dedicated result continuation"));
    }
    Ok(match variant {
        "Ok" => "push_child_with_wrap_ok_continuation",
        "Err" => "push_value_with_optional_cause_wrap_err_continuation",
        _ => return Err(format!("unknown forced result constructor {variant}")),
    }.into())
}

fn result_constructor_continuation_matches(variant: &str, expression: &Expr) -> bool {
    match variant {
        "Ok" => exact_call(expression, &["FrameContinuation", "WrapOk"])
            .is_some_and(|wrap| wrap.args.len() == 1 && boxed_next(&wrap.args[0])),
        "Err" => {
            let Expr::Struct(continuation) = expression else { return false; };
            if !path_ends_with_segments(&continuation.path, &["FrameContinuation", "WrapErr"])
                || continuation.rest.is_some()
                || continuation.fields.len() != 3
            {
                return false;
            }
            let field = |name: &str| continuation.fields.iter().find_map(|field| {
                matches!(&field.member, syn::Member::Named(ident) if ident == name).then_some(&field.expr)
            });
            field("cause").is_some_and(|cause| is_option_raw_mapping_expr(cause, "cause"))
                && field("span").is_some_and(|span| path_is_ident(span, "span"))
                && field("next").is_some_and(boxed_next)
        }
        _ => false,
    }
}

fn boxed_next(expression: &Expr) -> bool {
    exact_call(expression, &["Box", "new"])
        .is_some_and(|call| call.args.len() == 1 && path_is_ident(&call.args[0], "next"))
}

fn is_option_raw_mapping_expr(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::MethodCall(call) if is_option_raw_mapping(call, expected))
}

fn collection_construction_schema_rows(
    expression_rows: &[ParsedExprSchemaRow],
) -> Result<Vec<FrameConstructorSchemaRow>, String> {
    let expected = COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(variant, tag, ordinal, strategy)| (*variant, *tag, *ordinal, *strategy))
        .collect::<BTreeSet<_>>();
    let actual = expression_rows
        .iter()
        .filter(|row| {
            COLLECTION_CONSTRUCTION_ROWS
                .iter()
                .any(|(_, tag, _, _)| *tag == row.tag)
        })
        .map(|row| {
            (
                row.variant.as_str(),
                row.tag.as_str(),
                row.ordinal,
                row.strategy.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "collection construction schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(variant, tag, ordinal, strategy)| {
            let row = expression_rows
                .iter()
                .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
                .ok_or_else(|| format!("missing collection construction schema row {tag}"))?;
            if row.effects != "0"
                || row.class != "Operands"
                || row.strategy != *strategy
                || row.classification.is_some()
            {
                return Err(format!(
                    "collection construction schema row {tag} has unexpected effects, class, strategy, or classification"
                ));
            }
            Ok(FrameConstructorSchemaRow {
                variant: row.variant.clone(),
                tag: row.tag.clone(),
                ordinal: row.ordinal,
                effects: row.effects.clone(),
                class: row.class.clone(),
                strategy: row.strategy.clone(),
                line: row.line,
            })
        })
        .collect()
}

fn collection_normal_arms(file: &File) -> Result<Vec<CollectionDispatchArm>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    let expected = COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(variant, tag, _, _)| (*tag, *variant))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    for (arm_index, arm) in dispatch.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded indexed expression arm at line {}", arm.pat.span().start().line));
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            continue;
        }
        let mut tags = Vec::new();
        collect_exact_tag_patterns(&arm.pat, &mut tags)?;
        for (tag, line) in tags.iter().filter(|(tag, _)| expected.contains_key(tag.as_str())) {
            if tags.len() != 1 {
                return Err(format!("collection constructor {tag} is grouped with another dispatch tag"));
            }
            let variant = expected[tag.as_str()];
            let behavior = collection_normal_arm_behavior(variant, &arm.body)?;
            if found.insert(tag.clone(), CollectionDispatchArm {
                tag: tag.clone(),
                variant: variant.into(),
                behavior,
                line: *line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate normal collection constructor arm {tag}"));
            }
        }
    }
    if found.len() != expected.len() {
        return Err(format!("normal collection constructor coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    Ok(COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(_, tag, _, _)| found.get(*tag).expect("validated constructor branch").clone())
        .collect())
}

fn top_level_tag_dispatch_match<'a>(block: &'a Block) -> Result<(usize, &'a syn::ExprMatch), String> {
    let index = top_level_tag_match_index(block)?;
    let Stmt::Local(local) = &block.stmts[index] else {
        return Err("indexed evaluator tag match is not a local initializer".into());
    };
    let expression = local.init.as_ref().ok_or_else(|| "indexed evaluator tag match has no initializer".to_string())?.expr.as_ref();
    let Expr::Match(dispatch) = expression else {
        return Err("indexed evaluator tag initializer is not a match".into());
    };
    Ok((index, dispatch))
}

fn collection_normal_arm_behavior(variant: &str, body: &Expr) -> Result<String, String> {
    validate_collection_normal_decode(variant, body)?;
    let mut child_matches = ChildEvaluationMatchFinder { matches: Vec::new() };
    child_matches.visit_expr(body);
    let expected_evaluations = match variant {
        "List" | "ListBuild" | "Record" => 1,
        "MapLiteral" | "RecordUpdate" => 2,
        _ => return Err(format!("unknown collection constructor {variant}")),
    };
    if child_matches.matches.len() != expected_evaluations {
        return Err(format!("{variant} normal arm evaluates {} child expressions, expected {expected_evaluations}", child_matches.matches.len()));
    }
    let mut calls = CallNameFinder::default();
    calls.visit_expr(body);
    let names = calls.names.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    let require_count = |name: &str, count: usize| {
        let actual = names.iter().filter(|actual| **actual == name).count();
        (actual == count).then_some(()).ok_or_else(|| format!("{variant} normal arm calls {name} {actual} times, expected {count}"))
    };
    match variant {
        "List" | "ListBuild" => {
            require_count("append_lowered_list_element", 1)?;
            validate_collection_list_dataflow(body, variant, &child_matches.matches)?;
        }
        "MapLiteral" => {
            require_count("lowered_map_literal_key", 1)?;
            require_count("append_lowered_map_literal", 1)?;
            validate_collection_map_dataflow(body, &child_matches.matches)?;
        }
        "Record" => {
            require_count("append_indexed_record_entry", 1)?;
            require_count("finish_record_entries", 1)?;
            validate_collection_record_dataflow(body, &child_matches.matches)?;
        }
        "RecordUpdate" => {
            require_count("decode_record_updates", 1)?;
            require_count("lowered_record_update_batch", 1)?;
            validate_collection_update_dataflow(body, &child_matches.matches)?;
        }
        _ => unreachable!("constructor variants were closed above"),
    }
    Ok(match variant {
        "List" => "evaluate_each_item_then_append_without_splicing",
        "ListBuild" => "evaluate_each_item_then_append_with_encoded_splice_policy",
        "MapLiteral" => "evaluate_optional_key_before_value_then_append_entry",
        "Record" => "evaluate_each_entry_then_append_record_field_or_spread",
        "RecordUpdate" => "evaluate_base_then_update_values_before_batch_replacement",
        _ => unreachable!("constructor variants were closed above"),
    }.into())
}

fn validate_collection_normal_decode(variant: &str, body: &Expr) -> Result<(), String> {
    if variant == "RecordUpdate" {
        let decode = call_nodes_in_expr(body, "decode_record_updates");
        if decode.len() != 1 || decode[0].args.len() != 4
            || !path_is_ident(&decode[0].args[0], "execution")
            || !path_is_ident(&decode[0].args[1], "tag")
            || !path_is_ident(&decode[0].args[2], "payload")
            || !path_is_ident(&decode[0].args[3], "call_span")
        {
            return Err("RecordUpdate normal route does not decode its typed view once".into());
        }
        return Ok(());
    }
    let decode = call_nodes_in_expr(body, "decode");
    if decode.len() != 1 || !path_has_segments(&decode[0].func, &["ExprView", "decode"])
        || decode[0].args.len() != 3
        || !path_is_ident(&decode[0].args[0], "execution")
        || !path_is_ident(&decode[0].args[1], "tag")
        || !path_is_ident(&decode[0].args[2], "payload")
    {
        return Err(format!("{variant} normal route does not decode its typed view once"));
    }
    Ok(())
}

fn collection_normal_view_fallbacks(
    file: &File,
    normal_arms: &[CollectionDispatchArm],
) -> Result<Vec<CollectionFallback>, String> {
    let mut fallbacks = Vec::new();
    for variant in ["Record", "MapLiteral", "List", "ListBuild"] {
        let arm = normal_arms.iter().find(|arm| arm.variant == variant)
            .ok_or_else(|| format!("normal collection route has no {variant} arm"))?;
        let block = find_function(file, "eval_indexed_expr_inner")?;
        let (_, dispatch) = top_level_tag_dispatch_match(block)?;
        let source_arm = dispatch.arms.get(arm.arm_index)
            .ok_or_else(|| format!("normal {variant} arm index is absent"))?;
        let mut finder = ExprViewLetElseFinder { candidates: Vec::new() };
        finder.visit_expr(&source_arm.body);
        if finder.candidates.len() != 1 {
            return Err(format!("normal {variant} arm must have one typed-view let-else"));
        }
        let candidate = &finder.candidates[0];
        let mut variants = Vec::new();
        collect_named_view_variants(candidate.pattern, "ExprView", &mut variants)?;
        let expected_message = match variant {
            "Record" => "record literal tag has a typed record view",
            "MapLiteral" => "map literal tag has a typed map view",
            "List" => "list literal tag has a typed list view",
            "ListBuild" => "list build tag has a typed list build view",
            _ => unreachable!("constructor variants were closed above"),
        };
        if variants != [variant] || !is_unreachable_block(candidate.fallback, expected_message) {
            return Err(format!("normal {variant} typed-view fallback changed"));
        }
        fallbacks.push(CollectionFallback {
            helper: "eval_indexed_expr_inner".into(),
            outcome: format!("unreachable!: {expected_message}"),
            scope: format!("{variant} tag reaches the normal evaluator with a different typed ExprView"),
            line: candidate.fallback.span().start().line,
            arm_index: arm.arm_index,
        });
    }
    Ok(fallbacks)
}

fn validate_collection_list_dataflow(
    body: &Expr,
    variant: &str,
    child_matches: &[&syn::ExprMatch],
) -> Result<(), String> {
    let match_expr = child_matches[0];
    let (value, continue_arm) = continue_value_arm(match_expr)?;
    let append = call_nodes_in_expr(&continue_arm.body, "append_lowered_list_element");
    let checks = [
        append.len() == 1,
        append.first().is_some_and(|call| call.args.len() == 4),
        append.first().is_some_and(|call| is_mut_reference_to(&call.args[0], "result")),
        append.first().is_some_and(|call| path_is_ident(&call.args[1], &value)),
        append.first().is_some_and(|call| if variant == "List" {
            matches!(&call.args[2], Expr::Lit(literal) if matches!(&literal.lit, syn::Lit::Bool(value) if !value.value))
        } else {
            path_is_ident(&call.args[2], "splice")
        }),
        append.first().is_some_and(|call| path_is_ident(&call.args[3], "span")),
    ];
    if checks.iter().any(|check| !check) {
        return Err(format!("{variant} child result does not flow to the matching list append: {checks:?}"));
    }
    if method_call_nodes_in_expr(body, "eval_indexed_expr").len() != 1 {
        return Err(format!("{variant} normal arm has an unexpected child evaluator"));
    }
    Ok(())
}

fn validate_collection_map_dataflow(
    body: &Expr,
    child_matches: &[&syn::ExprMatch],
) -> Result<(), String> {
    let (key_statement, key_initializer, key_match, value_statement, value_initializer, value_match) =
        ordered_map_child_bindings(body)?;
    if key_statement >= value_statement
        || !std::ptr::eq(key_match, child_matches[0])
        || !std::ptr::eq(value_match, child_matches[1])
    {
        return Err("MapLiteral key child evaluation must precede value child evaluation".into());
    }
    let (key_value, key_continue) = continue_value_arm(child_matches[0])?;
    let key_conversion = call_nodes_in_expr(body, "lowered_map_literal_key");
    let (value_value, value_continue) = continue_value_arm(child_matches[1])?;
    let append = call_nodes_in_expr(body, "append_lowered_map_literal");
    let checks = [
        path_is_ident(&key_continue.body, &key_value),
        local_binding_contains_match(body, "key", key_match),
        contains_child_match(key_initializer, key_match),
        path_is_ident(&value_continue.body, &value_value),
        matches!(value_initializer, Expr::Match(expression) if std::ptr::eq(expression, value_match)),
        path_is_ident(&value_continue.body, "value"),
        key_conversion.len() == 1,
        key_conversion.first().is_some_and(|call| call.args.len() == 2),
        key_conversion.first().is_some_and(|call| is_reference_to(&call.args[0], "key")),
        key_conversion.first().is_some_and(|call| path_is_ident(&call.args[1], "span")),
        append.len() == 1,
        append.first().is_some_and(|call| call.args.len() == 4),
        append.first().is_some_and(|call| is_mut_reference_to(&call.args[0], "map")),
        append.first().is_some_and(|call| path_is_ident(&call.args[1], "key")),
        append.first().is_some_and(|call| path_is_ident(&call.args[2], &value_value)),
        append.first().is_some_and(|call| path_is_ident(&call.args[3], "span")),
    ];
    if checks.iter().any(|check| !check) {
        let mut call_names = CallNameFinder::default();
        call_names.visit_expr(body);
        return Err(format!("MapLiteral child results do not flow through key conversion to the map append: {checks:?}; names={:?}", call_names.names));
    }
    if method_call_nodes_in_expr(body, "eval_indexed_expr").len() != 2 {
        return Err("MapLiteral normal arm has an unexpected child evaluator".into());
    }
    Ok(())
}

fn direct_block_child_match<'ast>(
    block: &'ast syn::Block,
    expected: &str,
) -> Option<(usize, &'ast Expr, &'ast syn::ExprMatch)> {
    let mut found = None;
    for (statement_index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Local(local) = statement else { continue; };
        let Pat::Ident(pattern) = &local.pat else { continue; };
        if pattern.ident != expected { continue; }
        if found.is_some() {
            return None;
        }
        let initializer = local.init.as_ref()?.expr.as_ref();
        let mut finder = ChildEvaluationMatchFinder { matches: Vec::new() };
        finder.visit_expr(initializer);
        if finder.matches.len() != 1 {
            return None;
        }
        found = Some((statement_index, initializer, finder.matches[0]));
    }
    found
}

fn ordered_map_child_bindings<'ast>(
    body: &'ast Expr,
) -> Result<(usize, &'ast Expr, &'ast syn::ExprMatch, usize, &'ast Expr, &'ast syn::ExprMatch), String> {
    let mut finder = MapChildBindingBlockFinder { candidates: Vec::new() };
    finder.visit_expr(body);
    if finder.candidates.len() != 1 {
        return Err(format!("MapLiteral must have one ordered key/value child-binding block, found {}", finder.candidates.len()));
    }
    Ok(finder.candidates.remove(0))
}

struct MapChildBindingBlockFinder<'ast> {
    candidates: Vec<(usize, &'ast Expr, &'ast syn::ExprMatch, usize, &'ast Expr, &'ast syn::ExprMatch)>,
}

impl<'ast> Visit<'ast> for MapChildBindingBlockFinder<'ast> {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        if let (Some((key_index, key_initializer, key_match)), Some((value_index, value_initializer, value_match))) = (
            direct_block_child_match(block, "key"),
            direct_block_child_match(block, "value"),
        ) {
            self.candidates.push((
                key_index,
                key_initializer,
                key_match,
                value_index,
                value_initializer,
                value_match,
            ));
        }
        visit::visit_block(self, block);
    }
}

fn local_binding_contains_match(
    expression: &Expr,
    expected: &str,
    target: &syn::ExprMatch,
) -> bool {
    let mut finder = LocalMatchBindingFinder { expected, target, found: false };
    finder.visit_expr(expression);
    finder.found
}

fn contains_child_match(expression: &Expr, target: &syn::ExprMatch) -> bool {
    let mut finder = ChildEvaluationMatchFinder { matches: Vec::new() };
    finder.visit_expr(expression);
    finder.matches.iter().any(|found| std::ptr::eq(*found, target))
}

struct LocalMatchBindingFinder<'name, 'ast> {
    expected: &'name str,
    target: &'ast syn::ExprMatch,
    found: bool,
}

impl<'ast> Visit<'ast> for LocalMatchBindingFinder<'_, 'ast> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == self.expected)
            && local.init.as_ref().is_some_and(|initializer| {
                matches!(initializer.expr.as_ref(), Expr::Match(expression) if std::ptr::eq(expression, self.target))
            })
        {
            self.found = true;
        }
        visit::visit_local(self, local);
    }
}

fn validate_collection_record_dataflow(
    body: &Expr,
    child_matches: &[&syn::ExprMatch],
) -> Result<(), String> {
    let (value, continue_arm) = continue_value_arm(child_matches[0])?;
    let append = call_nodes_in_expr(&continue_arm.body, "append_indexed_record_entry");
    if append.len() != 1 || append[0].args.len() != 4
        || !path_is_ident(&append[0].args[0], "entry")
        || !is_mut_reference_to(&append[0].args[1], "record")
        || !path_is_ident(&append[0].args[2], &value)
        || !path_is_ident(&append[0].args[3], "call_span")
    {
        return Err("Record child result does not flow to its entry append".into());
    }
    let finish = call_nodes_in_expr(body, "finish_record_entries");
    if finish.len() != 1 || finish[0].args.len() != 1 || !path_is_ident(&finish[0].args[0], "record") {
        return Err("Record entries do not flow to the record finisher".into());
    }
    Ok(())
}

fn validate_collection_update_dataflow(
    body: &Expr,
    child_matches: &[&syn::ExprMatch],
) -> Result<(), String> {
    let (base_value, base_continue) = continue_value_arm(child_matches[0])?;
    let (update_value, update_continue) = continue_value_arm(child_matches[1])?;
    let replacements = method_call_nodes_in_expr(body, "push");
    let batch = call_nodes_in_expr(body, "lowered_record_update_batch");
    let pushed_path = replacements.iter().find_map(|call| {
        if !path_is_ident(&call.receiver, "replacements") || call.args.len() != 1 {
            return None;
        }
        let Expr::Tuple(tuple) = &call.args[0] else { return None; };
        (tuple.elems.len() == 3
            && path_is_ident(&tuple.elems[0], "path")
            && path_is_ident(&tuple.elems[1], &update_value)
            && path_is_ident(&tuple.elems[2], "field_span"))
            .then_some(())
    });
    if !path_is_ident(&base_continue.body, &base_value)
        || !path_is_ident(&update_continue.body, &update_value)
        || !local_binding_contains_child_eval(body, "base")
        || pushed_path.is_none() || batch.len() != 1 || batch[0].args.len() != 3
        || !path_is_ident(&batch[0].args[0], "base")
        || !path_is_ident(&batch[0].args[1], "replacements")
        || !path_is_ident(&batch[0].args[2], "span")
    {
        return Err("RecordUpdate base and replacement child results do not flow to the batch update".into());
    }
    Ok(())
}

fn continue_value_arm(match_expr: &syn::ExprMatch) -> Result<(String, &syn::Arm), String> {
    let arms = match_expr.arms.iter().filter_map(|arm| {
        let Pat::TupleStruct(pattern) = &arm.pat else { return None; };
        if pattern.path.segments.len() != 2
            || pattern.path.segments[0].ident != "ControlFlow"
            || pattern.path.segments[1].ident != "Continue"
            || pattern.elems.len() != 1
        {
            return None;
        }
        let Pat::Ident(value) = pattern.elems.first()? else { return None; };
        Some((value.ident.to_string(), arm))
    }).collect::<Vec<_>>();
    if arms.len() != 1 {
        return Err(format!("child evaluation match has {} Continue value arms", arms.len()));
    }
    Ok(arms[0].clone())
}

fn call_nodes_in_expr<'a>(expression: &'a Expr, name: &str) -> Vec<&'a syn::ExprCall> {
    let mut finder = CallNodeFinder { name, calls: Vec::new() };
    finder.visit_expr(expression);
    finder.calls
}

fn method_call_nodes_in_expr<'a>(expression: &'a Expr, name: &str) -> Vec<&'a syn::ExprMethodCall> {
    let mut finder = MethodCallNodeFinder { name, calls: Vec::new() };
    finder.visit_expr(expression);
    finder.calls
}

fn method_call_nodes_in_stmt<'a>(statement: &'a Stmt, name: &str) -> Vec<&'a syn::ExprMethodCall> {
    let mut finder = MethodCallNodeFinder { name, calls: Vec::new() };
    finder.visit_stmt(statement);
    finder.calls
}

fn is_mut_reference_to(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Reference(reference)
        if reference.mutability.is_some() && path_is_ident(&reference.expr, expected))
}

fn is_reference_to(expression: &Expr, expected: &str) -> bool {
    match expression {
        Expr::Reference(reference) => path_is_ident(&reference.expr, expected),
        expression => path_is_ident(expression, expected),
    }
}

fn local_binding_contains_child_eval(expression: &Expr, expected: &str) -> bool {
    let mut finder = LocalBindingFinder { expected, matches: Vec::new() };
    finder.visit_expr(expression);
    finder.matches.iter().any(|initializer| {
        initializer.as_ref().is_some_and(|initializer| {
            method_call_nodes_in_expr(initializer, "eval_indexed_expr").len() == 1
        })
    })
}

struct LocalBindingFinder<'name, 'ast> {
    expected: &'name str,
    matches: Vec<Option<&'ast Expr>>,
}

impl<'ast> Visit<'ast> for LocalBindingFinder<'_, 'ast> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == self.expected) {
            self.matches.push(local.init.as_ref().map(|initializer| initializer.expr.as_ref()));
        }
        visit::visit_local(self, local);
    }
}

struct ChildEvaluationMatchFinder<'ast> {
    matches: Vec<&'ast syn::ExprMatch>,
}

impl<'ast> Visit<'ast> for ChildEvaluationMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if method_call_nodes_in_expr(&expression.expr, "eval_indexed_expr").len() == 1 {
            self.matches.push(expression);
        }
        visit::visit_expr_match(self, expression);
    }
}

fn collection_forced_arms(
    file: &File,
) -> Result<(Vec<CollectionDispatchArm>, CollectionFallback), String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder { family: "ExprViewKind", matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!("eval_expr_view must have one ExprViewKind match, found {}", finder.matches.len()));
    }
    let dispatch = finder.matches[0];
    let expected = COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(variant, tag, _, _)| (*variant, *tag))
        .collect::<BTreeMap<_, _>>();
    let mut found = BTreeMap::new();
    let mut error_group = None;
    for (arm_index, arm) in dispatch.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded ExprViewKind dispatch arm at line {}", arm.pat.span().start().line));
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
        if is_frame_strategy_error_return(&arm.body) {
            if variants.iter().any(|variant| expected.contains_key(variant.as_str())) {
                return Err("collection constructor is intercepted by frame_strategy_error".into());
            }
            if error_group.replace(CollectionFallback {
                helper: "eval_expr_view".into(),
                outcome: "frame_strategy_error(tag, span)".into(),
                scope: "valid ExprViewKind values without dedicated frame handlers".into(),
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err("ExprViewKind frame_strategy_error group is split".into());
            }
            continue;
        }
        for variant in variants {
            let Some(tag) = expected.get(variant.as_str()) else { continue; };
            let behavior = collection_forced_arm_behavior(&variant, &arm.body)?;
            if found.insert(variant.clone(), CollectionDispatchArm {
                tag: (*tag).into(),
                variant: variant.clone(),
                behavior,
                line: arm.pat.span().start().line,
                arm_index,
            }).is_some() {
                return Err(format!("duplicate forced collection constructor view {variant}"));
            }
        }
    }
    if found.len() != expected.len() {
        return Err(format!("forced collection constructor coverage differs: found {:?}", found.keys().collect::<Vec<_>>()));
    }
    let error_group = error_group.ok_or_else(|| "ExprViewKind frame_strategy_error group is missing".to_string())?;
    Ok((COLLECTION_CONSTRUCTION_ROWS
        .iter()
        .map(|(variant, _, _, _)| found.get(*variant).expect("validated view arm").clone())
        .collect(), error_group))
}

fn collection_forced_arm_behavior(variant: &str, body: &Expr) -> Result<String, String> {
    let calls = |name| call_nodes_in_expr(body, name);
    let methods = |name| method_call_nodes_in_expr(body, name);
    let valid = match variant {
        "List" | "ListBuild" => {
            let decode = calls("indexed_list_literal_view");
            let push = methods("push_expr");
            let empty = methods("push_value");
            let preflight = methods("preflight");
            let next_operand = methods("next_operand");
            let checks = [
                decode.len() == 1,
                decode.first().is_some_and(|call| call.args.len() == 4),
                decode.first().is_some_and(|call| is_reference_to(&call.args[0], "execution")),
                decode.first().is_some_and(|call| path_is_ident(&call.args[1], "tag")),
                decode.first().is_some_and(|call| path_is_ident(&call.args[2], "payload")),
                decode.first().is_some_and(|call| path_is_ident(&call.args[3], "span")),
                preflight.len() == 1,
                next_operand.len() == 1,
                push.len() == 1,
                push.first().is_some_and(|call| call.args.len() == 4),
                push.first().is_some_and(|call| path_is_ident(&call.args[0], "index")),
                push.first().is_some_and(|call| path_is_ident(&call.args[1], "operand")),
                push.first().is_some_and(|call| path_is_ident(&call.args[2], "item_span")),
                empty.len() == 1,
                matches!(body, Expr::Block(_)),
            ];
            if checks.iter().any(|check| !check) {
                return Err(format!("forced {variant} view operands changed: {checks:?}"));
            }
            true
        }
        "MapLiteral" => {
            let decode = calls("indexed_map_literal_view");
            let step = methods("step_map_literal");
            let preflight = methods("preflight");
            decode.len() == 1 && decode[0].args.len() == 4
                && is_reference_to(&decode[0].args[0], "execution")
                && path_is_ident(&decode[0].args[1], "tag")
                && path_is_ident(&decode[0].args[2], "payload")
                && path_is_ident(&decode[0].args[3], "span")
                && step.len() == 1 && step[0].args.len() == 3
                && path_is_ident(&step[0].args[0], "index")
                && preflight.len() == 1
        }
        "RecordUpdate" => {
            let decode = calls("decode_record_updates");
            let push = methods("push_expr");
            decode.len() == 1 && decode[0].args.len() == 4
                && is_reference_to(&decode[0].args[0], "execution")
                && path_is_ident(&decode[0].args[1], "tag")
                && path_is_ident(&decode[0].args[2], "payload")
                && path_is_ident(&decode[0].args[3], "span")
                && push.len() == 1 && push[0].args.len() == 4
                && path_is_ident(&push[0].args[0], "index")
                && path_is_ident(&push[0].args[1], "base")
                && path_is_ident(&push[0].args[2], "update_span")
        }
        "Record" => {
            let decode = calls("indexed_record_literal_view");
            let step = methods("step_record_items");
            let preflight = methods("preflight");
            decode.len() == 1 && decode[0].args.len() == 4
                && is_reference_to(&decode[0].args[0], "execution")
                && path_is_ident(&decode[0].args[1], "tag")
                && path_is_ident(&decode[0].args[2], "payload")
                && path_is_ident(&decode[0].args[3], "span")
                && step.len() == 1 && step[0].args.len() == 3
                && path_is_ident(&step[0].args[0], "index")
                && preflight.len() == 1
        }
        _ => return Err(format!("unknown collection constructor view {variant}")),
    };
    if !valid {
        return Err(format!("forced {variant} view no longer selects its expected frame helper and operands"));
    }
    Ok(match variant {
        "List" => "indexed_list_literal_frame_continuation",
        "ListBuild" => "indexed_list_build_frame_continuation",
        "MapLiteral" => "map_literal_frame_step",
        "RecordUpdate" => "record_update_base_continuation",
        "Record" => "record_items_frame_step",
        _ => unreachable!("constructor variants were closed above"),
    }.into())
}

fn collection_normal_fallback(file: &File) -> Result<CollectionFallback, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (_, dispatch) = top_level_tag_dispatch_match(block)?;
    let wildcards = dispatch.arms.iter().enumerate()
        .filter(|(_, arm)| matches!(arm.pat, Pat::Wild(_)))
        .collect::<Vec<_>>();
    if wildcards.len() != 1 || wildcards[0].0 + 1 != dispatch.arms.len() {
        return Err("indexed expression dispatch must end in one wildcard fallback".into());
    }
    let (arm_index, arm) = wildcards[0];
    if arm.guard.is_some() || !is_unhandled_indexed_expr_fallback(&arm.body) {
        return Err("indexed expression wildcard fallback changed from the explicit unsupported-tag error".into());
    }
    Ok(CollectionFallback {
        helper: "eval_indexed_expr_inner".into(),
        outcome: "RuntimeError(indexed-ir: direct indexed evaluator does not support tag)".into(),
        scope: "FullTag values without a direct indexed expression arm".into(),
        line: arm.pat.span().start().line,
        arm_index,
    })
}

fn collection_forced_route(file: &File) -> Result<CollectionForcedRoute, String> {
    let block = find_function(file, "eval_expr")?;
    let (frame_apply_statement_index, frame_apply) = find_frame_apply_branch(block, true)?;
    let view_kind_statement_index = frame_view_kind_index(block)?;
    if frame_apply_statement_index >= view_kind_statement_index {
        return Err("forced collection route tests generic FrameApply after ExprViewKind selection".into());
    }
    let eval_expr_view = find_single_method_call(block, "eval_expr_view")?;
    if !path_is_ident(&eval_expr_view.receiver, "self")
        || eval_expr_view.args.len() != 8
        || !path_is_ident(&eval_expr_view.args[0], "index")
        || !path_is_ident(&eval_expr_view.args[1], "instruction")
        || !path_is_ident(&eval_expr_view.args[2], "tag")
        || !path_is_ident(&eval_expr_view.args[3], "view_kind")
        || !matches!(&eval_expr_view.args[4], Expr::Reference(reference)
            if reference.mutability.is_none() && path_is_ident(&reference.expr, "execution"))
        || !path_is_ident(&eval_expr_view.args[5], "payload")
        || !path_is_ident(&eval_expr_view.args[6], "span")
        || !path_is_ident(&eval_expr_view.args[7], "next")
        || eval_expr_view.method.span().start().line <= block.stmts[view_kind_statement_index].span().start().line
        || !matches!(block.stmts.last(), Some(Stmt::Expr(Expr::MethodCall(call), None))
            if std::ptr::eq(call, eval_expr_view))
    {
        return Err("forced collection route does not pass the selected ExprViewKind to eval_expr_view".into());
    }
    Ok(CollectionForcedRoute {
        frame_apply_line: frame_apply.if_token.span().start().line,
        frame_apply_statement_index,
        view_kind_line: block.stmts[view_kind_statement_index].span().start().line,
        view_kind_statement_index,
        eval_expr_view_line: eval_expr_view.method.span().start().line,
    })
}

fn is_unhandled_indexed_expr_fallback(body: &Expr) -> bool {
    let mut calls = CallNameFinder::default();
    calls.visit_expr(body);
    let names = calls.names.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    if names.iter().filter(|name| **name == "Err").count() != 1
        || names.iter().filter(|name| **name == "new").count() != 1
    {
        return false;
    }
    let new_calls = call_nodes_in_expr(body, "new");
    if new_calls.len() != 1 || new_calls[0].args.len() != 2
        || !matches!(&new_calls[0].args[0], Expr::Lit(literal)
            if matches!(&literal.lit, syn::Lit::Str(value) if value.value() == "indexed-ir"))
    {
        return false;
    }
    let mut methods = MethodCallNodeFinder { name: "with_span", calls: Vec::new() };
    methods.visit_expr(body);
    if methods.calls.len() != 1 || !path_is_ident(&methods.calls[0].args[0], "call_span") {
        return false;
    }
    let mut macros = MacroTokenFinder { name: "format", tokens: Vec::new() };
    macros.visit_expr(body);
    macros.tokens.len() == 1
        && macros.tokens[0].contains("direct indexed evaluator does not support")
        && macros.tokens[0].contains("tag")
}

fn collection_view_helper_fallback(
    file: &File,
    function_name: &str,
    first_variant: &str,
    extra_variant: &str,
) -> Result<CollectionFallback, String> {
    let block = find_function(file, function_name)?;
    let mut finder = EnumMatchFinder { family: "ExprView", matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!("{function_name} must have one typed ExprView match"));
    }
    let dispatch = finder.matches[0];
    let mut found = BTreeSet::new();
    let mut wildcard = None;
    for (arm_index, arm) in dispatch.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded ExprView helper arm in {function_name}"));
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            if wildcard.replace((arm_index, arm)).is_some() {
                return Err(format!("duplicate ExprView wildcard in {function_name}"));
            }
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
        for variant in variants {
            if variant != first_variant && variant != extra_variant {
                return Err(format!("unexpected {function_name} view arm {variant}"));
            }
            validate_collection_view_helper_arm(function_name, &variant, &arm.pat, &arm.body)?;
            if !found.insert(variant.clone()) {
                return Err(format!("duplicate {function_name} view arm {variant}"));
            }
        }
    }
    let mut expected = BTreeSet::from([first_variant.to_string()]);
    if !extra_variant.is_empty() {
        expected.insert(extra_variant.to_string());
    }
    if found != expected {
        return Err(format!("{function_name} view arms differ: {found:?}"));
    }
    let (arm_index, arm) = wildcard.ok_or_else(|| format!("{function_name} has no typed-view fallback"))?;
    if arm_index + 1 != dispatch.arms.len() {
        return Err(format!("{function_name} typed-view fallback is not terminal"));
    }
    let expected_error = match function_name {
        "indexed_list_literal_view" => "list literal view is invalid",
        "indexed_map_literal_view" => "map literal view is invalid",
        "indexed_record_literal_view" => "record literal view is invalid",
        _ => return Err(format!("unknown collection view helper {function_name}")),
    };
    if !is_invalid_collection_view_error(&arm.body, expected_error) {
        return Err(format!("{function_name} typed-view fallback changed"));
    }
    Ok(CollectionFallback {
        helper: function_name.into(),
        outcome: format!("Err(indexed-ir: {expected_error})"),
        scope: "a tag reaches the helper with a different typed ExprView".into(),
        line: arm.pat.span().start().line,
        arm_index,
    })
}

fn validate_collection_view_helper_arm(
    function_name: &str,
    variant: &str,
    pattern: &Pat,
    body: &Expr,
) -> Result<(), String> {
    let binding = match pattern {
        Pat::TupleStruct(pattern)
            if pattern.path.segments.len() == 2
                && pattern.path.segments[0].ident == "ExprView"
                && pattern.path.segments[1].ident == variant
                && pattern.elems.len() == 1 =>
        {
            match pattern.elems.first() {
                Some(Pat::Ident(binding)) => binding.ident.to_string(),
                _ => return Err(format!("{function_name} {variant} arm does not bind one view value")),
            }
        }
        _ => return Err(format!("{function_name} {variant} arm has an unexpected view pattern")),
    };
    let ok = call_nodes_in_expr(body, "Ok");
    if ok.len() != 1 || ok[0].args.len() != 1 {
        return Err(format!("{function_name} {variant} arm does not return its decoded view value"));
    }
    let result_is_valid = match function_name {
        "indexed_list_literal_view" => {
            let helper_variant = if variant == "ListBuild" { "Build" } else { variant };
            exact_call(&ok[0].args[0], &["IndexedListLiteralView", helper_variant])
                .is_some_and(|constructor| constructor.args.len() == 1
                    && path_is_ident(&constructor.args[0], &binding))
        }
        "indexed_map_literal_view" | "indexed_record_literal_view" => {
            path_is_ident(&ok[0].args[0], &binding)
        }
        _ => false,
    };
    if !result_is_valid {
        return Err(format!("{function_name} {variant} arm discards or changes its decoded view value"));
    }
    Ok(())
}

fn is_invalid_collection_view_error(body: &Expr, expected_message: &str) -> bool {
    let mut calls = CallNameFinder::default();
    calls.visit_expr(body);
    let names = calls.names.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    let new_calls = call_nodes_in_expr(body, "new");
    let mut methods = MethodCallNodeFinder { name: "with_span", calls: Vec::new() };
    methods.visit_expr(body);
    let strings = string_literals_in_expr(body);
    names.iter().filter(|name| **name == "Err").count() == 1
        && new_calls.len() == 1
        && new_calls[0].args.len() == 2
        && matches!(&new_calls[0].args[0], Expr::Lit(literal)
            if matches!(&literal.lit, syn::Lit::Str(value) if value.value() == "indexed-ir"))
        && strings.iter().filter(|value| value.as_str() == expected_message).count() == 1
        && methods.calls.len() == 1
}

fn collection_record_update_fallback(file: &File) -> Result<CollectionFallback, String> {
    let block = find_function(file, "decode_record_updates")?;
    let mut finder = ExprViewLetElseFinder { candidates: Vec::new() };
    finder.visit_block(block);
    if finder.candidates.len() != 1 {
        return Err(format!("decode_record_updates must have one typed RecordUpdate let-else, found {}", finder.candidates.len()));
    }
    let candidate = &finder.candidates[0];
    let mut variants = Vec::new();
    collect_named_view_variants(candidate.pattern, "ExprView", &mut variants)?;
    if variants != ["RecordUpdate"] || !is_unreachable_block(candidate.fallback, "record-update tag has a matching borrowed expression view") {
        return Err("decode_record_updates typed-view fallback changed".into());
    }
    let Expr::Block(fallback_block) = candidate.fallback else {
        return Err("decode_record_updates typed-view fallback is not a block".into());
    };
    let [Stmt::Expr(unreachable, None)] = fallback_block.block.stmts.as_slice() else {
        return Err("decode_record_updates typed-view fallback is not one expression".into());
    };
    Ok(CollectionFallback {
        helper: "decode_record_updates".into(),
        outcome: "unreachable!: record-update tag has a matching borrowed expression view".into(),
        scope: "a tag reaches record-update decoding with a different typed ExprView".into(),
        line: unreachable.span().start().line,
        arm_index: 0,
    })
}

fn is_unreachable_block(expression: &Expr, message: &str) -> bool {
    let Expr::Block(block) = expression else { return false; };
    matches!(block.block.stmts.as_slice(), [Stmt::Expr(expression, None)]
        if is_exact_unreachable(expression, message))
}

fn string_literals_in_expr(expression: &Expr) -> Vec<String> {
    let mut finder = StringLiteralFinder { values: Vec::new() };
    finder.visit_expr(expression);
    finder.values
}

struct StringLiteralFinder {
    values: Vec<String>,
}

impl<'ast> Visit<'ast> for StringLiteralFinder {
    fn visit_expr_lit(&mut self, expression: &'ast syn::ExprLit) {
        if let syn::Lit::Str(value) = &expression.lit {
            self.values.push(value.value());
        }
        visit::visit_expr_lit(self, expression);
    }
}

struct MacroTokenFinder<'name> {
    name: &'name str,
    tokens: Vec<String>,
}

impl<'ast> Visit<'ast> for MacroTokenFinder<'_> {
    fn visit_expr_macro(&mut self, expression: &'ast syn::ExprMacro) {
        if expression.mac.path.segments.last().is_some_and(|segment| segment.ident == self.name) {
            self.tokens.push(expression.mac.tokens.to_string());
        }
        visit::visit_expr_macro(self, expression);
    }
}

#[derive(Clone, Copy)]
struct ExprViewLetElse<'ast> {
    pattern: &'ast Pat,
    fallback: &'ast Expr,
}

struct ExprViewLetElseFinder<'ast> {
    candidates: Vec<ExprViewLetElse<'ast>>,
}

impl<'ast> Visit<'ast> for ExprViewLetElseFinder<'ast> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if pattern_uses_enum(&local.pat, "ExprView") {
            if let Some(fallback) = local.init.as_ref().and_then(|initializer| initializer.diverge.as_ref().map(|(_, expr)| expr.as_ref())) {
                self.candidates.push(ExprViewLetElse { pattern: &local.pat, fallback });
            }
        }
        visit::visit_local(self, local);
    }
}

fn expression_schema_rows(file: &File) -> Result<Vec<ParsedExprSchemaRow>, String> {
    let schema_items = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item) if item.mac.path.is_ident("instruction_schema") => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    if schema_items.len() != 1 {
        return Err(format!(
            "expected one instruction_schema! invocation, found {}",
            schema_items.len()
        ));
    }
    let tokens = schema_items[0].mac.tokens.clone().into_iter().collect::<Vec<_>>();
    let mut groups = Vec::new();
    for index in 0..tokens.len() {
        if !ident_is(tokens.get(index), "BuildExprRow") {
            continue;
        }
        if !arrow_at(&tokens, index + 1) || !ident_is(tokens.get(index + 3), "ExprView") {
            return Err("BuildExprRow has an unrecognized view layout".into());
        }
        let kind = tokens.get(index + 4).and_then(group_tokens);
        if !kind.is_some_and(|kind| {
            let items = kind.into_iter().collect::<Vec<_>>();
            items.len() == 1 && ident_is(items.first(), "Expr")
        }) {
            return Err("BuildExprRow is not [Expr]".into());
        }
        let Some(TokenTree::Group(block)) = tokens.get(index + 5) else {
            return Err("BuildExprRow has no row block".into());
        };
        if block.delimiter() != Delimiter::Brace {
            return Err("BuildExprRow row block is not braced".into());
        }
        groups.push(block.stream());
    }
    if groups.len() != 1 {
        return Err(format!(
            "expected one BuildExprRow => ExprView [Expr] group, found {}",
            groups.len()
        ));
    }
    let parsed = syn::parse2::<ExprSchemaRows>(groups.into_iter().next().unwrap())
        .map_err(|error| format!("parse BuildExprRow declarations: {error}"))?;
    if parsed.0.is_empty() {
        return Err("BuildExprRow has no rows".into());
    }
    let mut tags = BTreeSet::new();
    let mut variants = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for row in &parsed.0 {
        if !tags.insert(row.tag.clone()) {
            return Err(format!("duplicate Expr schema tag {}", row.tag));
        }
        if !variants.insert(row.variant.clone()) {
            return Err(format!("duplicate Expr schema variant {}", row.variant));
        }
        if !ordinals.insert(row.ordinal) {
            return Err(format!("duplicate Expr schema ordinal {}", row.ordinal));
        }
    }
    Ok(parsed.0)
}

fn frame_constructor_schema_rows(
    expression_rows: &[ParsedExprSchemaRow],
) -> Result<Vec<FrameConstructorSchemaRow>, String> {
    let expected = FRAME_CONSTRUCTOR_ROWS.iter().copied().collect::<BTreeSet<_>>();
    let actual = expression_rows
        .iter()
        .filter(|row| {
            FRAME_CONSTRUCTOR_ROWS
                .iter()
                .any(|(_, tag, _)| *tag == row.tag)
        })
        .map(|row| (row.variant.as_str(), row.tag.as_str(), row.ordinal))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "frame constructor schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    FRAME_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, tag, ordinal)| {
            let row = expression_rows
                .iter()
                .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
                .ok_or_else(|| format!("missing frame constructor schema row {tag}"))?;
            if row.effects != "0"
                || row.class != "Scheduled"
                || row.strategy != "FrameApply"
                || row.classification.is_some()
            {
                return Err(format!(
                    "frame constructor schema row {tag} must be pure Scheduled; FrameApply without classification"
                ));
            }
            Ok(FrameConstructorSchemaRow {
                variant: row.variant.clone(),
                tag: row.tag.clone(),
                ordinal: row.ordinal,
                effects: row.effects.clone(),
                class: row.class.clone(),
                strategy: row.strategy.clone(),
                line: row.line,
            })
        })
        .collect()
}

fn frame_constructor_normal_route(file: &File) -> Result<FrameConstructorRoute, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let (statement_index, branch) = find_frame_apply_branch(block, false)?;
    if branch.then_branch.stmts.len() != 1 || branch.else_branch.is_some() {
        return Err("normal FrameApply constructor route must directly return through frames".into());
    }
    let Stmt::Expr(Expr::Return(returned), _) = &branch.then_branch.stmts[0] else {
        return Err("normal FrameApply constructor route does not return through frames".into());
    };
    let Some(Expr::MethodCall(call)) = returned.expr.as_deref() else {
        return Err("normal FrameApply constructor route has no frame-evaluator call".into());
    };
    if call.method != "eval_indexed_expr_with_frames"
        || !path_is_ident(&call.receiver, "self")
        || call.args.len() != 4
        || ![
            "execution",
            "instruction",
            "slots",
            "call_span",
        ]
        .iter()
        .zip(&call.args)
        .all(|(expected, actual)| path_is_ident(actual, expected))
    {
        return Err("normal FrameApply constructor route has an unexpected frame call".into());
    }
    let fallback_statement_index = top_level_tag_match_index(block)?;
    if statement_index >= fallback_statement_index {
        return Err("normal FrameApply constructor route occurs after tag dispatch".into());
    }
    Ok(FrameConstructorRoute {
        mode: "normal".into(),
        line: branch.if_token.span().start().line,
        helper_line: call.method.span().start().line,
        statement_index,
        fallback_line: block.stmts[fallback_statement_index].span().start().line,
        fallback_statement_index,
    })
}

fn frame_constructor_forced_route(file: &File) -> Result<FrameConstructorRoute, String> {
    let block = find_function(file, "eval_expr")?;
    let (statement_index, branch) = find_frame_apply_branch(block, true)?;
    if branch.else_branch.is_some() {
        return Err("forced FrameApply constructor branch must not have an else arm".into());
    }
    let statements = &branch.then_branch.stmts;
    if statements.len() != 5 || !is_view_decode_binding(&statements[0]) {
        return Err(
            "forced FrameApply constructor branch must decode, select, span, push, then return".into(),
        );
    }
    let (child_result, child_call) = forced_frame_child_binding(&statements[1])?;
    let unique_child_call = find_single_call(&branch.then_branch, "indexed_frame_apply_child")?;
    if !std::ptr::eq(child_call, unique_child_call) {
        return Err("forced FrameApply constructor child selection is not a direct binding".into());
    }
    validate_frame_apply_child_call(child_call)?;
    if !is_apply_span_binding(&statements[2], &child_result) {
        return Err("forced FrameApply constructor span is not derived from the selected child".into());
    }
    let push_call = direct_try_method_call(&statements[3], "push_apply_operands")
        .ok_or_else(|| "forced FrameApply constructor push is not unconditional".to_string())?;
    let unique_push_call = find_single_method_call(&branch.then_branch, "push_apply_operands")?;
    if !std::ptr::eq(push_call, unique_push_call) {
        return Err("forced FrameApply constructor push is not a direct branch statement".into());
    }
    validate_push_apply_call(push_call, &child_result)?;
    if !is_success_return(&statements[4]) {
        return Err("forced FrameApply constructor branch does not return after pushing".into());
    }
    let fallback_statement_index = frame_view_kind_index(block)?;
    if statement_index >= fallback_statement_index {
        return Err("forced FrameApply constructor route occurs after ExprViewKind fallback".into());
    }
    Ok(FrameConstructorRoute {
        mode: "forced".into(),
        line: branch.if_token.span().start().line,
        helper_line: child_call.func.span().start().line,
        statement_index,
        fallback_line: block.stmts[fallback_statement_index].span().start().line,
        fallback_statement_index,
    })
}

fn find_frame_apply_branch(
    block: &Block,
    explicit: bool,
) -> Result<(usize, &syn::ExprIf), String> {
    let path = if explicit {
        &["super", "indexed_frame_apply_tag"][..]
    } else {
        &["indexed_frame_apply_tag"][..]
    };
    let matches = block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| match statement {
            Stmt::Expr(Expr::If(expression), _) if is_classifier_condition(&expression.cond, path) => {
                Some((index, expression))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "{} must have one FrameApply constructor branch, found {}",
            if explicit { "eval_expr" } else { "eval_indexed_expr_inner" },
            matches.len()
        ));
    }
    Ok(matches[0])
}

fn is_classifier_condition(expression: &Expr, path: &[&str]) -> bool {
    matches!(expression, Expr::Call(call)
        if call.args.len() == 1
            && path_is_ident(&call.args[0], "tag")
            && path_has_segments(&call.func, path))
}

fn top_level_tag_match_index(block: &Block) -> Result<usize, String> {
    let matches = block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            let Stmt::Local(local) = statement else {
                return None;
            };
            let expression = local.init.as_ref()?.expr.as_ref();
            match expression {
                Expr::Match(expression) if path_is_ident(&expression.expr, "tag") => Some(index),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!("expected one top-level match tag, found {}", matches.len()));
    }
    Ok(matches[0])
}

fn frame_view_kind_index(block: &Block) -> Result<usize, String> {
    let matches = block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            let Stmt::Local(local) = statement else {
                return None;
            };
            if !matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "view_kind") {
                return None;
            }
            let expression = local.init.as_ref()?.expr.as_ref();
            let mut finder = MethodCallNodeFinder {
                name: "expr_view_kind",
                calls: Vec::new(),
            };
            finder.visit_expr(expression);
            if finder.calls.len() == 1 && path_is_ident(&finder.calls[0].receiver, "tag") {
                Some(index)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!("expected one ExprViewKind fallback, found {}", matches.len()));
    }
    Ok(matches[0])
}

fn validate_frame_apply_child_call(call: &syn::ExprCall) -> Result<(), String> {
    if !path_has_segments(&call.func, &["super", "indexed_frame_apply_child"])
        || call.args.len() != 6
        || !path_is_ident(&call.args[0], "tag")
        || !matches!(&call.args[1], Expr::Reference(reference)
            if reference.mutability.is_some() && path_is_ident(&reference.expr, "view"))
        || !matches!(&call.args[2], Expr::Lit(literal)
            if matches!(&literal.lit, syn::Lit::Int(value) if value.base10_parse::<u8>().ok() == Some(0)))
        || !path_is_ident(&call.args[3], "None")
        || !path_is_ident(&call.args[4], "None")
        || !path_is_ident(&call.args[5], "span")
    {
        return Err("forced FrameApply constructor route calls child selection with unexpected arguments".into());
    }
    Ok(())
}

fn forced_frame_child_binding(statement: &Stmt) -> Result<(String, &syn::ExprCall), String> {
    let Stmt::Local(local) = statement else {
        return Err("forced FrameApply child selection is not a top-level binding".into());
    };
    let Pat::Ident(pattern) = &local.pat else {
        return Err("forced FrameApply child selection has an unexpected binding pattern".into());
    };
    if pattern.subpat.is_some() {
        return Err("forced FrameApply child selection binding has a subpattern".into());
    }
    let expression = local
        .init
        .as_ref()
        .map(|initializer| initializer.expr.as_ref())
        .ok_or_else(|| "forced FrameApply child selection has no initializer".to_string())?;
    let Expr::Try(expression) = expression else {
        return Err("forced FrameApply child selection is not evaluated unconditionally".into());
    };
    let Expr::Call(call) = expression.expr.as_ref() else {
        return Err("forced FrameApply child selection is not a direct call".into());
    };
    if !path_has_segments(&call.func, &["super", "indexed_frame_apply_child"]) {
        return Err("forced FrameApply child selection calls an unexpected helper".into());
    }
    Ok((pattern.ident.to_string(), call))
}

fn is_view_decode_binding(statement: &Stmt) -> bool {
    let Stmt::Local(local) = statement else {
        return false;
    };
    if !matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "view" && pattern.subpat.is_none()) {
        return false;
    }
    let Some(initializer) = local.init.as_ref() else {
        return false;
    };
    let Expr::Try(expression) = initializer.expr.as_ref() else {
        return false;
    };
    let expression = match expression.expr.as_ref() {
        Expr::MethodCall(method) if method.method == "map_err" => method.receiver.as_ref(),
        expression => expression,
    };
    matches!(expression, Expr::Call(call)
        if call.args.len() == 3
            && matches!(&*call.func, Expr::Path(path)
                if path.path.segments.len() >= 2
                    && path.path.segments[path.path.segments.len() - 2].ident == "ExprView"
                    && path.path.segments.last().is_some_and(|segment| segment.ident == "decode")))
}

fn is_apply_span_binding(statement: &Stmt, child_result: &str) -> bool {
    let Stmt::Local(local) = statement else {
        return false;
    };
    if !matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "apply_span" && pattern.subpat.is_none()) {
        return false;
    }
    let Some(initializer) = local.init.as_ref() else {
        return false;
    };
    let Expr::MethodCall(map_or) = initializer.expr.as_ref() else {
        return false;
    };
    if map_or.method != "map_or"
        || map_or.args.len() != 2
        || !path_is_ident(&map_or.args[0], "span")
        || !matches!(&*map_or.receiver, Expr::MethodCall(as_ref)
            if as_ref.method == "as_ref"
                && as_ref.args.is_empty()
                && path_is_ident(&as_ref.receiver, child_result))
    {
        return false;
    }
    matches!(&map_or.args[1], Expr::Closure(closure)
        if closure.inputs.len() == 1
            && matches!(closure.inputs.first(), Some(Pat::Ident(pattern))
                if pattern.ident == "child" && pattern.subpat.is_none())
            && matches!(closure.body.as_ref(), Expr::Field(field)
                if path_is_ident(&field.base, "child")
                    && matches!(&field.member, syn::Member::Named(name) if name == "span")))
}

fn direct_try_method_call<'a>(statement: &'a Stmt, method: &str) -> Option<&'a syn::ExprMethodCall> {
    let Stmt::Expr(Expr::Try(expression), Some(_)) = statement else {
        return None;
    };
    let Expr::MethodCall(call) = expression.expr.as_ref() else {
        return None;
    };
    (call.method == method).then_some(call)
}

fn is_success_return(statement: &Stmt) -> bool {
    let Stmt::Expr(Expr::Return(returned), _) = statement else {
        return false;
    };
    let Some(Expr::Call(call)) = returned.expr.as_deref() else {
        return false;
    };
    path_is_ident(&call.func, "Ok")
        && call.args.len() == 1
        && matches!(&call.args[0], Expr::Tuple(tuple) if tuple.elems.is_empty())
}

fn validate_push_apply_call(call: &syn::ExprMethodCall, child_result: &str) -> Result<(), String> {
    if call.method != "push_apply_operands"
        || !path_is_ident(&call.receiver, "self")
        || call.args.len() != 6
        || !path_is_ident(&call.args[0], "index")
        || !path_is_ident(&call.args[1], "instruction")
        || !path_is_ident(&call.args[2], "tag")
        || !path_is_ident(&call.args[3], child_result)
        || !path_is_ident(&call.args[4], "apply_span")
        || !path_is_ident(&call.args[5], "next")
    {
        return Err("forced FrameApply constructor push does not consume the selected child result".into());
    }
    Ok(())
}

fn frame_constructor_view_arms(
    file: &File,
    function_name: &str,
    helper: &str,
    known_variants: &BTreeSet<String>,
) -> Result<Vec<FrameConstructorArm>, String> {
    let view_match = frame_constructor_view_match(file, function_name, known_variants)?;
    let expected = FRAME_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, _, _)| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    let mut found = BTreeMap::<String, FrameConstructorArm>::new();
    for (arm_index, arm) in view_match.arms.iter().enumerate() {
        if matches!(arm.pat, Pat::Wild(_) | Pat::Ident(_)) {
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
        if let Some(unknown) = variants
            .iter()
            .find(|variant| !known_variants.contains(*variant))
        {
            return Err(format!("unknown ExprView arm {unknown} in {function_name}"));
        }
        if variants.iter().any(|variant| expected.contains(variant)) && arm.guard.is_some() {
            return Err(format!("guarded {function_name} target arm at line {}", arm.pat.span().start().line));
        }
        for variant in variants {
            if !expected.contains(&variant) {
                continue;
            }
            let behavior = frame_constructor_arm_behavior(helper, &variant, &arm.body)?;
            let record = FrameConstructorArm {
                helper: helper.into(),
                variant: variant.clone(),
                behavior,
                line: arm.pat.span().start().line,
                arm_index,
            };
            if found.insert(variant.clone(), record).is_some() {
                return Err(format!("duplicate {function_name} view arm {variant}"));
            }
        }
    }
    if found.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(format!("{function_name} view arm coverage differs from frame constructors"));
    }
    Ok(found.into_values().collect())
}

fn frame_constructor_arm_behavior(
    helper: &str,
    variant: &str,
    body: &Expr,
) -> Result<String, String> {
    if helper == "indexed_frame_apply_child" {
        if variant == "FunctionRef" || variant == "EmptyMap" {
            return if path_is_ident(body, "None") {
                Ok("no_child".into())
            } else {
                Err(format!("{helper} {variant} arm must have no child"))
            };
        }
        if variant == "PathFrom" {
            let mut calls = CallNameFinder::default();
            calls.visit_expr(body);
            if calls
                .names
                .iter()
                .filter(|(name, _)| name == "indexed_frame_apply_child_from_fields")
                .count()
                == 1
                && returned_call_matches(body, "indexed_frame_apply_child_from_fields")
            {
                return Ok("one_child".into());
            }
            return Err(format!("{helper} {variant} arm does not return child selection"));
        }
    }
    if helper == "indexed_frame_apply_value" {
        let expected = match variant {
            "FunctionRef" => "indexed_function_ref_value",
            "PathFrom" => "indexed_path_from_value",
            "EmptyMap" => "indexed_empty_map_value",
            _ => return Err(format!("unknown frame constructor view {variant}")),
        };
        let mut calls = CallNameFinder::default();
        calls.visit_expr(body);
        if calls.names.iter().filter(|(name, _)| name == expected).count() == 1
            && returned_call_matches(body, expected)
        {
            return Ok(expected.into());
        }
        return Err(format!("{helper} {variant} arm does not return {expected}"));
    }
    Err(format!("unknown frame constructor helper {helper}"))
}

fn returned_call_matches(expression: &Expr, expected: &str) -> bool {
    let returned = match expression {
        Expr::Block(block) => match block.block.stmts.last() {
            Some(Stmt::Expr(Expr::Return(returned), _)) => returned.expr.as_deref(),
            Some(Stmt::Expr(expression, None)) => Some(expression),
            _ => None,
        },
        Expr::Return(returned) => returned.expr.as_deref(),
        expression => Some(expression),
    };
    let Some(returned) = returned else {
        return false;
    };
    match returned {
        Expr::Call(call) if path_is_ident(&call.func, expected) => true,
        Expr::Call(call) if path_is_ident(&call.func, "Ok") && call.args.len() == 1 => {
            returned_call_matches(&call.args[0], expected)
        }
        _ => false,
    }
}

fn frame_constructor_helper_fallback(
    file: &File,
    function_name: &str,
    known_variants: &BTreeSet<String>,
) -> Result<FrameConstructorFallback, String> {
    let view_match = frame_constructor_view_match(file, function_name, known_variants)?;
    let wildcards = view_match
        .arms
        .iter()
        .enumerate()
        .filter(|(_, arm)| matches!(arm.pat, Pat::Wild(_)))
        .collect::<Vec<_>>();
    if wildcards.len() != 1 || wildcards[0].0 + 1 != view_match.arms.len() {
        return Err(format!("{function_name} must end in one typed-view wildcard"));
    }
    let (arm_index, arm) = wildcards[0];
    let expected_message = "frame apply tag has a matching typed view";
    if !is_exact_unreachable(&arm.body, expected_message) {
        return Err(format!("{function_name} wildcard outcome changed"));
    }
    Ok(FrameConstructorFallback {
        helper: function_name.into(),
        outcome: format!("unreachable!: {expected_message}"),
        scope: "non-matching ExprView reaches a frame-apply helper".into(),
        line: arm.pat.span().start().line,
        arm_index,
    })
}

fn frame_constructor_value_arms(
    file: &File,
    known_variants: &BTreeSet<String>,
) -> Result<(Vec<FrameConstructorArm>, FrameConstructorFallback, usize), String> {
    let block = find_function(file, "indexed_frame_apply_value")?;
    let decode_lines = find_call_lines(block, "decode")
        .into_iter()
        .filter(|line| *line > 0)
        .collect::<Vec<_>>();
    let value_decode_line = decode_lines
        .first()
        .copied()
        .ok_or_else(|| "indexed_frame_apply_value has no ExprView decode".to_string())?;
    let arms = frame_constructor_view_arms(
        file,
        "indexed_frame_apply_value",
        "indexed_frame_apply_value",
        known_variants,
    )?;
    let fallback = frame_constructor_helper_fallback(
        file,
        "indexed_frame_apply_value",
        known_variants,
    )?;
    Ok((arms, fallback, value_decode_line))
}

fn frame_constructor_view_match<'ast>(
    file: &'ast File,
    function_name: &str,
    known_variants: &BTreeSet<String>,
) -> Result<&'ast syn::ExprMatch, String> {
    let block = find_function(file, function_name)?;
    let mut finder = ViewMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    let expected = FRAME_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, _, _)| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    let candidates = finder
        .matches
        .into_iter()
        .filter(|view_match| {
            view_match.arms.iter().any(|arm| {
                let mut variants = Vec::new();
                collect_named_view_variants(&arm.pat, "ExprView", &mut variants).is_ok()
                    && variants.iter().any(|variant| expected.contains(variant))
            })
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(format!(
            "{function_name} must have one ExprView match for the frame constructors, found {}",
            candidates.len()
        ));
    }
    let view_match = candidates[0];
    for arm in &view_match.arms {
        if matches!(arm.pat, Pat::Wild(_) | Pat::Ident(_)) {
            if arm.guard.is_some() {
                return Err(format!(
                    "guarded catch-all pattern in {function_name} can intercept frame constructors"
                ));
            }
            if matches!(&arm.pat, Pat::Ident(pattern) if pattern.subpat.is_none()) {
                return Err(format!(
                    "binding catch-all pattern in {function_name} can intercept frame constructors"
                ));
            }
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
        if let Some(unknown) = variants
            .iter()
            .find(|variant| !known_variants.contains(*variant))
        {
            return Err(format!("unknown ExprView arm {unknown} in {function_name}"));
        }
    }
    Ok(view_match)
}

fn frame_constructor_explicit_error_group(
    file: &File,
) -> Result<FrameConstructorFallback, String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder {
        family: "ExprViewKind",
        matches: Vec::new(),
    };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!(
            "eval_expr_view must have one ExprViewKind match, found {}",
            finder.matches.len()
        ));
    }
    let expected = FRAME_CONSTRUCTOR_ROWS
        .iter()
        .map(|(variant, _, _)| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    let mut found = BTreeSet::new();
    let mut matching = None;
    for (arm_index, arm) in finder.matches[0].arms.iter().enumerate() {
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
        let target_variants = variants
            .into_iter()
            .filter(|variant| expected.contains(variant))
            .collect::<Vec<_>>();
        if target_variants.is_empty() {
            continue;
        }
        if arm.guard.is_some() {
            return Err("guarded FrameApply constructor defensive error arm".into());
        }
        if matching.is_some() {
            return Err("FrameApply constructor defensive error group was split".into());
        }
        for variant in target_variants {
            if !found.insert(variant.clone()) {
                return Err(format!("duplicate ExprViewKind fallback arm {variant}"));
            }
        }
        if !is_frame_strategy_error_return(&arm.body) {
            return Err("FrameApply constructor fallback no longer returns frame_strategy_error".into());
        }
        matching = Some((arm_index, arm));
    }
    if found != expected {
        return Err(format!("ExprViewKind defensive group differs: {found:?}"));
    }
    let (arm_index, arm) = matching.ok_or_else(|| "missing defensive FrameApply error group".to_string())?;
    Ok(FrameConstructorFallback {
        helper: "eval_expr_view".into(),
        outcome: "frame_strategy_error(tag, span)".into(),
        scope: "FrameApply constructor reaches generic ExprView dispatch".into(),
        line: arm.pat.span().start().line,
        arm_index,
    })
}

fn is_frame_strategy_error_return(body: &Expr) -> bool {
    let Expr::Block(block) = body else {
        return false;
    };
    if block.block.stmts.len() != 1 {
        return false;
    }
    let Stmt::Expr(Expr::Return(returned), _) = &block.block.stmts[0] else {
        return false;
    };
    let Some(Expr::Call(error_call)) = returned.expr.as_deref() else {
        return false;
    };
    if !path_is_ident(&error_call.func, "Err") || error_call.args.len() != 1 {
        return false;
    }
    let Some(call) = exact_call(&error_call.args[0], &["frame_strategy_error"]) else {
        return false;
    };
    call.args.len() == 2 && path_is_ident(&call.args[0], "tag") && path_is_ident(&call.args[1], "span")
}

fn find_single_call<'a>(block: &'a Block, name: &str) -> Result<&'a syn::ExprCall, String> {
    let calls = find_call_nodes(block, name);
    if calls.len() != 1 {
        return Err(format!("expected one {name} call, found {}", calls.len()));
    }
    Ok(calls[0])
}

fn find_single_method_call<'a>(block: &'a Block, name: &str) -> Result<&'a syn::ExprMethodCall, String> {
    let mut finder = MethodCallNodeFinder {
        name,
        calls: Vec::new(),
    };
    finder.visit_block(block);
    if finder.calls.len() != 1 {
        return Err(format!("expected one {name} method call, found {}", finder.calls.len()));
    }
    Ok(finder.calls[0])
}

fn find_call_nodes<'a>(block: &'a Block, name: &str) -> Vec<&'a syn::ExprCall> {
    let mut finder = CallNodeFinder { name, calls: Vec::new() };
    finder.visit_block(block);
    finder.calls
}

fn find_call_lines(block: &Block, name: &str) -> Vec<usize> {
    find_call_nodes(block, name)
        .iter()
        .map(|call| call.func.span().start().line)
        .collect()
}

#[derive(Clone, Debug)]
struct ParsedExprSchemaRow {
    variant: String,
    tag: String,
    ordinal: u16,
    effects: String,
    class: String,
    strategy: String,
    classification: Option<String>,
    payload: Option<TokenStream>,
    line: usize,
}

struct ExprSchemaRows(Vec<ParsedExprSchemaRow>);

impl Parse for ExprSchemaRows {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut rows = Vec::new();
        while !input.is_empty() {
            let _attributes = input.call(syn::Attribute::parse_outer)?;
            if input.is_empty() {
                break;
            }
            let variant: syn::Ident = input.parse()?;
            let line = variant.span().start().line;
            let payload = if input.peek(syn::token::Brace) || input.peek(syn::token::Paren) {
                let payload: TokenTree = input.parse()?;
                match payload {
                    TokenTree::Group(group)
                        if matches!(group.delimiter(), Delimiter::Brace | Delimiter::Parenthesis) =>
                    {
                        Some(group.stream())
                    }
                    _ => {
                        return Err(input.error(
                            "expression schema payload must be a tuple or struct group",
                        ));
                    }
                }
            } else {
                None
            };
            let _equal: Token![=] = input.parse()?;
            let _greater: Token![>] = input.parse()?;
            let tag: syn::Ident = input.parse()?;
            let _ordinal_equal: Token![=] = input.parse()?;
            let ordinal: syn::LitInt = input.parse()?;
            let metadata: TokenTree = input.parse()?;
            let TokenTree::Group(metadata) = metadata else {
                return Err(input.error("expression schema route metadata must be grouped"));
            };
            if metadata.delimiter() != Delimiter::Bracket {
                return Err(input.error("expression schema route metadata must be bracketed"));
            }
            let (effects, class, strategy, classification) =
                parse_expr_schema_metadata(metadata.stream())
                    .map_err(|message| input.error(message))?;
            let _semicolon: Token![;] = input.parse()?;
            rows.push(ParsedExprSchemaRow {
                variant: variant.to_string(),
                tag: tag.to_string(),
                ordinal: ordinal
                    .base10_parse()
                    .map_err(|error| syn::Error::new(ordinal.span(), error))?,
                effects,
                class,
                strategy,
                classification,
                payload,
                line,
            });
        }
        Ok(Self(rows))
    }
}

fn parse_expr_schema_metadata(
    stream: TokenStream,
) -> Result<(String, String, String, Option<String>), String> {
    let tokens = stream.into_iter().collect::<Vec<_>>();
    let mut sections = vec![Vec::new()];
    for token in tokens {
        if punct_is(&token, ';') {
            sections.push(Vec::new());
        } else {
            sections.last_mut().expect("initial metadata section").push(token);
        }
    }
    if sections.len() != 3 && sections.len() != 4 {
        return Err("expression schema row has unrecognized classification metadata".into());
    }
    let effects = TokenStream::from_iter(sections[0].clone())
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let class = ident_value(sections[1].first())
        .ok_or_else(|| "expression schema row has no in-place class".to_string())?;
    if sections[1].len() != 1 {
        return Err("expression schema row has malformed in-place class".into());
    }
    let strategy = ident_value(sections[2].first())
        .ok_or_else(|| "expression schema row has no frame strategy".to_string())?;
    if sections[2].len() != 1 {
        return Err("expression schema row has malformed frame strategy".into());
    }
    let classification = if sections.len() == 4 {
        let value = ident_value(sections[3].first())
            .ok_or_else(|| "expression schema row has no classification name".to_string())?;
        if sections[3].len() != 1 {
            return Err("expression schema row has malformed classification".into());
        }
        if value != "EagerAtom" {
            return Err(format!("unknown expression schema classification {value}"));
        }
        Some(value)
    } else {
        None
    };
    Ok((effects, class, strategy, classification))
}

fn eager_atom_schema_and_macro(
    file: &File,
) -> Result<(Vec<EagerAtomSchemaRow>, usize, usize, usize), String> {
    let schema_items = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item) if item.mac.path.is_ident("instruction_schema") => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    if schema_items.len() != 1 {
        return Err(format!(
            "expected one instruction_schema! invocation, found {}",
            schema_items.len()
        ));
    }
    let schema_tokens = schema_items[0].mac.tokens.clone().into_iter().collect::<Vec<_>>();
    let mut groups = Vec::new();
    for index in 0..schema_tokens.len() {
        if !ident_is(schema_tokens.get(index), "BuildExprRow") {
            continue;
        }
        if !arrow_at(&schema_tokens, index + 1)
            || !ident_is(schema_tokens.get(index + 3), "ExprView")
        {
            return Err("BuildExprRow has an unrecognized view layout".into());
        }
        let kind = schema_tokens.get(index + 4).and_then(group_tokens);
        if !kind.is_some_and(|kind| {
            let items = kind.into_iter().collect::<Vec<_>>();
            items.len() == 1 && ident_is(items.first(), "Expr")
        }) {
            return Err("BuildExprRow is not [Expr]".into());
        }
        let Some(TokenTree::Group(block)) = schema_tokens.get(index + 5) else {
            return Err("BuildExprRow has no row block".into());
        };
        if block.delimiter() != Delimiter::Brace {
            return Err("BuildExprRow row block is not braced".into());
        }
        groups.push(block.stream());
    }
    if groups.len() != 1 {
        return Err(format!(
            "expected one BuildExprRow => ExprView [Expr] group, found {}",
            groups.len()
        ));
    }
    let parsed = syn::parse2::<ExprSchemaRows>(groups.into_iter().next().unwrap())
        .map_err(|error| format!("parse BuildExprRow declarations: {error}"))?;
    if parsed.0.is_empty() {
        return Err("BuildExprRow has no rows".into());
    }

    let mut seen_tags = BTreeSet::new();
    let mut seen_variants = BTreeSet::new();
    let mut seen_ordinals = BTreeSet::new();
    for row in &parsed.0 {
        if !seen_tags.insert(row.tag.clone()) {
            return Err(format!("duplicate Expr schema tag {}", row.tag));
        }
        if !seen_variants.insert(row.variant.clone()) {
            return Err(format!("duplicate Expr schema variant {}", row.variant));
        }
        if !seen_ordinals.insert(row.ordinal) {
            return Err(format!("duplicate Expr schema ordinal {}", row.ordinal));
        }
    }
    let classification_count = count_eager_schema_flags(&schema_items[0].mac.tokens);
    if classification_count != EAGER_ATOM_ROWS.len() {
        return Err(format!(
            "instruction_schema! has {} EagerAtom row flags, expected {}",
            classification_count,
            EAGER_ATOM_ROWS.len()
        ));
    }
    let expected = EAGER_ATOM_ROWS.iter().copied().collect::<BTreeSet<_>>();
    let actual = parsed
        .0
        .iter()
        .filter(|row| row.classification.as_deref() == Some("EagerAtom"))
        .map(|row| (row.variant.as_str(), row.tag.as_str(), row.ordinal))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "EagerAtom schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    let mut rows = Vec::new();
    for (variant, tag, ordinal) in EAGER_ATOM_ROWS {
        let row = parsed
            .0
            .iter()
            .find(|row| row.variant == *variant && row.tag == *tag && row.ordinal == *ordinal)
            .ok_or_else(|| format!("missing EagerAtom schema row {tag}"))?;
        if row.effects != "0" || row.class != "Operands" || row.strategy != "FrameApply" {
            return Err(format!(
                "EagerAtom schema row {tag} must be pure Operands; FrameApply"
            ));
        }
        let fields = parse_eager_schema_fields(row)?;
        let expected_fields = expected_eager_schema_fields(variant)
            .ok_or_else(|| format!("unknown EagerAtom schema variant {variant}"))?;
        let actual_fields = fields
            .iter()
            .map(|field| (field.name.as_str(), field.ty.as_str()))
            .collect::<Vec<_>>();
        if actual_fields != expected_fields {
            return Err(format!(
                "EagerAtom schema fields for {tag} differ: expected {expected_fields:?}, found {actual_fields:?}"
            ));
        }
        rows.push(EagerAtomSchemaRow {
            variant: row.variant.clone(),
            tag: row.tag.clone(),
            ordinal: row.ordinal,
            effects: row.effects.clone(),
            class: row.class.clone(),
            strategy: row.strategy.clone(),
            classification: row.classification.clone(),
            fields,
            line: row.line,
        });
    }

    let definitions = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item)
                if item.ident.as_ref().is_some_and(|ident| ident == "instruction_schema")
                    && item.mac.path.is_ident("macro_rules") =>
            {
                Some(item)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if definitions.len() != 1 {
        return Err(format!(
            "expected one macro_rules! instruction_schema definition, found {}",
            definitions.len()
        ));
    }
    let macro_rules = parse_macro_rules(definitions[0])?;
    if macro_rules.len() != 3 {
        return Err(format!(
            "instruction_schema! macro rules changed: expected 3, found {}",
            macro_rules.len()
        ));
    }
    let mut default_false = None;
    let mut eager_true = None;
    let mut main_rule = None;
    for rule in &macro_rules {
        match rule.pattern.as_str() {
            "@eager_atom" => {
                if rule.body != "false" || default_false.replace(rule.line).is_some() {
                    return Err("@eager_atom default rule must map directly to false".into());
                }
            }
            "@eager_atomEagerAtom" => {
                if rule.body != "true" || eager_true.replace(rule.line).is_some() {
                    return Err("@eager_atom EagerAtom rule must map directly to true".into());
                }
            }
            pattern if pattern.contains("@eager_atom") => {
                return Err(format!("unexpected eager-atom macro rule {pattern}"));
            }
            _ => {
                if main_rule.replace(rule).is_some() {
                    return Err("instruction_schema! has multiple row-expansion rules".into());
                }
            }
        }
    }
    let default_false_line = default_false.ok_or_else(|| "missing @eager_atom false rule".to_string())?;
    let eager_true_line = eager_true.ok_or_else(|| "missing @eager_atom EagerAtom true rule".to_string())?;
    let main_rule = main_rule.ok_or_else(|| "missing instruction schema row expansion rule".to_string())?;
    if !main_rule.pattern.contains("$(;$classification:ident)?") {
        return Err("instruction_schema! row matcher lost optional classification".into());
    }
    if !main_rule.body.contains("instruction_schema!(@eager_atom$($classification)?)")
        || !main_rule.body.contains("Self::$tag=>instruction_schema!")
        || !main_rule.body.contains("fnis_eager_atom(self)->bool{matchself")
    {
        return Err("instruction_schema! eager-atom expansion changed".into());
    }
    let expansion_lines = eager_atom_macro_invocation_lines(&definitions[0].mac.tokens);
    if expansion_lines.len() != 1 {
        return Err(format!(
            "expected one generated eager classification call, found {}",
            expansion_lines.len()
        ));
    }
    Ok((rows, default_false_line, eager_true_line, expansion_lines[0]))
}

fn count_eager_schema_flags(tokens: &TokenStream) -> usize {
    let tokens = tokens.clone().into_iter().collect::<Vec<_>>();
    let local = tokens
        .windows(2)
        .filter(|window| punct_is(&window[0], ';') && ident_is(Some(&window[1]), "EagerAtom"))
        .count();
    local
        + tokens
            .iter()
            .filter_map(|token| match token {
                TokenTree::Group(group) => Some(count_eager_schema_flags(&group.stream())),
                _ => None,
            })
            .sum::<usize>()
}

struct ParsedMacroRule {
    pattern: String,
    body: String,
    line: usize,
}

fn parse_macro_rules(item: &syn::ItemMacro) -> Result<Vec<ParsedMacroRule>, String> {
    let tokens = item.mac.tokens.clone().into_iter().collect::<Vec<_>>();
    let mut rules = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let Some(TokenTree::Group(pattern)) = tokens.get(index) else {
            return Err("instruction_schema! macro pattern is not grouped".into());
        };
        if pattern.delimiter() != Delimiter::Parenthesis || !arrow_at(&tokens, index + 1) {
            return Err("instruction_schema! macro rule has an unsupported layout".into());
        }
        let Some(TokenTree::Group(body)) = tokens.get(index + 3) else {
            return Err("instruction_schema! macro rule body is not grouped".into());
        };
        if body.delimiter() != Delimiter::Brace {
            return Err("instruction_schema! macro rule body is not braced".into());
        }
        rules.push(ParsedMacroRule {
            pattern: compact_tokens(&pattern.stream()),
            body: compact_tokens(&body.stream()),
            line: pattern.span().start().line,
        });
        index += 4;
        if punct_is_at(&tokens, index, ';') {
            index += 1;
        } else if index != tokens.len() {
            return Err("instruction_schema! macro has tokens outside a rule".into());
        }
    }
    Ok(rules)
}

fn compact_tokens(tokens: &TokenStream) -> String {
    tokens
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn eager_atom_macro_invocation_lines(tokens: &TokenStream) -> Vec<usize> {
    let tokens = tokens.clone().into_iter().collect::<Vec<_>>();
    let mut lines = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token, TokenTree::Ident(ident) if ident == "instruction_schema")
            && punct_is_at(&tokens, index + 1, '!')
        {
            if let Some(TokenTree::Group(arguments)) = tokens.get(index + 2) {
                if arguments.delimiter() == Delimiter::Parenthesis
                    && compact_tokens(&arguments.stream()) == "@eager_atom$($classification)?"
                {
                    lines.push(token.span().start().line);
                }
            }
        }
        if let TokenTree::Group(group) = token {
            lines.extend(eager_atom_macro_invocation_lines(&group.stream()));
        }
    }
    lines
}

fn normal_eager_atom_route(file: &File) -> Result<EagerAtomRoute, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let mut eager = None;
    let mut generic = None;
    for (index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Expr(Expr::If(expression), _) = statement else {
            continue;
        };
        if is_eager_atom_condition(&expression.cond) {
            if eager.replace((index, expression)).is_some() {
                return Err("eval_indexed_expr_inner has duplicate eager atom branches".into());
            }
        }
        if is_indexed_frame_apply_condition(&expression.cond)
            && generic
                .replace((index, expression.if_token.span.start().line))
                .is_some()
        {
            return Err("eval_indexed_expr_inner has duplicate generic FrameApply branches".into());
        }
    }
    let (statement_index, branch) = eager.ok_or_else(|| {
        "eval_indexed_expr_inner has no eager atom route".to_string()
    })?;
    let (generic_statement_index, generic_line) = generic.ok_or_else(|| {
        "eval_indexed_expr_inner has no generic FrameApply route".to_string()
    })?;
    if statement_index >= generic_statement_index {
        return Err("normal eager atom route occurs after generic FrameApply".into());
    }
    let helper_line = normal_eager_atom_return(branch)?;
    Ok(EagerAtomRoute {
        mode: "normal".into(),
        line: branch.if_token.span.start().line,
        helper_line,
        statement_index,
        generic_line,
        generic_statement_index,
    })
}

fn normal_eager_atom_return(expression: &syn::ExprIf) -> Result<usize, String> {
    if expression.else_branch.is_some() || expression.then_branch.stmts.len() != 1 {
        return Err("normal eager atom branch must directly return the eager value".into());
    }
    let returned = match &expression.then_branch.stmts[0] {
        Stmt::Expr(Expr::Return(returned), _) => returned
            .expr
            .as_deref()
            .ok_or_else(|| "normal eager atom branch returns no value".to_string())?,
        _ => return Err("normal eager atom branch does not return directly".into()),
    };
    let ok = exact_call(returned, &["Ok"])
        .ok_or_else(|| "normal eager atom branch does not return Ok".to_string())?;
    if ok.args.len() != 1 {
        return Err("normal eager atom branch has an unexpected Ok result".into());
    }
    let continue_call = exact_call(&ok.args[0], &["ControlFlow", "Continue"])
        .ok_or_else(|| "normal eager atom branch does not continue with its value".to_string())?;
    if continue_call.args.len() != 1 {
        return Err("normal eager atom branch has an unexpected continue value".into());
    }
    let Expr::Try(try_expression) = &continue_call.args[0] else {
        return Err("normal eager atom helper error is not propagated".into());
    };
    let helper = exact_call(&try_expression.expr, &["indexed_eager_atom_value"])
        .ok_or_else(|| "normal eager atom route does not call indexed_eager_atom_value".to_string())?;
    validate_eager_helper_arguments(helper, false)?;
    Ok(helper.func.span().start().line)
}

fn forced_eager_atom_route(file: &File) -> Result<EagerAtomRoute, String> {
    let block = find_function(file, "eval_expr")?;
    let mut eager = None;
    let mut generic = None;
    for (index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Expr(Expr::If(expression), _) = statement else {
            continue;
        };
        if is_eager_atom_condition(&expression.cond) {
            if eager.replace((index, expression)).is_some() {
                return Err("eval_expr has duplicate eager atom branches".into());
            }
        }
        if is_indexed_frame_apply_condition(&expression.cond)
            && generic
                .replace((index, expression.if_token.span.start().line))
                .is_some()
        {
            return Err("eval_expr has duplicate generic FrameApply branches".into());
        }
    }
    let (statement_index, branch) = eager.ok_or_else(|| "eval_expr has no eager atom route".to_string())?;
    let (generic_statement_index, generic_line) =
        generic.ok_or_else(|| "eval_expr has no generic FrameApply route".to_string())?;
    if statement_index >= generic_statement_index {
        return Err("forced eager atom route occurs after generic FrameApply".into());
    }
    let helper_line = forced_eager_atom_body(branch)?;
    Ok(EagerAtomRoute {
        mode: "forced".into(),
        line: branch.if_token.span.start().line,
        helper_line,
        statement_index,
        generic_line,
        generic_statement_index,
    })
}

fn forced_eager_atom_body(expression: &syn::ExprIf) -> Result<usize, String> {
    if expression.else_branch.is_some() || expression.then_branch.stmts.len() != 4 {
        return Err("forced eager atom fast path has an unrecognized body".into());
    }
    if !matches!(
        &expression.then_branch.stmts[0],
        Stmt::Local(local) if matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "call")
    ) {
        return Err("forced eager atom fast path has no call-local binding".into());
    }
    let Stmt::Local(value_local) = &expression.then_branch.stmts[1] else {
        return Err("forced eager atom fast path has no eager value binding".into());
    };
    if !matches!(&value_local.pat, Pat::Ident(pattern) if pattern.ident == "value") {
        return Err("forced eager atom fast path binds an unexpected value".into());
    }
    let helper_initializer = value_local
        .init
        .as_ref()
        .ok_or_else(|| "forced eager atom value binding has no initializer".to_string())?;
    let Expr::Try(try_expression) = helper_initializer.expr.as_ref() else {
        return Err("forced eager atom helper error is not propagated".into());
    };
    let helper = exact_call(&try_expression.expr, &["super", "indexed_eager_atom_value"])
        .ok_or_else(|| "forced eager atom route does not call indexed_eager_atom_value".to_string())?;
    validate_eager_helper_arguments(helper, true)?;

    let Stmt::Expr(Expr::MethodCall(push), _) = &expression.then_branch.stmts[2] else {
        return Err("forced eager atom route does not push its result".into());
    };
    if push.method != "push_value"
        || !path_is_ident(&push.receiver, "self")
        || push.args.len() != 3
        || !path_is_ident(&push.args[0], "index")
        || !matches!(
            &push.args[1],
            Expr::Call(call) if path_has_segments(&call.func, &["FrameValue", "Value"])
                && call.args.len() == 1
                && path_is_ident(&call.args[0], "value")
        )
        || !path_is_ident(&push.args[2], "next")
    {
        return Err("forced eager atom route has an unrecognized result push".into());
    }
    let Stmt::Expr(Expr::Return(returned), _) = &expression.then_branch.stmts[3] else {
        return Err("forced eager atom route does not return after pushing".into());
    };
    let Some(returned) = returned.expr.as_deref() else {
        return Err("forced eager atom route returns no completion".into());
    };
    let Some(ok) = exact_call(returned, &["Ok"]) else {
        return Err("forced eager atom route does not return Ok".into());
    };
    if ok.args.len() != 1 || !matches!(&ok.args[0], Expr::Tuple(tuple) if tuple.elems.is_empty()) {
        return Err("forced eager atom route returns an unexpected completion".into());
    }
    Ok(helper.func.span().start().line)
}

fn validate_eager_helper_arguments(call: &syn::ExprCall, explicit: bool) -> Result<(), String> {
    if call.args.len() != 5 {
        return Err("indexed_eager_atom_value call has an unexpected argument count".into());
    }
    let valid = if explicit {
        matches!(&call.args[0], Expr::Reference(reference) if !reference.mutability.is_some() && is_field_access(&reference.expr, "call", "execution"))
            && path_is_ident(&call.args[1], "tag")
            && path_is_ident(&call.args[2], "payload")
            && matches!(&call.args[3], Expr::Reference(reference) if reference.mutability.is_some() && is_field_access(&reference.expr, "call", "slots"))
            && path_is_ident(&call.args[4], "span")
    } else {
        path_is_ident(&call.args[0], "execution")
            && path_is_ident(&call.args[1], "tag")
            && path_is_ident(&call.args[2], "payload")
            && path_is_ident(&call.args[3], "slots")
            && path_is_ident(&call.args[4], "call_span")
    };
    if !valid {
        return Err("indexed_eager_atom_value call has an unexpected argument mapping".into());
    }
    Ok(())
}

fn exact_call<'a>(expression: &'a Expr, path: &[&str]) -> Option<&'a syn::ExprCall> {
    match expression {
        Expr::Call(call) if path_has_segments(&call.func, path) => Some(call),
        _ => None,
    }
}

fn eager_atom_helper_inventory(
    file: &File,
) -> Result<(
    Vec<EagerAtomViewArm>,
    EagerAtomFallback,
    Vec<EagerAtomHelperBoundary>,
    EagerAtomValueContract,
), String> {
    let block = find_function(file, "indexed_eager_atom_value")?;
    if block.stmts.len() != 3 {
        return Err("eager atom helper must decode, match, and return one value".into());
    }
    let decode_line = eager_decode_error_boundary(block)?;
    let Stmt::Local(value_local) = &block.stmts[1] else {
        return Err("eager atom helper does not bind its view match result".into());
    };
    if !matches!(&value_local.pat, Pat::Ident(pattern) if pattern.ident == "value")
        || !matches!(
            value_local.init.as_ref().map(|init| init.expr.as_ref()),
            Some(Expr::Match(expression)) if path_is_ident(&expression.expr, "view")
        )
    {
        return Err("eager atom helper does not match the decoded view into value".into());
    }
    let Stmt::Expr(returned, _) = &block.stmts[2] else {
        return Err("eager atom helper does not return the matched value".into());
    };
    let Some(returned) = exact_call(returned, &["Ok"]) else {
        return Err("eager atom helper does not return Ok(value)".into());
    };
    if returned.args.len() != 1 || !path_is_ident(&returned.args[0], "value") {
        return Err("eager atom helper returns a different value than its view arm".into());
    }

    let mut finder = ViewMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!(
            "indexed_eager_atom_value must match view once, found {}",
            finder.matches.len()
        ));
    }
    let view_match = finder.matches[0];
    if !path_is_ident(&view_match.expr, "view") {
        return Err("eager atom helper matches a value other than the decoded view".into());
    }
    let mut arms = Vec::new();
    let mut fallback = None;
    let mut seen = BTreeSet::new();
    let mut path_new_line = None;
    let mut param_freeze_line = None;
    let mut param_slot_lines = Vec::new();
    for (arm_index, arm) in view_match.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!(
                "guarded eager ExprView arm at line {}",
                arm.pat.span().start().line
            ));
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            if arm_index + 1 != view_match.arms.len()
                || !is_exact_unreachable(
                    &arm.body,
                    "only eager atom tags reach this value helper",
                )
                || fallback.is_some()
            {
                return Err("eager helper wildcard is not the exact terminal unreachable arm".into());
            }
            fallback = Some(EagerAtomFallback {
                scope: "non-eager ExprView reaches the eager atom helper".into(),
                outcome: "unreachable!: only eager atom tags reach this value helper".into(),
                source: "normal_dispatch".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, "ExprView", &mut variants)?;
        if variants.len() != 1 {
            return Err(format!(
                "eager ExprView arm must name exactly one variant at line {}",
                arm.pat.span().start().line
            ));
        }
        let variant = variants.into_iter().next().expect("one pattern checked");
        if !EAGER_VIEW_VARIANTS.contains(&variant.as_str()) {
            return Err(format!("unknown eager ExprView arm {variant}"));
        }
        if !seen.insert(variant.clone()) {
            return Err(format!("duplicate eager ExprView arm {variant}"));
        }
        let tag = EAGER_ATOM_ROWS
            .iter()
            .find(|(expected_variant, _, _)| *expected_variant == variant)
            .map(|(_, tag, _)| *tag)
            .ok_or_else(|| format!("eager ExprView arm {variant} has no schema row"))?;
        let (value_arm, path_line, freeze_line, slot_lines) =
            eager_atom_value_arm(&variant, tag, &arm.pat, &arm.body, arm_index)?;
        if let Some(line) = path_line {
            path_new_line = Some(line);
        }
        if let Some(line) = freeze_line {
            param_freeze_line = Some(line);
        }
        param_slot_lines.extend(slot_lines);
        arms.push(value_arm);
    }
    let expected = EAGER_VIEW_VARIANTS
        .iter()
        .map(|variant| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    if seen != expected {
        return Err(format!(
            "eager ExprView arm coverage differs: expected {expected:?}, found {seen:?}"
        ));
    }
    let helper_fallback = fallback.ok_or_else(|| "eager helper has no wildcard fallback".to_string())?;
    let path_new_line = path_new_line.ok_or_else(|| "ExprView::Path has no PathValue conversion".to_string())?;
    let param_freeze_line = param_freeze_line.ok_or_else(|| "ExprView::Param has no slot freeze".to_string())?;
    if param_slot_lines.len() != 2 {
        return Err("ExprView::Param must read its selected slot for freeze and clone".into());
    }
    let helper_boundaries = vec![
        EagerAtomHelperBoundary {
            kind: "ExprView decode".into(),
            tags: EAGER_ATOM_ROWS.iter().map(|(_, tag, _)| (*tag).into()).collect(),
            outcome: "ExprView decode errors become indexed-ir RuntimeError values at the expression span".into(),
            references: vec![EagerAtomSourceRef {
                source: "normal_dispatch".into(),
                line: decode_line,
            }],
        },
        EagerAtomHelperBoundary {
            kind: "PathValue conversion".into(),
            tags: vec!["ExprPath".into()],
            outcome: "PathValue validation errors become indexed-ir RuntimeError values at the expression span".into(),
            references: vec![EagerAtomSourceRef {
                source: "normal_dispatch".into(),
                line: path_new_line,
            }],
        },
        EagerAtomHelperBoundary {
            kind: "ExprParam slot access and freeze".into(),
            tags: vec!["ExprParam".into()],
            outcome: "usize decoding checks slot_count; the selected slot is frozen in place before it is cloned".into(),
            references: vec![
                EagerAtomSourceRef {
                    source: "normal_dispatch".into(),
                    line: param_freeze_line,
                },
                EagerAtomSourceRef {
                    source: "normal_dispatch".into(),
                    line: param_slot_lines[0],
                },
                EagerAtomSourceRef {
                    source: "normal_dispatch".into(),
                    line: param_slot_lines[1],
                },
            ],
        },
    ];
    Ok((
        arms,
        helper_fallback,
        helper_boundaries,
        EagerAtomValueContract {
            decode_error: "map_err(indexed_error(error, span)) propagates ExprView decode errors".into(),
            path_error: "map_err(indexed_error(IrVerifyError { message: error.message }, span)) propagates PathValue validation errors".into(),
            slot_bounds: "FullCodec<usize>::decode rejects slot >= decoder.slot_count before ExprParam reads slots[slot]".into(),
            freeze: "lowered_freeze_large_slot_list is infallible and runs before slots[slot].clone()".into(),
        },
    ))
}

fn eager_decode_error_boundary(block: &Block) -> Result<usize, String> {
    let Stmt::Local(local) = &block.stmts[0] else {
        return Err("eager atom helper does not bind its decoded ExprView".into());
    };
    if !matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "view") {
        return Err("eager atom helper binds decode to an unexpected local".into());
    }
    let init = local
        .init
        .as_ref()
        .ok_or_else(|| "eager atom decoded view has no initializer".to_string())?;
    let Expr::Try(try_expression) = init.expr.as_ref() else {
        return Err("eager atom ExprView decode error is not propagated".into());
    };
    let Expr::MethodCall(map_err) = try_expression.expr.as_ref() else {
        return Err("eager atom ExprView decode has no indexed-error conversion".into());
    };
    if map_err.method != "map_err" || map_err.args.len() != 1 {
        return Err("eager atom ExprView decode has an unexpected error boundary".into());
    }
    let Some(decode) = exact_call(&map_err.receiver, &["ExprView", "decode"]) else {
        return Err("eager atom helper does not decode ExprView".into());
    };
    if decode.args.len() != 3
        || !path_is_ident(&decode.args[0], "execution")
        || !path_is_ident(&decode.args[1], "tag")
        || !path_is_ident(&decode.args[2], "payload")
    {
        return Err("eager atom ExprView decode uses unexpected inputs".into());
    }
    let Expr::Closure(conversion) = &map_err.args[0] else {
        return Err("eager atom ExprView decode error conversion is not a closure".into());
    };
    if conversion.inputs.len() != 1
        || !matches!(conversion.inputs.first(), Some(Pat::Ident(pattern)) if pattern.ident == "error")
        || !is_indexed_error_call(&conversion.body, "error", false)
    {
        return Err("eager atom ExprView decode error must map through indexed_error(error, span)".into());
    }
    Ok(decode.func.span().start().line)
}

fn eager_atom_value_arm(
    variant: &str,
    tag: &str,
    pattern: &Pat,
    body: &Expr,
    arm_index: usize,
) -> Result<(EagerAtomViewArm, Option<usize>, Option<usize>, Vec<usize>), String> {
    let (expected_input, transformation, output) = match variant {
        "Null" => (None, "construct", "LoweredValue::Null"),
        "Unit" => (None, "construct", "LoweredValue::Unit"),
        "Int" => (Some("value"), "copy", "LoweredValue::Int"),
        "Float" => (Some("value"), "copy", "LoweredValue::Float"),
        "Duration" => (Some("value"), "copy", "LoweredValue::Duration"),
        "Bool" => (Some("value"), "copy", "LoweredValue::Bool"),
        "Str" => (Some("value"), "copy_borrowed_string", "LoweredValue::Str"),
        "Bytes" => (Some("value"), "copy_borrowed_bytes", "LoweredValue::Bytes"),
        "PreparedRegex" => (Some("value"), "clone_box", "LoweredValue::Regex"),
        "PreparedConstant" => (Some("value"), "clone_prepared_value", "prepared_constant.value"),
        "Path" => (Some("value"), "validate_path_bytes", "LoweredValue::Path"),
        "Param" => (Some("slot"), "freeze_slot_then_clone", "slots[slot]"),
        _ => return Err(format!("unknown EagerAtom value provenance for {variant}")),
    };
    let input = match (expected_input, eager_view_binding(pattern, variant)?) {
        (None, None) => "none".to_string(),
        (Some(expected), Some(actual)) if actual == expected => actual,
        (Some(expected), Some(actual)) => {
            return Err(format!(
                "ExprView::{variant} binds {actual}, expected schema input {expected}"
            ));
        }
        _ => return Err(format!("ExprView::{variant} has an unexpected payload binding")),
    };
    let mut path_line = None;
    let mut freeze_line = None;
    let mut slot_lines = Vec::new();
    match variant {
        "Null" | "Unit" => {
            if !path_has_segments(body, &["LoweredValue", variant]) {
                return Err(format!("ExprView::{variant} constructs the wrong value"));
            }
        }
        "Int" | "Float" | "Duration" | "Bool" => {
            if !eager_value_call(body, variant, "value") {
                return Err(format!("ExprView::{variant} does not preserve its decoded value"));
            }
        }
        "Str" | "Bytes" => {
            if !eager_arc_value_call(body, variant, "value") {
                return Err(format!("ExprView::{variant} does not preserve its decoded value"));
            }
        }
        "PreparedRegex" => {
            if !eager_regex_value(body) {
                return Err("ExprView::PreparedRegex no longer clones into LoweredValue::Regex".into());
            }
        }
        "PreparedConstant" => {
            if !eager_prepared_constant_value(body) {
                return Err("ExprView::PreparedConstant no longer clones its prepared value".into());
            }
        }
        "Path" => {
            path_line = Some(eager_path_value_line(body)?);
        }
        "Param" => {
            let (line, indexes) = eager_param_slot_value(body, "slot")?;
            freeze_line = Some(line);
            slot_lines = indexes;
        }
        _ => unreachable!("the EagerAtom variant list is closed"),
    }
    Ok((
        EagerAtomViewArm {
            variant: variant.into(),
            tag: tag.into(),
            input,
            transformation: transformation.into(),
            output: output.into(),
            line: pattern.span().start().line,
            arm_index,
        },
        path_line,
        freeze_line,
        slot_lines,
    ))
}

fn eager_view_binding(pattern: &Pat, variant: &str) -> Result<Option<String>, String> {
    match pattern {
        Pat::Path(path)
            if path.qself.is_none()
                && path.path.segments.len() == 2
                && path.path.segments[0].ident == "ExprView"
                && path.path.segments[1].ident == variant =>
        {
            Ok(None)
        }
        Pat::TupleStruct(pattern)
            if pattern.path.leading_colon.is_none()
                && pattern.path.segments.len() == 2
                && pattern.path.segments[0].ident == "ExprView"
                && pattern.path.segments[1].ident == variant
                && pattern.elems.len() == 1 =>
        {
            match pattern.elems.first() {
                Some(Pat::Ident(binding))
                    if binding.by_ref.is_none()
                        && binding.mutability.is_none()
                        && binding.subpat.is_none() => Ok(Some(binding.ident.to_string())),
                _ => Err(format!("ExprView::{variant} has an unsupported payload binding")),
            }
        }
        _ => Err(format!("ExprView::{variant} has an unexpected pattern")),
    }
}

fn eager_value_call(expression: &Expr, variant: &str, input: &str) -> bool {
    exact_call(expression, &["LoweredValue", variant])
        .is_some_and(|call| call.args.len() == 1 && path_is_ident(&call.args[0], input))
}

fn eager_arc_value_call(expression: &Expr, variant: &str, input: &str) -> bool {
    let Some(value) = exact_call(expression, &["LoweredValue", variant]) else {
        return false;
    };
    value.args.len() == 1
        && exact_call(&value.args[0], &["Arc", "from"])
            .is_some_and(|arc| arc.args.len() == 1 && path_is_ident(&arc.args[0], input))
}

fn eager_regex_value(expression: &Expr) -> bool {
    let Some(regex) = exact_call(expression, &["LoweredValue", "Regex"]) else {
        return false;
    };
    if regex.args.len() != 1 {
        return false;
    }
    let Some(boxed) = exact_call(&regex.args[0], &["Box", "new"]) else {
        return false;
    };
    if boxed.args.len() != 1 {
        return false;
    }
    matches!(&boxed.args[0], Expr::MethodCall(clone) if clone.method == "clone" && clone.args.is_empty() && path_is_ident(&clone.receiver, "value"))
}

fn eager_prepared_constant_value(expression: &Expr) -> bool {
    matches!(expression, Expr::MethodCall(clone)
        if clone.method == "clone"
            && clone.args.is_empty()
            && matches!(clone.receiver.as_ref(), Expr::Field(field)
                if matches!(&field.member, syn::Member::Unnamed(index) if index.index == 0)
                    && path_is_ident(&field.base, "value")))
}

fn eager_path_value_line(expression: &Expr) -> Result<usize, String> {
    let Some(path) = exact_call(expression, &["LoweredValue", "Path"]) else {
        return Err("ExprView::Path does not construct LoweredValue::Path".into());
    };
    if path.args.len() != 1 {
        return Err("ExprView::Path has an unexpected output constructor".into());
    }
    let Expr::Try(try_expression) = &path.args[0] else {
        return Err("ExprView::Path validation error is not propagated".into());
    };
    let Expr::MethodCall(map_err) = try_expression.expr.as_ref() else {
        return Err("ExprView::Path validation has no indexed-error conversion".into());
    };
    if map_err.method != "map_err" || map_err.args.len() != 1 {
        return Err("ExprView::Path has an unexpected error boundary".into());
    }
    let Some(new_path) = exact_call(&map_err.receiver, &["PathValue", "new"]) else {
        return Err("ExprView::Path does not call PathValue::new".into());
    };
    if new_path.args.len() != 1
        || !matches!(&new_path.args[0], Expr::MethodCall(to_vec) if to_vec.method == "to_vec" && to_vec.args.is_empty() && path_is_ident(&to_vec.receiver, "value"))
    {
        return Err("ExprView::Path does not validate the decoded path bytes".into());
    }
    let Expr::Closure(conversion) = &map_err.args[0] else {
        return Err("ExprView::Path error conversion is not a closure".into());
    };
    if conversion.inputs.len() != 1
        || !matches!(conversion.inputs.first(), Some(Pat::Ident(pattern)) if pattern.ident == "error")
        || !is_indexed_error_call(&conversion.body, "error", true)
    {
        return Err("ExprView::Path error must map through indexed_error with the expression span".into());
    }
    Ok(new_path.func.span().start().line)
}

fn is_indexed_error_call(expression: &Expr, error_name: &str, verify_error: bool) -> bool {
    let Some(indexed_error) = exact_call(expression, &["indexed_error"]) else {
        return false;
    };
    if indexed_error.args.len() != 2 || !path_is_ident(&indexed_error.args[1], "span") {
        return false;
    }
    if !verify_error {
        return path_is_ident(&indexed_error.args[0], error_name);
    }
    let Expr::Struct(error) = &indexed_error.args[0] else {
        return false;
    };
    if !path_has_segments(&Expr::Path(syn::ExprPath {
        attrs: Vec::new(),
        qself: None,
        path: error.path.clone(),
    }), &["IrVerifyError"])
        || error.fields.len() != 1
    {
        return false;
    }
    error.fields.iter().any(|field| {
        matches!(&field.member, syn::Member::Named(name) if name == "message")
            && matches!(&field.expr, Expr::Field(message)
                if matches!(&message.member, syn::Member::Named(name) if name == "message")
                    && path_is_ident(&message.base, error_name))
    })
}

fn eager_param_slot_value(body: &Expr, slot: &str) -> Result<(usize, Vec<usize>), String> {
    let Expr::Block(block) = body else {
        return Err("ExprView::Param does not freeze and clone its slot in order".into());
    };
    if block.block.stmts.len() != 2 {
        return Err("ExprView::Param has unexpected slot operations".into());
    }
    let Stmt::Expr(freeze, _) = &block.block.stmts[0] else {
        return Err("ExprView::Param does not freeze before cloning".into());
    };
    let Expr::Call(freeze) = freeze else {
        return Err("ExprView::Param does not freeze before cloning".into());
    };
    if !path_is_ident(&freeze.func, "lowered_freeze_large_slot_list") || freeze.args.len() != 1 {
        return Err("ExprView::Param calls the wrong slot freezer".into());
    }
    let Some(freeze_slot) = mutable_slot_index(&freeze.args[0], slot) else {
        return Err("ExprView::Param freezes a different slot".into());
    };
    let Stmt::Expr(cloned, _) = &block.block.stmts[1] else {
        return Err("ExprView::Param does not return the frozen slot".into());
    };
    let Expr::MethodCall(clone) = cloned else {
        return Err("ExprView::Param does not clone its selected slot".into());
    };
    if clone.method != "clone" || !clone.args.is_empty() {
        return Err("ExprView::Param has an unexpected slot clone".into());
    }
    let Some(clone_slot) = indexed_slot(&clone.receiver, slot) else {
        return Err("ExprView::Param clones a different slot".into());
    };
    Ok((
        freeze.func.span().start().line,
        vec![freeze_slot, clone_slot],
    ))
}

fn mutable_slot_index(expression: &Expr, slot: &str) -> Option<usize> {
    let Expr::Reference(reference) = expression else {
        return None;
    };
    if reference.mutability.is_none() {
        return None;
    }
    indexed_slot(&reference.expr, slot)
}

fn indexed_slot(expression: &Expr, slot: &str) -> Option<usize> {
    let Expr::Index(index) = expression else {
        return None;
    };
    if !path_is_ident(&index.expr, "slots") || !path_is_ident(&index.index, slot) {
        return None;
    }
    Some(index.index.span().start().line)
}

fn eager_slot_decode_contract(file: &File) -> Result<(usize, usize), String> {
    let implementations = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(item)
                if matches!(&*item.self_ty, syn::Type::Path(ty) if ty.qself.is_none() && ty.path.is_ident("usize")) =>
            {
                Some(item)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if implementations.len() != 1 {
        return Err(format!("expected one FullCodec<usize> implementation, found {}", implementations.len()));
    }
    let decoders = implementations[0]
        .items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(function) if function.sig.ident == "decode" => Some(function),
            _ => None,
        })
        .collect::<Vec<_>>();
    if decoders.len() != 1 {
        return Err(format!("FullCodec<usize> must define decode once, found {}", decoders.len()));
    }
    let decoder = decoders[0];
    if decoder.block.stmts.len() != 3 {
        return Err("FullCodec<usize>::decode must read, bounds-check, and return one slot".into());
    }
    let Stmt::Local(local) = &decoder.block.stmts[0] else {
        return Err("FullCodec<usize>::decode does not bind the raw slot".into());
    };
    if !matches!(&local.pat, Pat::Ident(pattern) if pattern.ident == "slot")
        || !local.init.as_ref().is_some_and(|init| is_usize_raw_read(&init.expr, "input"))
    {
        return Err("FullCodec<usize>::decode reads an unexpected slot value".into());
    }
    let Stmt::Expr(Expr::If(bounds), _) = &decoder.block.stmts[1] else {
        return Err("FullCodec<usize>::decode has no slot bounds check".into());
    };
    if !is_slot_count_check(&bounds.cond)
        || bounds.else_branch.is_some()
        || !is_slot_bounds_error(&bounds.then_branch)
    {
        return Err("FullCodec<usize>::decode has an unexpected slot bounds boundary".into());
    }
    let Stmt::Expr(returned, _) = &decoder.block.stmts[2] else {
        return Err("FullCodec<usize>::decode does not return its checked slot".into());
    };
    let Some(ok) = exact_call(returned, &["Ok"]) else {
        return Err("FullCodec<usize>::decode does not return Ok(slot)".into());
    };
    if ok.args.len() != 1 || !path_is_ident(&ok.args[0], "slot") {
        return Err("FullCodec<usize>::decode returns a different slot".into());
    }
    Ok((
        decoder.sig.ident.span().start().line,
        bounds.if_token.span.start().line,
    ))
}

fn is_usize_raw_read(expression: &Expr, input: &str) -> bool {
    let Expr::Cast(cast) = expression else {
        return false;
    };
    if !matches!(&*cast.ty, syn::Type::Path(ty) if ty.qself.is_none() && ty.path.is_ident("usize")) {
        return false;
    }
    matches!(&*cast.expr, Expr::Try(try_expression)
        if matches!(try_expression.expr.as_ref(), Expr::MethodCall(raw)
            if raw.method == "raw" && raw.args.is_empty() && path_is_ident(&raw.receiver, input)))
}

fn is_slot_count_check(expression: &Expr) -> bool {
    matches!(expression, Expr::Binary(binary)
        if matches!(binary.op, syn::BinOp::Ge(_))
            && path_is_ident(&binary.left, "slot")
            && matches!(&*binary.right, Expr::Cast(cast)
                if matches!(&*cast.expr, Expr::Field(field)
                    if matches!(&field.member, syn::Member::Named(name) if name == "slot_count")
                        && path_is_ident(&field.base, "decoder"))
                    && matches!(&*cast.ty, syn::Type::Path(ty) if ty.path.is_ident("usize"))))
}

fn is_slot_bounds_error(expression: &Block) -> bool {
    if expression.stmts.len() != 1 {
        return false;
    }
    let Stmt::Expr(Expr::Return(returned), _) = &expression.stmts[0] else {
        return false;
    };
    let Some(error) = returned.expr.as_deref().and_then(|expr| exact_call(expr, &["Err"])) else {
        return false;
    };
    if error.args.len() != 1 {
        return false;
    }
    let Some(new_error) = exact_call(&error.args[0], &["IrVerifyError", "new"]) else {
        return false;
    };
    matches!(new_error.args.first(), Some(Expr::Lit(literal))
        if matches!(&literal.lit, syn::Lit::Str(message) if message.value() == "slot is out of bounds"))
        && new_error.args.len() == 1
}

fn eager_slot_freeze_contract(file: &File) -> Result<EagerAtomFreezeContract, String> {
    let function = find_function_item(file, "lowered_freeze_large_slot_list")?;
    if !matches!(function.sig.output, syn::ReturnType::Default) || function.block.stmts.len() != 4 {
        return Err("slot freezer must be an infallible four-step list conversion".into());
    }
    let Stmt::Local(list_local) = &function.block.stmts[0] else {
        return Err("slot freezer does not select a list slot".into());
    };
    if !matches!(&list_local.pat, Pat::TupleStruct(pattern)
        if path_has_segments(&Expr::Path(syn::ExprPath { attrs: Vec::new(), qself: None, path: pattern.path.clone() }), &["LoweredValue", "List"])
            && pattern.elems.len() == 1
            && matches!(pattern.elems.first(), Some(Pat::Ident(items)) if items.ident == "items"))
        || !list_local.init.as_ref().is_some_and(|init| path_is_ident(&init.expr, "slot"))
    {
        return Err("slot freezer does not match LoweredValue::List(items) from slot".into());
    }
    let Stmt::Expr(Expr::If(threshold), _) = &function.block.stmts[1] else {
        return Err("slot freezer has no shared-list threshold".into());
    };
    if !matches!(&*threshold.cond, Expr::Binary(binary)
        if matches!(binary.op, syn::BinOp::Lt(_))
            && matches!(&*binary.left, Expr::MethodCall(len) if len.method == "len" && len.args.is_empty() && path_is_ident(&len.receiver, "items"))
            && path_is_ident(&binary.right, "LOWERED_SHARED_LIST_THRESHOLD"))
        || threshold.else_branch.is_some()
        || !is_bare_return_block(&threshold.then_branch)
    {
        return Err("slot freezer threshold must leave smaller lists unchanged".into());
    }
    let Stmt::Local(taken) = &function.block.stmts[2] else {
        return Err("slot freezer does not take the selected list".into());
    };
    if !matches!(&taken.pat, Pat::Ident(pattern) if pattern.ident == "items")
        || !taken.init.as_ref().is_some_and(|init| {
            exact_call(&init.expr, &["std", "mem", "take"])
                .is_some_and(|take| take.args.len() == 1 && path_is_ident(&take.args[0], "items"))
        })
    {
        return Err("slot freezer does not move list items with std::mem::take".into());
    }
    let Stmt::Expr(Expr::Assign(assignment), _) = &function.block.stmts[3] else {
        return Err("slot freezer does not replace the selected slot".into());
    };
    if !matches!(&*assignment.left, Expr::Unary(unary)
        if matches!(unary.op, syn::UnOp::Deref(_)) && path_is_ident(&unary.expr, "slot"))
        || !is_shared_list_constructor(&assignment.right)
    {
        return Err("slot freezer does not replace the list with SharedList".into());
    }
    let threshold_line = threshold.if_token.span.start().line;
    let list_take_line = taken
        .init
        .as_ref()
        .expect("the initializer was checked")
        .expr
        .span()
        .start()
        .line;
    Ok(EagerAtomFreezeContract {
        line: function.sig.ident.span().start().line,
        shared_threshold: "LOWERED_SHARED_LIST_THRESHOLD".into(),
        threshold_line,
        list_take_line,
        shared_list_line: assignment.right.span().start().line,
    })
}

fn find_function_item<'a>(file: &'a File, name: &str) -> Result<&'a ItemFn, String> {
    let functions = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == name => Some(function),
            _ => None,
        })
        .collect::<Vec<_>>();
    if functions.len() != 1 {
        return Err(format!("expected one function {name}, found {}", functions.len()));
    }
    Ok(functions[0])
}

fn is_bare_return_block(block: &Block) -> bool {
    block.stmts.len() == 1
        && matches!(&block.stmts[0], Stmt::Expr(Expr::Return(returned), _) if returned.expr.is_none())
}

fn is_shared_list_constructor(expression: &Expr) -> bool {
    let Some(shared) = exact_call(expression, &["LoweredValue", "SharedList"]) else {
        return false;
    };
    if shared.args.len() != 1 {
        return false;
    }
    exact_call(&shared.args[0], &["Arc", "new"])
        .is_some_and(|arc| arc.args.len() == 1 && path_is_ident(&arc.args[0], "items"))
}

#[derive(Default)]
struct EagerBoundaryFinder {
    decode_lines: Vec<usize>,
    path_new_lines: Vec<usize>,
    freeze_slot_lines: Vec<usize>,
}

impl<'ast> Visit<'ast> for EagerBoundaryFinder {
    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if path_has_segments(&expression.func, &["ExprView", "decode"]) {
            self.decode_lines.push(expression.func.span().start().line);
        }
        if path_has_segments(&expression.func, &["PathValue", "new"]) {
            self.path_new_lines.push(expression.func.span().start().line);
        }
        if path_is_ident(&expression.func, "lowered_freeze_large_slot_list") {
            self.freeze_slot_lines.push(expression.func.span().start().line);
        }
        visit::visit_expr_call(self, expression);
    }
}

#[derive(Default)]
struct SlotIndexFinder {
    count: usize,
}

impl<'ast> Visit<'ast> for SlotIndexFinder {
    fn visit_expr_index(&mut self, expression: &'ast syn::ExprIndex) {
        if path_is_ident(&expression.expr, "slots") && path_is_ident(&expression.index, "slot") {
            self.count += 1;
        }
        visit::visit_expr_index(self, expression);
    }
}

fn is_exact_unreachable(expression: &Expr, expected_message: &str) -> bool {
    let Expr::Macro(expression) = expression else {
        return false;
    };
    if !expression.mac.path.is_ident("unreachable") {
        return false;
    }
    matches!(
        syn::parse2::<syn::LitStr>(expression.mac.tokens.clone()),
        Ok(message) if message.value() == expected_message
    )
}

fn eager_atom_explicit_error_group(file: &File) -> Result<EagerAtomFallback, String> {
    let block = find_function(file, "eval_expr_view")?;
    let mut finder = EnumMatchFinder {
        family: "ExprViewKind",
        matches: Vec::new(),
    };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!(
            "eval_expr_view must have one ExprViewKind match, found {}",
            finder.matches.len()
        ));
    }
    let mut groups = Vec::new();
    for (arm_index, arm) in finder.matches[0].arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err("guarded ExprViewKind arm in explicit eager fallback".into());
        }
        if pattern_uses_enum(&arm.pat, "ExprViewKind") {
            let mut variants = Vec::new();
            collect_named_view_variants(&arm.pat, "ExprViewKind", &mut variants)?;
            let eager_variants = variants
                .into_iter()
                .filter(|variant| EAGER_VIEW_VARIANTS.contains(&variant.as_str()))
                .collect::<Vec<_>>();
            if !eager_variants.is_empty() {
                groups.push((arm_index, arm, eager_variants));
            }
        }
    }
    if groups.len() != 1 {
        return Err(format!(
            "eager ExprViewKind variants must share one explicit error group, found {}",
            groups.len()
        ));
    }
    let (arm_index, arm, variants) = &groups[0];
    let actual = variants.iter().cloned().collect::<BTreeSet<_>>();
    let expected = EAGER_VIEW_VARIANTS
        .iter()
        .map(|variant| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    if variants.len() != actual.len() || actual != expected {
        return Err(format!(
            "explicit eager ExprViewKind error group differs: expected {expected:?}, found {actual:?}"
        ));
    }
    if !is_return_frame_strategy_error_block(&arm.body) {
        return Err("explicit eager ExprViewKind group must directly return frame_strategy_error".into());
    }
    Ok(EagerAtomFallback {
        scope: "eager atoms misrouted to generic explicit expression-view dispatch".into(),
        outcome: "frame_strategy_error(tag, span)".into(),
        source: "explicit_dispatch".into(),
        line: arm.pat.span().start().line,
        arm_index: *arm_index,
    })
}

fn is_return_frame_strategy_error_block(expression: &Expr) -> bool {
    match expression {
        Expr::Block(block) if block.block.stmts.len() == 1 => {
            matches!(&block.block.stmts[0], Stmt::Expr(expression, _) if is_return_frame_strategy_error(expression))
        }
        expression => is_return_frame_strategy_error(expression),
    }
}

fn typed_scalar_schema_rows(file: &File) -> Result<Vec<ScalarSchemaRow>, String> {
    let macro_invocations = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item) if item.mac.path.is_ident("instruction_schema") => {
                Some(item.mac.tokens.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if macro_invocations.len() != 1 {
        return Err(format!(
            "expected one instruction_schema! invocation, found {}",
            macro_invocations.len()
        ));
    }

    let tokens = macro_invocations[0].clone().into_iter().collect::<Vec<_>>();
    let mut rows = Vec::new();
    rows.extend(parse_typed_schema_group(&tokens, "BuildIntRow", "IntView", "Int")?);
    rows.extend(parse_typed_schema_group(&tokens, "BuildBoolRow", "BoolView", "Bool")?);
    validate_scalar_schema_family(&rows, "Int", INT_TAGS)?;
    validate_scalar_schema_family(&rows, "Bool", BOOL_TAGS)?;
    let mut all_tags = BTreeSet::new();
    for row in &rows {
        if !all_tags.insert(row.tag.clone()) {
            return Err(format!("duplicate typed scalar schema tag {}", row.tag));
        }
    }
    rows.sort_by_key(|row| row.ordinal);
    Ok(rows)
}

fn parse_typed_schema_group(
    tokens: &[TokenTree],
    row_name: &str,
    view_name: &str,
    kind_name: &str,
) -> Result<Vec<ScalarSchemaRow>, String> {
    let mut groups = Vec::new();
    for index in 0..tokens.len() {
        if !ident_is(tokens.get(index), row_name) {
            continue;
        }
        if !arrow_at(tokens, index + 1) || !ident_is(tokens.get(index + 3), view_name) {
            return Err(format!("{row_name} has an unrecognized view layout"));
        }
        let kind = tokens.get(index + 4).and_then(group_tokens);
        if !kind.is_some_and(|kind| {
            let items = kind.into_iter().collect::<Vec<_>>();
            items.len() == 1 && ident_is(items.first(), kind_name)
        }) {
            return Err(format!("{row_name} is not [{kind_name}]"));
        }
        let Some(TokenTree::Group(block)) = tokens.get(index + 5) else {
            return Err(format!("{row_name} has no row block"));
        };
        if block.delimiter() != Delimiter::Brace {
            return Err(format!("{row_name} row block is not braced"));
        }
        groups.push((block.stream(), block.span().start().line));
    }
    if groups.len() != 1 {
        return Err(format!(
            "expected one {row_name} => {view_name} [{kind_name}] group, found {}",
            groups.len()
        ));
    }

    let mut rows = Vec::new();
    let mut row_tokens = Vec::new();
    for token in groups.into_iter().next().expect("one group checked").0 {
        if punct_is(&token, ';') {
            if row_tokens.is_empty() {
                return Err(format!("empty row in {row_name}"));
            }
            rows.push(parse_typed_schema_row(
                &row_tokens,
                row_name,
                view_name,
                kind_name,
            )?);
            row_tokens.clear();
        } else {
            row_tokens.push(token);
        }
    }
    if !row_tokens.is_empty() {
        return Err(format!("unterminated row in {row_name}"));
    }
    if rows.is_empty() {
        return Err(format!("{row_name} has no rows"));
    }
    Ok(rows)
}

fn parse_typed_schema_row(
    tokens: &[TokenTree],
    family: &str,
    view: &str,
    kind: &str,
) -> Result<ScalarSchemaRow, String> {
    let Some(TokenTree::Ident(variant)) = tokens.first() else {
        return Err(format!("{family} row has no view variant"));
    };
    let variant = variant.to_string();
    let mut arrow_index = 1;
    if matches!(tokens.get(arrow_index), Some(TokenTree::Group(group)) if matches!(group.delimiter(), Delimiter::Brace | Delimiter::Parenthesis)) {
        arrow_index += 1;
    }
    if !arrow_at(tokens, arrow_index) {
        return Err(format!("{family} row {variant} has an unrecognized payload layout"));
    }
    let tag = ident_value(tokens.get(arrow_index + 2))
        .ok_or_else(|| format!("{family} row {variant} has no tag"))?;
    if !punct_is_at(tokens, arrow_index + 3, '=') {
        return Err(format!("{family} row {tag} has no ordinal"));
    }
    let ordinal = match tokens.get(arrow_index + 4) {
        Some(TokenTree::Literal(literal)) => literal
            .to_string()
            .parse::<u16>()
            .map_err(|_| format!("{family} row {tag} has an invalid ordinal"))?,
        _ => return Err(format!("{family} row {tag} has a nonliteral ordinal")),
    };
    let Some(TokenTree::Group(metadata)) = tokens.get(arrow_index + 5) else {
        return Err(format!("{family} row {tag} has no route metadata"));
    };
    if metadata.delimiter() != Delimiter::Bracket || arrow_index + 6 != tokens.len() {
        return Err(format!("{family} row {tag} has an unrecognized route layout"));
    }
    let metadata = metadata.stream().into_iter().collect::<Vec<_>>();
    if metadata.len() != 5
        || !matches!(&metadata[0], TokenTree::Literal(literal) if literal.to_string() == "0")
        || !punct_is(&metadata[1], ';')
        || !ident_is(metadata.get(2), "Operands")
        || !punct_is(&metadata[3], ';')
        || !ident_is(metadata.get(4), "FrameApply")
    {
        return Err(format!(
            "typed scalar schema row {tag} must be pure Operands; FrameApply"
        ));
    }
    Ok(ScalarSchemaRow {
        family: kind.into(),
        kind: kind.into(),
        view: view.into(),
        variant,
        tag,
        ordinal,
        line: tokens[0].span().start().line,
    })
}

fn validate_scalar_schema_family(
    rows: &[ScalarSchemaRow],
    family: &str,
    expected_tags: &[&str],
) -> Result<(), String> {
    let actual = rows
        .iter()
        .filter(|row| row.family == family)
        .map(|row| row.tag.as_str())
        .collect::<BTreeSet<_>>();
    let expected = expected_tags.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "{family} schema cohort drift: expected {expected:?}, found {actual:?}"
        ));
    }
    Ok(())
}

struct MatchesPatternInput {
    target: Expr,
    _comma: Token![,],
    pattern: Pat,
}

impl Parse for MatchesPatternInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let target = input.parse()?;
        let comma = input.parse()?;
        let pattern = input.call(Pat::parse_multi)?;
        if !input.is_empty() {
            return Err(input.error("unexpected token after matches! pattern"));
        }
        Ok(Self {
            target,
            _comma: comma,
            pattern,
        })
    }
}

fn parse_typed_classifier(
    file: &File,
    function_name: &str,
    family: &str,
    expected_tags: &[&str],
) -> Result<Vec<TagBranch>, String> {
    let block = find_function(file, function_name)?;
    if block.stmts.len() != 1 {
        return Err(format!("{function_name} must contain one matches! expression"));
    }
    let expression = match &block.stmts[0] {
        Stmt::Expr(expression, None) => expression,
        _ => return Err(format!("{function_name} must return matches! directly")),
    };
    let Expr::Macro(expression) = expression else {
        return Err(format!("{function_name} is not a matches! classifier"));
    };
    if !expression.mac.path.is_ident("matches") {
        return Err(format!("{function_name} uses an unexpected classifier macro"));
    }
    let parsed = syn::parse2::<MatchesPatternInput>(expression.mac.tokens.clone())
        .map_err(|error| format!("parse {function_name} pattern: {error}"))?;
    if !path_is_ident(&parsed.target, "tag") {
        return Err(format!("{function_name} does not classify tag"));
    }
    let mut tags = Vec::new();
    collect_exact_tag_patterns(&parsed.pattern, &mut tags)?;
    validate_tag_set(family, &tags, expected_tags)?;
    let pattern = tags
        .iter()
        .map(|(tag, _)| format!("FullTag::{tag}"))
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(tags
        .into_iter()
        .map(|(tag, line)| TagBranch {
            tag,
            pattern: pattern.clone(),
            line,
            arm_index: 0,
        })
        .collect())
}

fn validate_tag_set(family: &str, tags: &[(String, usize)], expected: &[&str]) -> Result<(), String> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for (tag, _) in tags {
        if !expected.contains(tag.as_str()) {
            return Err(format!("unknown typed {family} tag {tag}"));
        }
        if !seen.insert(tag.as_str()) {
            return Err(format!("duplicate typed {family} tag {tag}"));
        }
    }
    let missing = expected
        .difference(&seen)
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing typed {family} tags: {missing:?}"));
    }
    Ok(())
}

fn validate_scalar_route_checks(file: &File) -> Result<(), String> {
    scalar_route_checks(file).map(|_| ())
}

fn scalar_route_checks(file: &File) -> Result<Vec<ScalarRouteCheck>, String> {
    let block = find_function(file, "eval_indexed_expr_inner")?;
    let mut checks = Vec::new();
    let mut generic_route_index = None;
    for (statement_index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Expr(Expr::If(expression), _) = statement else {
            continue;
        };
        if generic_route_index.is_none() && is_generic_scalar_route_condition(&expression.cond) {
            generic_route_index = Some(statement_index);
        }
        let Some(family) = scalar_classifier_condition(&expression.cond)? else {
            continue;
        };
        let (wrapper, result_variant) = scalar_route_return(expression, &family)?;
        checks.push(ScalarRouteCheck {
            family,
            line: expression.if_token.span.start().line,
            statement_index,
            wrapper,
            result_variant,
        });
    }
    if checks.len() != 2 {
        return Err(format!(
            "eval_indexed_expr_inner must have exactly two top-level typed scalar checks, found {}",
            checks.len()
        ));
    }
    if checks[0].family != "Int" || checks[1].family != "Bool" {
        return Err("typed scalar checks must route Int before Bool".into());
    }
    if checks[0].statement_index >= checks[1].statement_index {
        return Err("typed scalar checks are out of order".into());
    }
    let generic_route_index = generic_route_index
        .ok_or_else(|| "eval_indexed_expr_inner has no eager or frame-apply route".to_string())?;
    if checks.iter().any(|check| check.statement_index >= generic_route_index) {
        return Err("typed scalar checks occur after a generic evaluator route".into());
    }
    if checks[0].statement_index != 1 || checks[1].statement_index != 2 {
        return Err("typed scalar checks must immediately follow the indexed tag decode".into());
    }
    Ok(checks)
}

fn scalar_classifier_condition(condition: &Expr) -> Result<Option<String>, String> {
    let Expr::Call(call) = condition else {
        return Ok(None);
    };
    let Expr::Path(path) = call.func.as_ref() else {
        return Ok(None);
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return Ok(None);
    }
    let family = match path.path.segments[0].ident.to_string().as_str() {
        "is_typed_int_tag" => "Int",
        "is_typed_bool_tag" => "Bool",
        _ => return Ok(None),
    };
    if call.args.len() != 1 || !path_is_ident(&call.args[0], "tag") {
        return Err(format!("typed {family} route check has an unrecognized argument"));
    }
    Ok(Some(family.into()))
}

fn scalar_route_return(expression: &syn::ExprIf, family: &str) -> Result<(String, String), String> {
    if expression.else_branch.is_some() || expression.then_branch.stmts.len() != 1 {
        return Err(format!("typed {family} route check is not a direct return"));
    }
    let returned = match &expression.then_branch.stmts[0] {
        Stmt::Expr(Expr::Return(returned), _) => returned
            .expr
            .as_deref()
            .ok_or_else(|| format!("typed {family} route check returns no expression"))?,
        _ => return Err(format!("typed {family} route check does not return directly")),
    };
    let Expr::MethodCall(mapped) = returned else {
        return Err(format!("typed {family} route check has no typed result mapping"));
    };
    if mapped.method != "map" || mapped.args.len() != 1 {
        return Err(format!("typed {family} route check uses an unexpected mapping"));
    }
    let Expr::MethodCall(evaluator_call) = mapped.receiver.as_ref() else {
        return Err(format!("typed {family} route check does not call its scalar evaluator"));
    };
    let expected_wrapper = format!("eval_indexed_typed_{}", family.to_lowercase());
    if evaluator_call.method != expected_wrapper || !path_is_ident(&evaluator_call.receiver, "self") {
        return Err(format!("typed {family} route check calls the wrong scalar evaluator"));
    }
    let Expr::Closure(closure) = mapped.args.first().expect("one mapping argument checked") else {
        return Err(format!("typed {family} route mapping is not a closure"));
    };
    if closure.inputs.len() != 1
        || !matches!(closure.inputs.first(), Some(Pat::Ident(pattern)) if pattern.ident == "flow")
    {
        return Err(format!("typed {family} route closure has an unexpected input"));
    }
    let Expr::MethodCall(mapped_flow) = closure.body.as_ref() else {
        return Err(format!("typed {family} route does not map the continue value"));
    };
    if mapped_flow.method != "map_continue"
        || !path_is_ident(&mapped_flow.receiver, "flow")
        || mapped_flow.args.len() != 1
    {
        return Err(format!("typed {family} route has an unexpected flow mapping"));
    }
    let expected_result = if family == "Int" { "Int" } else { "Bool" };
    if !path_has_segments(
        mapped_flow.args.first().expect("one flow mapping checked"),
        &["LoweredValue", expected_result],
    ) {
        return Err(format!("typed {family} route maps to the wrong LoweredValue variant"));
    }
    Ok((expected_wrapper, expected_result.into()))
}

fn is_generic_scalar_route_condition(condition: &Expr) -> bool {
    match condition {
        Expr::MethodCall(call) if call.method == "is_eager_atom" => path_is_ident(&call.receiver, "tag"),
        Expr::Call(call) => {
            matches!(&*call.func, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "indexed_frame_apply_tag"))
                && call.args.len() == 1
                && path_is_ident(&call.args[0], "tag")
        }
        _ => false,
    }
}

fn typed_scalar_view_arms(
    file: &File,
    schema_rows: &[ScalarSchemaRow],
) -> Result<Vec<ScalarViewArm>, String> {
    let block = find_function(file, "eval_indexed_typed_scalar")?;
    let mut finder = ViewMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 2 {
        return Err(format!(
            "eval_indexed_typed_scalar must match IntView and BoolView once, found {} view matches",
            finder.matches.len()
        ));
    }
    let mut found = Vec::new();
    let mut families = BTreeSet::new();
    for match_expression in finder.matches {
        let mut arms = Vec::new();
        let mut family = None;
        for (arm_index, arm) in match_expression.arms.iter().enumerate() {
            if arm.guard.is_some() {
                return Err(format!(
                    "guarded typed scalar view arm at line {}",
                    arm.pat.span().start().line
                ));
            }
            let mut patterns = Vec::new();
            collect_view_patterns(&arm.pat, &mut patterns)?;
            if patterns.is_empty() {
                return Err(format!(
                    "unparsed typed scalar view arm at line {}",
                    arm.pat.span().start().line
                ));
            }
            for (arm_family, variant, line) in patterns {
                if family.as_deref().is_some_and(|previous| previous != arm_family) {
                    return Err("typed scalar view match mixes IntView and BoolView".into());
                }
                family = Some(arm_family.clone());
                arms.push(ScalarViewArm {
                    family: arm_family,
                    variant,
                    line,
                    arm_index,
                });
            }
        }
        let family = family.ok_or_else(|| "typed scalar view match has no exact arms".to_string())?;
        if !families.insert(family.clone()) {
            return Err(format!("duplicate {family} view match"));
        }
        let expected = schema_rows
            .iter()
            .filter(|row| row.family == family)
            .map(|row| row.variant.as_str())
            .collect::<BTreeSet<_>>();
        validate_view_coverage(&family, &arms, &expected)?;
        found.extend(arms);
    }
    if families != BTreeSet::from(["Bool".to_string(), "Int".to_string()]) {
        return Err(format!("typed scalar view families differ: {families:?}"));
    }
    Ok(found)
}

struct ViewMatchFinder<'ast> {
    matches: Vec<&'ast syn::ExprMatch>,
}

#[derive(Default)]
struct CallNameFinder {
    names: Vec<(String, usize)>,
}

impl<'ast> Visit<'ast> for CallNameFinder {
    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if let Expr::Path(path) = expression.func.as_ref() {
            if let Some(segment) = path.path.segments.last() {
                self.names
                    .push((segment.ident.to_string(), expression.func.span().start().line));
            }
        }
        visit::visit_expr_call(self, expression);
    }
}

struct CallNodeFinder<'name, 'ast> {
    name: &'name str,
    calls: Vec<&'ast syn::ExprCall>,
}

impl<'ast, 'name> Visit<'ast> for CallNodeFinder<'name, 'ast> {
    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if matches!(&*expression.func, Expr::Path(path)
            if path.path.segments.last().is_some_and(|segment| segment.ident.to_string() == self.name))
        {
            self.calls.push(expression);
        }
        visit::visit_expr_call(self, expression);
    }
}

struct MethodCallNodeFinder<'name, 'ast> {
    name: &'name str,
    calls: Vec<&'ast syn::ExprMethodCall>,
}

impl<'ast, 'name> Visit<'ast> for MethodCallNodeFinder<'name, 'ast> {
    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if expression.method == self.name {
            self.calls.push(expression);
        }
        visit::visit_expr_method_call(self, expression);
    }
}

impl<'ast> Visit<'ast> for ViewMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if path_is_ident(&expression.expr, "view") {
            self.matches.push(expression);
        }
        visit::visit_expr_match(self, expression);
    }
}

fn collect_view_patterns(
    pattern: &Pat,
    output: &mut Vec<(String, String, usize)>,
) -> Result<(), String> {
    let path = match pattern {
        Pat::Path(path) => Some(&path.path),
        Pat::TupleStruct(pattern) => Some(&pattern.path),
        Pat::Struct(pattern) => Some(&pattern.path),
        Pat::Or(patterns) => {
            for pattern in &patterns.cases {
                collect_view_patterns(pattern, output)?;
            }
            return Ok(());
        }
        Pat::Paren(pattern) => {
            collect_view_patterns(&pattern.pat, output)?;
            return Ok(());
        }
        _ => None,
    };
    let Some(path) = path else {
        return Err(format!("unsupported typed scalar view pattern at line {}", pattern.span().start().line));
    };
    if path.leading_colon.is_some() || path.segments.len() != 2 {
        return Err(format!("unparsed typed scalar view path at line {}", pattern.span().start().line));
    }
    let family = path.segments[0].ident.to_string();
    if family != "IntView" && family != "BoolView" {
        return Err(format!("unexpected typed scalar view family {family}"));
    }
    let variant = path.segments[1].ident.to_string();
    output.push((family.strip_suffix("View").unwrap().into(), variant, pattern.span().start().line));
    Ok(())
}

fn validate_view_coverage(
    family: &str,
    arms: &[ScalarViewArm],
    expected: &BTreeSet<&str>,
) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    for arm in arms {
        if !expected.contains(arm.variant.as_str()) {
            return Err(format!("unknown {family} view arm {}", arm.variant));
        }
        if !actual.insert(arm.variant.as_str()) {
            return Err(format!("duplicate {family} view arm {}", arm.variant));
        }
    }
    let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing {family} view arms: {missing:?}"));
    }
    Ok(())
}

fn explicit_scalar_groups(
    file: &File,
) -> Result<(ScalarRouteCheck, Vec<ScalarFrameGroup>, Vec<ScalarFallback>), String> {
    let forced_entry = validate_explicit_scalar_entry(file)?;
    let block = find_function(file, "eval_typed_scalar_expr")?;
    let mut finder = TagMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err(format!(
            "eval_typed_scalar_expr must contain one match tag, found {}",
            finder.matches.len()
        ));
    }
    let tag_match = finder.matches[0];
    let mut int_group = None;
    let mut bool_group = None;
    let mut delegated = None;
    let mut generic_fallback = None;
    let mut fallbacks = Vec::new();

    for (arm_index, arm) in tag_match.arms.iter().enumerate() {
        if let Some(guard) = &arm.guard {
            if arm_index != 2 || !is_typed_scalar_guard(&arm.pat, &guard.1) || !path_is_ident(&arm.body, "None") {
                return Err(format!(
                    "unrecognized typed scalar delegation arm at line {}",
                    arm.pat.span().start().line
                ));
            }
            validate_typed_scalar_none_delegation(block)?;
            delegated = Some(ScalarFrameGroup {
                tags: BOOL_TAGS
                    .iter()
                    .chain(INT_TAGS)
                    .filter(|tag| !matches!(**tag, "IntBinary" | "IntStrByteAtSlot" | "BoolNot" | "BoolAnd" | "BoolOr" | "BoolIntCompare"))
                    .map(|tag| (*tag).into())
                    .collect(),
                behavior: "verified_in_place_typed_delegation".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
            continue;
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            if arm_index != 3 || !is_return_frame_strategy_error(&arm.body) {
                return Err("typed scalar outer wildcard must directly return frame_strategy_error".into());
            }
            generic_fallback = Some(ScalarFallback {
                applies_to: Vec::new(),
                scope: "tag outside the typed scalar cohort".into(),
                outcome: "frame_strategy_error: unmatched tag".into(),
                source: "explicit_dispatch".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
            continue;
        }

        let mut tags = Vec::new();
        collect_exact_tag_patterns(&arm.pat, &mut tags)?;
        let tag_names = tags.iter().map(|(tag, _)| tag.clone()).collect::<Vec<_>>();
        if tag_names == ["IntBinary", "IntStrByteAtSlot"] && arm_index == 0 {
            let fallback = validate_typed_frame_view_match(&arm.body, "IntView", &["Binary", "StrByteAtSlot"])?;
            fallbacks.push(ScalarFallback {
                applies_to: tag_names.clone(),
                scope: "typed tag with mismatched decoded view".into(),
                outcome: "frame_strategy_error: decoded IntView does not match tag".into(),
                source: "explicit_dispatch".into(),
                line: fallback.0,
                arm_index: fallback.1,
            });
            int_group = Some(ScalarFrameGroup {
                tags: tag_names,
                behavior: "frame_operation".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
        } else if tag_names == ["BoolNot", "BoolAnd", "BoolOr", "BoolIntCompare"] && arm_index == 1 {
            let fallback = validate_typed_frame_view_match(
                &arm.body,
                "BoolView",
                &["Not", "And", "Or", "IntCompare"],
            )?;
            fallbacks.push(ScalarFallback {
                applies_to: tag_names.clone(),
                scope: "typed tag with mismatched decoded view".into(),
                outcome: "frame_strategy_error: decoded BoolView does not match tag".into(),
                source: "explicit_dispatch".into(),
                line: fallback.0,
                arm_index: fallback.1,
            });
            bool_group = Some(ScalarFrameGroup {
                tags: tag_names,
                behavior: "frame_operation".into(),
                line: arm.pat.span().start().line,
                arm_index,
            });
        } else {
            return Err(format!("unrecognized typed scalar frame arm: {tag_names:?}"));
        }
    }

    let int_group = int_group.ok_or_else(|| "missing Int frame-operation group".to_string())?;
    let bool_group = bool_group.ok_or_else(|| "missing Bool frame-operation group".to_string())?;
    let delegated = delegated.ok_or_else(|| "missing typed scalar delegation guard".to_string())?;
    let generic_fallback = generic_fallback.ok_or_else(|| "missing typed scalar wildcard fallback".to_string())?;
    fallbacks.push(generic_fallback);
    let in_place_line = scalar_none_fallback(block)?;
    fallbacks.push(ScalarFallback {
        applies_to: delegated.tags.clone(),
        scope: "delegated typed leaf without verifier in-place proof".into(),
        outcome: "frame_strategy_error: verifier in-place proof absent".into(),
        source: "explicit_dispatch".into(),
        line: in_place_line,
        arm_index: 2,
    });
    Ok((forced_entry, vec![int_group, bool_group, delegated], fallbacks))
}

fn validate_explicit_scalar_entry(file: &File) -> Result<ScalarRouteCheck, String> {
    let block = find_function(file, "eval_expr")?;
    let mut eager = None;
    let mut typed = None;
    let mut generic = None;
    for (index, statement) in block.stmts.iter().enumerate() {
        let Stmt::Expr(Expr::If(expression), _) = statement else {
            continue;
        };
        if is_eager_atom_condition(&expression.cond) {
            if eager.replace(index).is_some() {
                return Err("eval_expr has duplicate eager atom branches".into());
            }
        } else if is_typed_frame_apply_condition(&expression.cond) {
            if !if_returns_typed_scalar_helper(expression) {
                return Err("typed scalar FrameApply branch does not call eval_typed_scalar_expr".into());
            }
            if typed.replace(index).is_some() {
                return Err("eval_expr has duplicate typed scalar FrameApply branches".into());
            }
        } else if is_indexed_frame_apply_condition(&expression.cond) {
            if generic.replace(index).is_some() {
                return Err("eval_expr has duplicate generic FrameApply branches".into());
            }
        }
    }
    let eager = eager.ok_or_else(|| "eval_expr has no eager atom route".to_string())?;
    let typed = typed.ok_or_else(|| "eval_expr has no typed scalar FrameApply route".to_string())?;
    let generic = generic.ok_or_else(|| "eval_expr has no generic FrameApply route".to_string())?;
    if !(eager < typed && typed < generic) {
        return Err("eager atom and typed scalar branches are out of order".into());
    }
    let Stmt::Expr(Expr::If(expression), _) = &block.stmts[typed] else {
        unreachable!("typed route index was found from an if expression")
    };
    Ok(ScalarRouteCheck {
        family: "FrameApply".into(),
        line: expression.if_token.span.start().line,
        statement_index: typed,
        wrapper: "eval_typed_scalar_expr".into(),
        result_variant: "typed scalar view".into(),
    })
}

fn is_eager_atom_condition(expression: &Expr) -> bool {
    matches!(expression, Expr::MethodCall(call) if call.method == "is_eager_atom" && path_is_ident(&call.receiver, "tag"))
}

fn is_indexed_frame_apply_condition(expression: &Expr) -> bool {
    matches!(expression, Expr::Call(call) if call.args.len() == 1 && path_is_ident(&call.args[0], "tag") && matches!(&*call.func, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "indexed_frame_apply_tag")))
}

fn is_typed_frame_apply_condition(expression: &Expr) -> bool {
    let Expr::Binary(binary) = expression else {
        return false;
    };
    if !matches!(binary.op, syn::BinOp::And(_))
        || !is_frame_apply_strategy_comparison(&binary.left)
    {
        return false;
    }
    let Expr::Paren(or_expression) = binary.right.as_ref() else {
        return false;
    };
    let Expr::Binary(or) = or_expression.expr.as_ref() else {
        return false;
    };
    matches!(or.op, syn::BinOp::Or(_))
        && is_super_classifier_call(&or.left, "is_typed_int_tag")
        && is_super_classifier_call(&or.right, "is_typed_bool_tag")
}

fn is_frame_apply_strategy_comparison(expression: &Expr) -> bool {
    let Expr::Binary(binary) = expression else {
        return false;
    };
    matches!(binary.op, syn::BinOp::Eq(_))
        && path_is_ident(&binary.left, "strategy")
        && path_has_segments(&binary.right, &["FrameStrategy", "FrameApply"])
}

fn is_super_classifier_call(expression: &Expr, name: &str) -> bool {
    let Expr::Call(call) = expression else {
        return false;
    };
    call.args.len() == 1
        && path_is_ident(&call.args[0], "tag")
        && matches!(&*call.func, Expr::Path(path) if path.path.segments.len() == 2 && path.path.segments[0].ident == "super" && path.path.segments[1].ident == name)
}

fn if_returns_typed_scalar_helper(expression: &syn::ExprIf) -> bool {
    if expression.else_branch.is_some() || expression.then_branch.stmts.len() != 1 {
        return false;
    }
    match &expression.then_branch.stmts[0] {
        Stmt::Expr(Expr::Return(returned), _) => match returned.expr.as_deref() {
            Some(Expr::MethodCall(call)) => {
                call.method == "eval_typed_scalar_expr" && path_is_ident(&call.receiver, "self")
            }
            _ => false,
        },
        _ => false,
    }
}

fn validate_typed_frame_view_match(
    expression: &Expr,
    expected_family: &str,
    expected_variants: &[&str],
) -> Result<(usize, usize), String> {
    let mut finder = EnumMatchFinder {
        family: expected_family,
        matches: Vec::new(),
    };
    finder.visit_expr(expression);
    if finder.matches.len() != 1 {
        return Err(format!(
            "typed {expected_family} frame arm must contain one decoded view match, found {}",
            finder.matches.len()
        ));
    }
    let view_match = finder.matches[0];
    let mut actual = BTreeSet::new();
    let mut actual_groups = Vec::new();
    let mut fallback = None;
    for (index, arm) in view_match.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!("guarded {expected_family} frame-view arm"));
        }
        if matches!(arm.pat, Pat::Wild(_)) {
            if !is_return_frame_strategy_error(&arm.body) || fallback.replace((arm.pat.span().start().line, index)).is_some() {
                return Err(format!("{expected_family} frame-view wildcard is not a direct error"));
            }
            continue;
        }
        let mut variants = Vec::new();
        collect_named_view_variants(&arm.pat, expected_family, &mut variants)?;
        let group = variants.iter().cloned().collect::<BTreeSet<_>>();
        if group.len() != variants.len() {
            return Err(format!("duplicate variant in {expected_family} grouped arm"));
        }
        actual_groups.push(group);
        for variant in variants {
            if !actual.insert(variant.clone()) {
                return Err(format!("duplicate {expected_family} frame-view arm {variant}"));
            }
        }
    }
    let expected = expected_variants
        .iter()
        .map(|variant| (*variant).to_string())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "{expected_family} frame-operation view arms differ: expected {expected:?}, found {actual:?}"
        ));
    }
    let expected_groups = match expected_family {
        "IntView" => vec![
            BTreeSet::from(["Binary".to_string()]),
            BTreeSet::from(["StrByteAtSlot".to_string()]),
        ],
        "BoolView" => vec![
            BTreeSet::from(["Not".to_string()]),
            BTreeSet::from(["And".to_string(), "Or".to_string()]),
            BTreeSet::from(["IntCompare".to_string()]),
        ],
        _ => unreachable!("only typed scalar view families are requested"),
    };
    if actual_groups != expected_groups {
        return Err(format!(
            "{expected_family} frame-operation group shapes differ: {actual_groups:?}"
        ));
    }
    fallback.ok_or_else(|| format!("{expected_family} frame-operation match has no wildcard error"))
}

struct EnumMatchFinder<'ast> {
    family: &'ast str,
    matches: Vec<&'ast syn::ExprMatch>,
}

impl<'ast> Visit<'ast> for EnumMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if expression.arms.iter().any(|arm| pattern_uses_enum(&arm.pat, self.family)) {
            self.matches.push(expression);
        }
        visit::visit_expr_match(self, expression);
    }
}

fn pattern_uses_enum(pattern: &Pat, enum_name: &str) -> bool {
    match pattern {
        Pat::Path(path) => path.path.segments.iter().any(|segment| segment.ident == enum_name),
        Pat::TupleStruct(pattern) => pattern.path.segments.iter().any(|segment| segment.ident == enum_name),
        Pat::Struct(pattern) => pattern.path.segments.iter().any(|segment| segment.ident == enum_name),
        Pat::Or(patterns) => patterns.cases.iter().any(|pattern| pattern_uses_enum(pattern, enum_name)),
        Pat::Paren(pattern) => pattern_uses_enum(&pattern.pat, enum_name),
        _ => false,
    }
}

fn collect_named_view_variants(pattern: &Pat, family: &str, output: &mut Vec<String>) -> Result<(), String> {
    match pattern {
        Pat::Or(patterns) => {
            for pattern in &patterns.cases {
                collect_named_view_variants(pattern, family, output)?;
            }
            Ok(())
        }
        Pat::Paren(pattern) => collect_named_view_variants(&pattern.pat, family, output),
        Pat::Path(path) => push_view_path(&path.path, family, output),
        Pat::TupleStruct(pattern) => push_view_path(&pattern.path, family, output),
        Pat::Struct(pattern) => push_view_path(&pattern.path, family, output),
        _ => Err(format!("unsupported {family} frame view pattern")),
    }
}

fn push_view_path(path: &syn::Path, family: &str, output: &mut Vec<String>) -> Result<(), String> {
    if path.leading_colon.is_some()
        || path.segments.len() != 2
        || path.segments[0].ident != family
    {
        return Err(format!("unrecognized {family} frame view path"));
    }
    output.push(path.segments[1].ident.to_string());
    Ok(())
}

fn validate_typed_scalar_none_delegation(block: &Block) -> Result<(), String> {
    let mut finder = OperationMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    if finder.matches.len() != 1 {
        return Err("eval_typed_scalar_expr must match operation once".into());
    }
    let operation_match = finder.matches[0];
    let none_arm = operation_match
        .arms
        .iter()
        .find(|arm| is_simple_path_pattern(&arm.pat, "None"))
        .ok_or_else(|| "typed scalar operation match has no None delegation arm".to_string())?;
    let body = match none_arm.body.as_ref() {
        Expr::Block(block) => &block.block,
        _ => return Err("typed scalar None delegation is not a block".into()),
    };
    let mut calls = InPlaceCallFinder { calls: Vec::new() };
    calls.visit_block(body);
    if calls.calls.len() != 1 || !is_in_place_method_call(calls.calls[0]) {
        return Err("typed scalar delegation must check its verifier in-place proof once".into());
    }
    let mut if_finder = InPlaceIfFinder { expressions: Vec::new() };
    if_finder.visit_block(body);
    if if_finder.expressions.len() != 1 {
        return Err("typed scalar delegation must have one in-place rejection".into());
    }
    let in_place_if = if_finder.expressions[0];
    if !is_direct_in_place_rejection(in_place_if) {
        return Err("missing-in-place branch must directly return frame_strategy_error".into());
    }
    let int_route = find_classifier_if(body, "is_typed_int_tag")?;
    if int_route.else_branch.is_none()
        || count_method_calls_in_block(&int_route.then_branch, "eval_indexed_typed_int") != 1
        || count_method_calls_in_block(&int_route.then_branch, "eval_indexed_typed_bool") != 0
        || count_method_calls(int_route.else_branch.as_ref().expect("else checked").1.as_ref(), "eval_indexed_typed_bool") != 1
        || count_method_calls(int_route.else_branch.as_ref().expect("else checked").1.as_ref(), "eval_indexed_typed_int") != 0
    {
        return Err("typed scalar in-place delegation calls the wrong typed evaluator".into());
    }
    Ok(())
}

fn scalar_none_fallback(block: &Block) -> Result<usize, String> {
    let mut finder = OperationMatchFinder { matches: Vec::new() };
    finder.visit_block(block);
    let operation_match = *finder
        .matches
        .first()
        .ok_or_else(|| "typed scalar operation match is missing".to_string())?;
    let none_arm = operation_match
        .arms
        .iter()
        .find(|arm| is_simple_path_pattern(&arm.pat, "None"))
        .ok_or_else(|| "typed scalar None delegation arm is missing".to_string())?;
    let body = match none_arm.body.as_ref() {
        Expr::Block(block) => &block.block,
        _ => return Err("typed scalar None delegation is not a block".into()),
    };
    let mut if_finder = InPlaceIfFinder { expressions: Vec::new() };
    if_finder.visit_block(body);
    let expression = if_finder.expressions.first().ok_or_else(|| "typed scalar in-place fallback is missing".to_string())?;
    Ok(expression.if_token.span.start().line)
}

fn find_classifier_if<'a>(block: &'a Block, expected: &str) -> Result<&'a syn::ExprIf, String> {
    let mut finder = ClassifierIfFinder {
        expected,
        expressions: Vec::new(),
    };
    finder.visit_block(block);
    if finder.expressions.len() != 1 {
        return Err(format!("expected one {expected} delegation branch"));
    }
    Ok(finder.expressions[0])
}

fn count_method_calls(expression: &Expr, name: &str) -> usize {
    let mut finder = MethodCallFinder {
        name,
        count: 0,
    };
    finder.visit_expr(expression);
    finder.count
}

fn typed_union_errors(file: &File) -> Result<Vec<ScalarWrongUnion>, String> {
    let mut errors = Vec::new();
    for (function_name, family, expected_variant, wrong_variant, message) in [
        ("eval_indexed_typed_int", "Int", "Int", "Bool", "typed integer evaluator produced Bool"),
        ("eval_indexed_typed_bool", "Bool", "Bool", "Int", "typed boolean evaluator produced Int"),
    ] {
        let block = find_function(file, function_name)?;
        let mut finder = AllMatchFinder { matches: Vec::new() };
        finder.visit_block(block);
        if finder.matches.len() != 1 {
            return Err(format!("{function_name} must contain one typed result match"));
        }
        let result_match = finder.matches[0];
        if result_match.arms.len() != 2 || result_match.arms.iter().any(|arm| arm.guard.is_some()) {
            return Err(format!("{function_name} has an unrecognized typed result match"));
        }
        let patterns = result_match
            .arms
            .iter()
            .map(|arm| enum_pattern_name(&arm.pat, "TypedScalarValue"))
            .collect::<Result<Vec<_>, _>>()?;
        if patterns.len() != 2 || patterns[0] != expected_variant || patterns[1] != wrong_variant {
            return Err(format!("{function_name} typed result arms differ: {patterns:?}"));
        }
        let wrong_arm = &result_match.arms[1];
        if !is_typed_union_error(&wrong_arm.body, message) {
            return Err(format!("{function_name} wrong-union branch changed from its defensive error"));
        }
        errors.push(ScalarWrongUnion {
            family: family.into(),
            outcome: message.into(),
            line: wrong_arm.pat.span().start().line,
        });
    }
    Ok(errors)
}

fn is_typed_scalar_guard(pattern: &Pat, expression: &Expr) -> bool {
    matches!(pattern, Pat::Ident(pattern) if pattern.ident == "tag" && pattern.subpat.is_none())
        && matches!(expression, Expr::Binary(binary) if matches!(binary.op, syn::BinOp::Or(_)) && is_super_classifier_call(&binary.left, "is_typed_int_tag") && is_super_classifier_call(&binary.right, "is_typed_bool_tag"))
}

fn is_return_frame_strategy_error(expression: &Expr) -> bool {
    match expression {
        Expr::Return(returned) => returned
            .expr
            .as_deref()
            .is_some_and(is_frame_strategy_error_fallback),
        expression => is_frame_strategy_error_fallback(expression),
    }
}

fn is_direct_in_place_rejection(expression: &syn::ExprIf) -> bool {
    expression.else_branch.is_none()
        && is_missing_in_place_condition(&expression.cond)
        && expression.then_branch.stmts.len() == 1
        && matches!(&expression.then_branch.stmts[0], Stmt::Expr(returned, _) if is_return_frame_strategy_error(returned))
}

fn is_missing_in_place_condition(expression: &Expr) -> bool {
    let Expr::Unary(unary) = expression else {
        return false;
    };
    matches!(unary.op, syn::UnOp::Not(_)) && is_in_place_call(&unary.expr)
}

fn is_in_place_call(expression: &Expr) -> bool {
    let Expr::MethodCall(call) = expression else {
        return false;
    };
    is_in_place_method_call(call)
}

fn is_in_place_method_call(call: &syn::ExprMethodCall) -> bool {
    call.method == "is_in_place"
        && call.args.len() == 1
        && path_is_ident(&call.args[0], "instruction")
        && is_field_access(&call.receiver, "call", "execution")
}

fn is_field_access(expression: &Expr, base_name: &str, field_name: &str) -> bool {
    matches!(expression, Expr::Field(field) if matches!(&field.member, syn::Member::Named(name) if name == field_name) && path_is_ident(&field.base, base_name))
}

fn is_simple_path_pattern(pattern: &Pat, expected: &str) -> bool {
    matches!(pattern, Pat::Path(path) if path.qself.is_none() && path.path.is_ident(expected))
        || matches!(pattern, Pat::Ident(pattern) if pattern.ident == expected && pattern.subpat.is_none())
}

fn enum_pattern_name(pattern: &Pat, expected_enum: &str) -> Result<String, String> {
    let path = match pattern {
        Pat::Path(path) => &path.path,
        Pat::TupleStruct(pattern) => &pattern.path,
        Pat::Struct(pattern) => &pattern.path,
        _ => return Err(format!("unexpected {expected_enum} pattern")),
    };
    if path.leading_colon.is_some()
        || path.segments.len() != 2
        || path.segments[0].ident != expected_enum
    {
        return Err(format!("unexpected {expected_enum} pattern path"));
    }
    Ok(path.segments[1].ident.to_string())
}

fn is_typed_union_error(expression: &Expr, expected_message: &str) -> bool {
    let Expr::Call(error_call) = expression else {
        return false;
    };
    if !path_is_ident(&error_call.func, "Err") || error_call.args.len() != 1 {
        return false;
    }
    let Expr::MethodCall(with_span) = &error_call.args[0] else {
        return false;
    };
    if with_span.method != "with_span"
        || with_span.args.len() != 1
        || !path_is_ident(&with_span.args[0], "call_span")
    {
        return false;
    }
    let Expr::Call(new_error) = with_span.receiver.as_ref() else {
        return false;
    };
    if !path_has_segments(&new_error.func, &["RuntimeError", "new"]) || new_error.args.len() != 2 {
        return false;
    }
    matches!(
        (new_error.args.first(), new_error.args.iter().nth(1)),
        (Some(Expr::Lit(domain)), Some(Expr::Lit(message)))
            if matches!(&domain.lit, syn::Lit::Str(domain) if domain.value() == "indexed-ir")
                && matches!(&message.lit, syn::Lit::Str(message) if message.value() == expected_message)
    )
}

fn path_has_segments(expression: &Expr, expected: &[&str]) -> bool {
    matches!(expression, Expr::Path(path) if path.qself.is_none() && path.path.segments.len() == expected.len() && path.path.segments.iter().zip(expected).all(|(actual, expected)| actual.ident == *expected))
}

fn path_ends_with_segments(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments.len() >= expected.len()
        && path.segments.iter().rev().take(expected.len()).map(|segment| segment.ident.to_string()).collect::<Vec<_>>()
            == expected.iter().rev().map(|segment| (*segment).to_string()).collect::<Vec<_>>()
}

struct TagMatchFinder<'ast> {
    matches: Vec<&'ast syn::ExprMatch>,
}

impl<'ast> Visit<'ast> for TagMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if path_is_ident(&expression.expr, "tag") {
            self.matches.push(expression);
        }
        visit::visit_expr_match(self, expression);
    }
}

struct OperationMatchFinder<'ast> {
    matches: Vec<&'ast syn::ExprMatch>,
}

impl<'ast> Visit<'ast> for OperationMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        if path_is_ident(&expression.expr, "operation") {
            self.matches.push(expression);
        }
        visit::visit_expr_match(self, expression);
    }
}

struct AllMatchFinder<'ast> {
    matches: Vec<&'ast syn::ExprMatch>,
}

impl<'ast> Visit<'ast> for AllMatchFinder<'ast> {
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        self.matches.push(expression);
        visit::visit_expr_match(self, expression);
    }
}

struct InPlaceCallFinder<'ast> {
    calls: Vec<&'ast syn::ExprMethodCall>,
}

impl<'ast> Visit<'ast> for InPlaceCallFinder<'ast> {
    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if expression.method == "is_in_place" {
            self.calls.push(expression);
        }
        visit::visit_expr_method_call(self, expression);
    }
}

struct InPlaceIfFinder<'ast> {
    expressions: Vec<&'ast syn::ExprIf>,
}

impl<'ast> Visit<'ast> for InPlaceIfFinder<'ast> {
    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if is_missing_in_place_condition(&expression.cond) {
            self.expressions.push(expression);
        }
        visit::visit_expr_if(self, expression);
    }
}

struct ClassifierIfFinder<'ast, 'name> {
    expected: &'name str,
    expressions: Vec<&'ast syn::ExprIf>,
}

impl<'ast, 'name> Visit<'ast> for ClassifierIfFinder<'ast, 'name> {
    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if is_super_classifier_call(&expression.cond, self.expected) {
            self.expressions.push(expression);
        }
        visit::visit_expr_if(self, expression);
    }
}

struct MethodCallFinder<'name> {
    name: &'name str,
    count: usize,
}

impl<'ast, 'name> Visit<'ast> for MethodCallFinder<'name> {
    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if expression.method == self.name {
            self.count += 1;
        }
        visit::visit_expr_method_call(self, expression);
    }
}

fn count_method_calls_in_block(block: &Block, name: &str) -> usize {
    let mut finder = MethodCallFinder { name, count: 0 };
    finder.visit_block(block);
    finder.count
}

fn is_frame_strategy_error_fallback(expression: &Expr) -> bool {
    let Expr::Call(error) = expression else {
        return false;
    };
    if !path_is_ident(&error.func, "Err") || error.args.len() != 1 {
        return false;
    }
    let Expr::Call(strategy_error) = &error.args[0] else {
        return false;
    };
    path_is_ident(&strategy_error.func, "frame_strategy_error")
        && strategy_error.args.len() == 2
        && path_is_ident(&strategy_error.args[0], "tag")
        && path_is_ident(&strategy_error.args[1], "span")
}

pub fn extract_statement_inventory(
    schema_source: &str,
    dispatch_source: &str,
    dispatch_path: &str,
) -> Result<StatementInventory, String> {
    let schema = syn::parse_file(schema_source)
        .map_err(|error| format!("parse instruction schema: {error}"))?;
    let dispatch = syn::parse_file(dispatch_source)
        .map_err(|error| format!("parse statement dispatch: {error}"))?;
    let schema_tags = statement_schema_tags(&schema)?;
    let function = find_function(&dispatch, "eval_statement")?;
    let match_expr = top_level_tag_match(function)?;
    let (branches, fallbacks) = parse_dispatch_arms(match_expr)?;
    validate_coverage(&schema_tags, &branches, &fallbacks)?;

    Ok(StatementInventory {
        owner: dispatch_path.to_string(),
        match_line: match_expr.span().start().line,
        schema_tags,
        branches,
        fallbacks,
    })
}

fn statement_schema_tags(file: &File) -> Result<Vec<String>, String> {
    let macros = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item) if item.mac.path.is_ident("instruction_schema") => {
                Some(&item.mac.tokens)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if macros.len() != 1 {
        return Err(format!(
            "expected one instruction_schema! invocation, found {}",
            macros.len()
        ));
    }

    let tokens = macros[0].clone().into_iter().collect::<Vec<_>>();
    let mut stmt_groups = Vec::new();
    for index in 0..tokens.len() {
        if !ident_is(tokens.get(index), "BuildStmtRow") {
            continue;
        }
        if !arrow_at(&tokens, index + 1) || !ident_is(tokens.get(index + 3), "StmtView") {
            return Err("BuildStmtRow context has an unrecognized view layout".into());
        }
        let kind = tokens.get(index + 4).and_then(group_tokens);
        if !kind.is_some_and(|kind: TokenStream| {
            let items = kind.into_iter().collect::<Vec<_>>();
            items.len() == 1 && ident_is(items.first(), "Stmt")
        }) {
            return Err("BuildStmtRow context is not [Stmt]".into());
        }
        let rows = tokens.get(index + 5).and_then(|token| match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                Some(group.stream())
            }
            _ => None,
        });
        let Some(rows) = rows else {
            return Err("BuildStmtRow context has no row block".into());
        };
        stmt_groups.push(rows);
    }
    if stmt_groups.len() != 1 {
        return Err(format!(
            "expected one BuildStmtRow => StmtView [Stmt] group, found {}",
            stmt_groups.len()
        ));
    }

    let mut tags = Vec::new();
    let mut row = Vec::new();
    for token in stmt_groups[0].clone() {
        if punct_is(&token, ';') {
            if row.is_empty() {
                return Err("empty row in BuildStmtRow context".into());
            }
            tags.push(parse_schema_row_tag(&row)?);
            row.clear();
        } else {
            row.push(token);
        }
    }
    if !row.is_empty() {
        return Err("unterminated row in BuildStmtRow context".into());
    }
    if tags.is_empty() {
        return Err("BuildStmtRow context has no instruction rows".into());
    }

    let mut seen = BTreeSet::new();
    for tag in &tags {
        if !tag.starts_with("Stmt") {
            return Err(format!("statement schema row has non-statement tag {tag}"));
        }
        if !seen.insert(tag.clone()) {
            return Err(format!("duplicate statement schema tag {tag}"));
        }
    }
    Ok(tags)
}

fn parse_schema_row_tag(row: &[TokenTree]) -> Result<String, String> {
    let mut index = 0;
    while index + 1 < row.len()
        && punct_is(&row[index], '#')
        && matches!(&row[index + 1], TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket)
    {
        index += 2;
    }
    if !matches!(row.get(index), Some(TokenTree::Ident(_))) {
        return Err("statement schema row has no variant identifier".into());
    }
    index += 1;
    if matches!(row.get(index), Some(TokenTree::Group(group)) if matches!(group.delimiter(), Delimiter::Brace | Delimiter::Parenthesis)) {
        index += 1;
    }
    if !arrow_at(row, index) {
        return Err("statement schema row has an unrecognized payload layout".into());
    }
    let tag = ident_value(row.get(index + 2))
        .ok_or_else(|| "statement schema row has no tag identifier".to_string())?;
    if !punct_is_at(row, index + 3, '=')
        || !matches!(row.get(index + 4), Some(TokenTree::Literal(_)))
        || !matches!(row.get(index + 5), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket)
        || index + 6 != row.len()
    {
        return Err(format!("statement schema row {tag} has an unrecognized route layout"));
    }
    Ok(tag)
}

fn find_function<'a>(file: &'a File, name: &str) -> Result<&'a Block, String> {
    let mut finder = FunctionFinder {
        name,
        blocks: Vec::new(),
    };
    finder.visit_file(file);
    if finder.blocks.len() != 1 {
        return Err(format!(
            "expected one {name} function, found {}",
            finder.blocks.len()
        ));
    }
    Ok(finder.blocks[0])
}

struct FunctionFinder<'ast, 'name> {
    name: &'name str,
    blocks: Vec<&'ast Block>,
}

impl<'ast, 'name> Visit<'ast> for FunctionFinder<'ast, 'name> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if item.sig.ident == self.name {
            self.blocks.push(&item.block);
        }
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if item.sig.ident == self.name {
            self.blocks.push(&item.block);
        }
        visit::visit_impl_item_fn(self, item);
    }
}

fn top_level_tag_match(block: &Block) -> Result<&syn::ExprMatch, String> {
    let matches = block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| match statement {
            Stmt::Expr(Expr::Match(expr), terminator) if path_is_ident(&expr.expr, "tag") => {
                Some((index, expr, terminator.is_none()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected one top-level match tag in eval_statement, found {}",
            matches.len()
        ));
    }
    let (index, match_expr, is_tail) = matches[0];
    if !is_tail || index + 1 != block.stmts.len() {
        return Err("eval_statement match tag must be the final tail expression".into());
    }
    Ok(match_expr)
}

fn parse_dispatch_arms(
    match_expr: &syn::ExprMatch,
) -> Result<(Vec<TagBranch>, Vec<FallbackBranch>), String> {
    let mut branches = Vec::new();
    let mut fallbacks = Vec::new();
    for (arm_index, arm) in match_expr.arms.iter().enumerate() {
        if arm.guard.is_some() {
            return Err(format!(
                "guarded eval_statement arm at line {} is unsupported",
                arm.pat.span().start().line
            ));
        }
        match &arm.pat {
            Pat::Wild(_) => {
                if is_frame_strategy_error_fallback(&arm.body) {
                    fallbacks.push(FallbackBranch {
                        pattern: "_".into(),
                        outcome: "frame_strategy_error".into(),
                        line: arm.pat.span().start().line,
                        arm_index,
                    });
                } else {
                    return Err(format!(
                        "wildcard eval_statement arm at line {} has an unrecognized fallback outcome",
                        arm.pat.span().start().line
                    ));
                }
            }
            pattern => {
                let mut tags = Vec::new();
                collect_exact_tag_patterns(pattern, &mut tags)?;
                if tags.is_empty() {
                    return Err(format!(
                        "unparsed eval_statement arm pattern at line {}",
                        pattern.span().start().line
                    ));
                }
                let pattern_text = tags
                    .iter()
                    .map(|(tag, _)| format!("FullTag::{tag}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                for (tag, line) in tags {
                    branches.push(TagBranch {
                        tag,
                        pattern: pattern_text.clone(),
                        line,
                        arm_index,
                    });
                }
            }
        }
    }
    if fallbacks.len() != 1 {
        return Err(format!(
            "expected one wildcard eval_statement fallback arm, found {}",
            fallbacks.len()
        ));
    }
    Ok((branches, fallbacks))
}

fn collect_exact_tag_patterns(pattern: &Pat, tags: &mut Vec<(String, usize)>) -> Result<(), String> {
    match pattern {
        Pat::Path(path) => {
            if path.qself.is_some() || path.path.segments.len() != 2 {
                return Err(format!(
                    "unparsed eval_statement path pattern at line {}",
                    pattern.span().start().line
                ));
            }
            let mut segments = path.path.segments.iter();
            if segments.next().is_none_or(|segment| segment.ident != "FullTag") {
                return Err(format!(
                    "eval_statement arm is not a FullTag pattern at line {}",
                    pattern.span().start().line
                ));
            }
            let tag = segments.next().expect("two path segments checked").ident.to_string();
            tags.push((tag, pattern.span().start().line));
            Ok(())
        }
        Pat::Or(patterns) => {
            for pattern in &patterns.cases {
                if matches!(pattern, Pat::Wild(_)) {
                    return Err("wildcard inside an or-pattern is unsupported".into());
                }
                collect_exact_tag_patterns(pattern, tags)?;
            }
            Ok(())
        }
        Pat::Paren(pattern) => collect_exact_tag_patterns(&pattern.pat, tags),
        _ => Err(format!(
            "unparsed eval_statement pattern at line {}",
            pattern.span().start().line
        )),
    }
}

fn validate_coverage(
    schema_tags: &[String],
    branches: &[TagBranch],
    fallbacks: &[FallbackBranch],
) -> Result<(), String> {
    let expected = schema_tags.iter().cloned().collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::<String, usize>::new();
    for branch in branches {
        if !expected.contains(&branch.tag) {
            return Err(format!("unknown eval_statement tag {}", branch.tag));
        }
        *counts.entry(branch.tag.clone()).or_default() += 1;
    }
    let duplicate = counts
        .iter()
        .filter_map(|(tag, count)| (*count != 1).then_some(tag.clone()))
        .collect::<Vec<_>>();
    if !duplicate.is_empty() {
        return Err(format!("duplicate eval_statement tags: {duplicate:?}"));
    }
    let missing = expected
        .iter()
        .filter(|tag| !counts.contains_key(*tag))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing eval_statement tags: {missing:?}"));
    }
    if let Some(fallback) = fallbacks.first()
        && branches.iter().any(|branch| branch.arm_index > fallback.arm_index)
    {
        return Err(format!(
            "wildcard fallback at arm {} precedes an exact statement arm",
            fallback.arm_index
        ));
    }
    Ok(())
}

fn path_is_ident(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Path(path) if path.qself.is_none() && path.path.is_ident(expected))
}

fn group_tokens(token: &TokenTree) -> Option<TokenStream> {
    match token {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => Some(group.stream()),
        _ => None,
    }
}

fn ident_value(token: Option<&TokenTree>) -> Option<String> {
    match token {
        Some(TokenTree::Ident(ident)) => Some(ident.to_string()),
        _ => None,
    }
}

fn ident_is(token: Option<&TokenTree>, expected: &str) -> bool {
    ident_value(token).as_deref() == Some(expected)
}

fn punct_is(token: &TokenTree, expected: char) -> bool {
    matches!(token, TokenTree::Punct(punct) if punct.as_char() == expected)
}

fn punct_is_at(tokens: &[TokenTree], index: usize, expected: char) -> bool {
    tokens.get(index).is_some_and(|token| punct_is(token, expected))
}

fn arrow_at(tokens: &[TokenTree], index: usize) -> bool {
    punct_is_at(tokens, index, '=') && punct_is_at(tokens, index + 1, '>')
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

impl EagerAtomInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self
            .schema_rows
            .iter()
            .map(|row| {
                let fields = row
                    .fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{{\"name\":{},\"type\":{}}}",
                            json_string(&field.name),
                            json_string(&field.ty),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"classification\":{},\"fields\":[{}],\"line\":{}}}",
                    json_string(&row.variant),
                    json_string(&row.tag),
                    row.ordinal,
                    json_string(&row.effects),
                    json_string(&row.class),
                    json_string(&row.strategy),
                    row.classification.as_deref().map(json_string).unwrap_or_else(|| "null".into()),
                    fields,
                    row.line,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let route = |route: &EagerAtomRoute| {
            format!(
                "{{\"mode\":{},\"line\":{},\"helper_line\":{},\"statement_index\":{},\"generic_line\":{},\"generic_statement_index\":{}}}",
                json_string(&route.mode),
                route.line,
                route.helper_line,
                route.statement_index,
                route.generic_line,
                route.generic_statement_index,
            )
        };
        let helper_arms = self
            .helper_arms
            .iter()
            .map(|arm| {
                format!(
                    "{{\"variant\":{},\"tag\":{},\"input\":{},\"transformation\":{},\"output\":{},\"line\":{},\"arm_index\":{}}}",
                    json_string(&arm.variant),
                    json_string(&arm.tag),
                    json_string(&arm.input),
                    json_string(&arm.transformation),
                    json_string(&arm.output),
                    arm.line,
                    arm.arm_index,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let fallback = |fallback: &EagerAtomFallback| {
            format!(
                "{{\"scope\":{},\"outcome\":{},\"source\":{},\"line\":{},\"arm_index\":{}}}",
                json_string(&fallback.scope),
                json_string(&fallback.outcome),
                json_string(&fallback.source),
                fallback.line,
                fallback.arm_index,
            )
        };
        let helper_boundaries = self
            .helper_boundaries
            .iter()
            .map(|boundary| {
                let references = boundary
                    .references
                    .iter()
                    .map(|reference| {
                        format!(
                            "{{\"source\":{},\"line\":{}}}",
                            json_string(&reference.source),
                            reference.line,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{{\"kind\":{},\"tags\":[{}],\"outcome\":{},\"references\":[{}]}}",
                    json_string(&boundary.kind),
                    boundary.tags.iter().map(|tag| json_string(tag)).collect::<Vec<_>>().join(","),
                    json_string(&boundary.outcome),
                    references,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let freeze_contract = format!(
            "{{\"line\":{},\"shared_threshold\":{},\"threshold_line\":{},\"list_take_line\":{},\"shared_list_line\":{}}}",
            self.freeze_contract.line,
            json_string(&self.freeze_contract.shared_threshold),
            self.freeze_contract.threshold_line,
            self.freeze_contract.list_take_line,
            self.freeze_contract.shared_list_line,
        );
        let value_contract = format!(
            "{{\"decode_error\":{},\"path_error\":{},\"slot_bounds\":{},\"freeze\":{}}}",
            json_string(&self.value_contract.decode_error),
            json_string(&self.value_contract.path_error),
            json_string(&self.value_contract.slot_bounds),
            json_string(&self.value_contract.freeze),
        );
        let source_paths = self
            .source_paths
            .iter()
            .map(|(name, path)| format!("{}:{}", json_string(name), json_string(path)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"classification_rules\":{{\"default_false_line\":{},\"eager_true_line\":{},\"expansion_line\":{}}},\"normal_route\":{},\"forced_route\":{},\"helper_arms\":[{helper_arms}],\"helper_fallback\":{},\"explicit_error_group\":{},\"helper_boundaries\":[{helper_boundaries}],\"slot_decode_line\":{},\"slot_decode_bounds_line\":{},\"freeze_contract\":{},\"value_contract\":{},\"runtime_error_paths\":{{\"status\":\"source_exhaustive_boundary_inventory\",\"note\":\"decode, path validation, slot bounds, and list freezing are source-classified; runtime conformance remains unproven\"}},\"source_paths\":{{{source_paths}}}}}",
            self.default_false_line,
            self.eager_true_line,
            self.expansion_line,
            route(&self.normal_route),
            route(&self.forced_route),
            fallback(&self.helper_fallback),
            fallback(&self.explicit_error_group),
            self.slot_decode_line,
            self.slot_decode_bounds_line,
            freeze_contract,
            value_contract,
        )
    }
}

impl FrameConstructorInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self
            .schema_rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"line\":{}}}",
                    json_string(&row.variant),
                    json_string(&row.tag),
                    row.ordinal,
                    json_string(&row.effects),
                    json_string(&row.class),
                    json_string(&row.strategy),
                    row.line,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let classifiers = self
            .classifiers
            .iter()
            .map(|(name, branches)| {
                let branches = branches
                    .iter()
                    .map(|branch| {
                        format!(
                            "{{\"tag\":{},\"pattern\":{},\"line\":{},\"arm_index\":{}}}",
                            json_string(&branch.tag),
                            json_string(&branch.pattern),
                            branch.line,
                            branch.arm_index,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}:[{}]", json_string(name), branches)
            })
            .collect::<Vec<_>>()
            .join(",");
        let route = |route: &FrameConstructorRoute| {
            format!(
                "{{\"mode\":{},\"line\":{},\"helper_line\":{},\"statement_index\":{},\"fallback_line\":{},\"fallback_statement_index\":{}}}",
                json_string(&route.mode),
                route.line,
                route.helper_line,
                route.statement_index,
                route.fallback_line,
                route.fallback_statement_index,
            )
        };
        let arms = |arms: &[FrameConstructorArm]| {
            arms.iter()
                .map(|arm| {
                    format!(
                        "{{\"helper\":{},\"variant\":{},\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
                        json_string(&arm.helper),
                        json_string(&arm.variant),
                        json_string(&arm.behavior),
                        arm.line,
                        arm.arm_index,
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let fallback = |fallback: &FrameConstructorFallback| {
            format!(
                "{{\"helper\":{},\"outcome\":{},\"scope\":{},\"line\":{},\"arm_index\":{}}}",
                json_string(&fallback.helper),
                json_string(&fallback.outcome),
                json_string(&fallback.scope),
                fallback.line,
                fallback.arm_index,
            )
        };
        let helper_fallbacks = self
            .helper_fallbacks
            .iter()
            .map(fallback)
            .collect::<Vec<_>>()
            .join(",");
        let value_boundaries = self
            .value_boundaries
            .iter()
            .map(|(variant, line)| format!("{}:{}", json_string(variant), line))
            .collect::<Vec<_>>()
            .join(",");
        let source_paths = self
            .source_paths
            .iter()
            .map(|(name, path)| format!("{}:{}", json_string(name), json_string(path)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"classifiers\":{{{classifiers}}},\"normal_route\":{},\"forced_route\":{},\"child_arms\":[{}],\"value_arms\":[{}],\"helper_fallbacks\":[{helper_fallbacks}],\"explicit_error_group\":{},\"value_decode_line\":{},\"value_boundaries\":{{{value_boundaries}}},\"runtime_error_paths\":{{\"status\":\"unreviewed\"}},\"source_paths\":{{{source_paths}}}}}",
            route(&self.normal_route),
            route(&self.forced_route),
            arms(&self.child_arms),
            arms(&self.value_arms),
            fallback(&self.explicit_error_group),
            self.value_decode_line,
        )
    }
}

impl CollectionConstructionInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self.schema_rows.iter().map(|row| format!(
            "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"line\":{}}}",
            json_string(&row.variant), json_string(&row.tag), row.ordinal,
            json_string(&row.effects), json_string(&row.class),
            json_string(&row.strategy), row.line,
        )).collect::<Vec<_>>().join(",");
        let classifier_tags = self.frame_apply_classifier_tags.iter()
            .map(|tag| json_string(tag)).collect::<Vec<_>>().join(",");
        let arms = |items: &[CollectionDispatchArm]| items.iter().map(|arm| format!(
            "{{\"tag\":{},\"variant\":{},\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&arm.tag), json_string(&arm.variant), json_string(&arm.behavior), arm.line, arm.arm_index,
        )).collect::<Vec<_>>().join(",");
        let fallback = |item: &CollectionFallback| format!(
            "{{\"helper\":{},\"outcome\":{},\"scope\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&item.helper), json_string(&item.outcome), json_string(&item.scope), item.line, item.arm_index,
        );
        let helper_fallbacks = self.helper_fallbacks.iter().map(fallback).collect::<Vec<_>>().join(",");
        let normal_view_fallbacks = self.normal_view_fallbacks.iter().map(fallback).collect::<Vec<_>>().join(",");
        let forced_route = format!(
            "{{\"frame_apply_line\":{},\"frame_apply_statement_index\":{},\"view_kind_line\":{},\"view_kind_statement_index\":{},\"eval_expr_view_line\":{}}}",
            self.forced_route.frame_apply_line,
            self.forced_route.frame_apply_statement_index,
            self.forced_route.view_kind_line,
            self.forced_route.view_kind_statement_index,
            self.forced_route.eval_expr_view_line,
        );
        let source_paths = self.source_paths.iter().map(|(name, path)| format!(
            "{}:{}", json_string(name), json_string(path),
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"frame_apply_classifier\":{{\"line\":{},\"tags\":[{classifier_tags}]}},\"normal_arms\":[{}],\"forced_arms\":[{}],\"forced_route\":{forced_route},\"normal_fallback\":{},\"normal_view_fallbacks\":[{normal_view_fallbacks}],\"helper_fallbacks\":[{helper_fallbacks}],\"explicit_error_group\":{},\"runtime_error_paths\":{{\"status\":\"unreviewed\",\"note\":\"typed-view mismatch guards are recorded; decoding, child evaluation, and row-specific construction errors remain unreviewed\"}},\"source_paths\":{{{source_paths}}}}}",
            self.frame_apply_classifier_line,
            arms(&self.normal_arms), arms(&self.forced_arms), fallback(&self.normal_fallback),
            fallback(&self.explicit_error_group),
        )
    }
}

impl ResultConstructorInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self.schema_rows.iter().map(|row| format!(
            "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"line\":{}}}",
            json_string(&row.variant), json_string(&row.tag), row.ordinal,
            json_string(&row.effects), json_string(&row.class),
            json_string(&row.strategy), row.line,
        )).collect::<Vec<_>>().join(",");
        let arms = |items: &[ResultConstructorArm]| items.iter().map(|arm| format!(
            "{{\"tag\":{},\"variant\":{},\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&arm.tag), json_string(&arm.variant), json_string(&arm.behavior), arm.line, arm.arm_index,
        )).collect::<Vec<_>>().join(",");
        let fallback = |item: &ResultConstructorFallback| format!(
            "{{\"helper\":{},\"outcome\":{},\"scope\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&item.helper), json_string(&item.outcome), json_string(&item.scope), item.line, item.arm_index,
        );
        let fallbacks = |items: &[ResultConstructorFallback]| items.iter().map(fallback).collect::<Vec<_>>().join(",");
        let forced_route = format!(
            "{{\"frame_apply_line\":{},\"frame_apply_statement_index\":{},\"view_kind_line\":{},\"view_kind_statement_index\":{},\"eval_expr_view_line\":{}}}",
            self.forced_route.frame_apply_line,
            self.forced_route.frame_apply_statement_index,
            self.forced_route.view_kind_line,
            self.forced_route.view_kind_statement_index,
            self.forced_route.eval_expr_view_line,
        );
        let source_paths = self.source_paths.iter().map(|(name, path)| format!(
            "{}:{}", json_string(name), json_string(path),
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"normal_arms\":[{}],\"normal_view_fallbacks\":[{}],\"normal_fallback\":{},\"forced_arms\":[{}],\"forced_view_fallbacks\":[{}],\"forced_route\":{forced_route},\"explicit_error_group\":{},\"runtime_error_paths\":{{\"status\":\"unreviewed\",\"note\":\"typed-view mismatch guards are recorded; decoding, child evaluation, and row-specific result construction errors remain unreviewed\"}},\"source_paths\":{{{source_paths}}}}}",
            arms(&self.normal_arms), fallbacks(&self.normal_view_fallbacks), fallback(&self.normal_fallback),
            arms(&self.forced_arms), fallbacks(&self.forced_view_fallbacks), fallback(&self.explicit_error_group),
        )
    }
}

impl CallFrameInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self.schema_rows.iter().map(|row| format!(
            "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"line\":{}}}",
            json_string(&row.variant), json_string(&row.tag), row.ordinal,
            json_string(&row.effects), json_string(&row.class), json_string(&row.strategy), row.line,
        )).collect::<Vec<_>>().join(",");
        let arms = self.forced_arms.iter().map(|arm| {
            let group = arm.view_group.iter().map(|variant| json_string(variant)).collect::<Vec<_>>().join(",");
            format!(
                "{{\"tag\":{},\"variant\":{},\"view_group\":[{}],\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
                json_string(&arm.tag), json_string(&arm.variant), group,
                json_string(&arm.behavior), arm.line, arm.arm_index,
            )
        }).collect::<Vec<_>>().join(",");
        let fallback = |item: &CallFrameFallback| format!(
            "{{\"helper\":{},\"outcome\":{},\"scope\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&item.helper), json_string(&item.outcome), json_string(&item.scope), item.line, item.arm_index,
        );
        let fallbacks = self.typed_view_fallbacks.iter().map(fallback).collect::<Vec<_>>().join(",");
        let route = format!(
            "{{\"scheduled_predicate_line\":{},\"frame_schedule_line\":{},\"frame_helper_line\":{},\"inner_helper_line\":{},\"view_kind_line\":{},\"eval_expr_view_line\":{}}}",
            self.normal_route.scheduled_predicate_line,
            self.normal_route.frame_schedule_line,
            self.normal_route.frame_helper_line,
            self.normal_route.inner_helper_line,
            self.normal_route.view_kind_line,
            self.normal_route.eval_expr_view_line,
        );
        let source_paths = self.source_paths.iter().map(|(name, path)| format!(
            "{}:{}", json_string(name), json_string(path),
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"normal_route\":{route},\"normal_fallback\":{},\"forced_arms\":[{arms}],\"typed_view_fallbacks\":[{fallbacks}],\"explicit_error_group\":{},\"runtime_error_paths\":{{\"status\":\"unreviewed\",\"note\":\"call decode, callee resolution, argument evaluation, and call outcome errors remain unreviewed\"}},\"source_paths\":{{{source_paths}}}}}",
            fallback(&self.normal_fallback), fallback(&self.explicit_error_group),
        )
    }
}

impl FieldIndexInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self.schema_rows.iter().map(|row| format!(
            "{{\"variant\":{},\"tag\":{},\"ordinal\":{},\"effects\":{},\"class\":{},\"strategy\":{},\"line\":{}}}",
            json_string(&row.variant), json_string(&row.tag), row.ordinal,
            json_string(&row.effects), json_string(&row.class), json_string(&row.strategy), row.line,
        )).collect::<Vec<_>>().join(",");
        let arms = |items: &[FieldIndexArm]| items.iter().map(|arm| format!(
            "{{\"tag\":{},\"variant\":{},\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&arm.tag), json_string(&arm.variant), json_string(&arm.behavior), arm.line, arm.arm_index,
        )).collect::<Vec<_>>().join(",");
        let fallback = |item: &FieldIndexFallback| format!(
            "{{\"helper\":{},\"outcome\":{},\"scope\":{},\"line\":{},\"arm_index\":{}}}",
            json_string(&item.helper), json_string(&item.outcome), json_string(&item.scope), item.line, item.arm_index,
        );
        let fallbacks = |items: &[FieldIndexFallback]| items.iter().map(fallback).collect::<Vec<_>>().join(",");
        let route = format!(
            "{{\"frame_apply_line\":{},\"frame_apply_statement_index\":{},\"view_kind_line\":{},\"view_kind_statement_index\":{},\"eval_expr_view_line\":{}}}",
            self.forced_route.frame_apply_line,
            self.forced_route.frame_apply_statement_index,
            self.forced_route.view_kind_line,
            self.forced_route.view_kind_statement_index,
            self.forced_route.eval_expr_view_line,
        );
        let normal_route = format!(
            "{{\"scheduled_predicate_line\":{},\"frame_schedule_line\":{},\"frame_helper_line\":{},\"inner_helper_line\":{}}}",
            self.normal_route.scheduled_predicate_line,
            self.normal_route.frame_schedule_line,
            self.normal_route.frame_helper_line,
            self.normal_route.inner_helper_line,
        );
        let source_paths = self.source_paths.iter().map(|(name, path)| format!(
            "{}:{}", json_string(name), json_string(path),
        )).collect::<Vec<_>>().join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"normal_route\":{normal_route},\"frame_apply_classifier_line\":{},\"normal_arms\":[{}],\"normal_view_fallbacks\":[{}],\"normal_fallback\":{},\"forced_arms\":[{}],\"forced_view_fallbacks\":[{}],\"forced_route\":{route},\"explicit_error_group\":{},\"runtime_error_paths\":{{\"status\":\"unreviewed\",\"note\":\"field/index decode, child evaluation outcomes, and runtime field/index lookup errors remain unreviewed\"}},\"source_paths\":{{{source_paths}}}}}",
            self.frame_apply_classifier_line,
            arms(&self.normal_arms), fallbacks(&self.normal_view_fallbacks), fallback(&self.normal_fallback),
            arms(&self.forced_arms), fallbacks(&self.forced_view_fallbacks), fallback(&self.explicit_error_group),
        )
    }
}

impl TypedScalarInventory {
    pub fn to_json(&self) -> String {
        let schema_rows = self
            .schema_rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"family\":{},\"kind\":{},\"view\":{},\"variant\":{},\"tag\":{},\"ordinal\":{},\"line\":{}}}",
                    json_string(&row.family),
                    json_string(&row.kind),
                    json_string(&row.view),
                    json_string(&row.variant),
                    json_string(&row.tag),
                    row.ordinal,
                    row.line,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let classifiers = self
            .classifiers
            .iter()
            .map(|(family, branches)| {
                let branches = branches
                    .iter()
                    .map(|branch| {
                        format!(
                            "{{\"tag\":{},\"pattern\":{},\"line\":{},\"arm_index\":{}}}",
                            json_string(&branch.tag),
                            json_string(&branch.pattern),
                            branch.line,
                            branch.arm_index,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}:[{}]", json_string(family), branches)
            })
            .collect::<Vec<_>>()
            .join(",");
        let normal_checks = self
            .normal_checks
            .iter()
            .map(|check| {
                format!(
                    "{{\"family\":{},\"line\":{},\"statement_index\":{},\"wrapper\":{},\"result_variant\":{}}}",
                    json_string(&check.family),
                    check.line,
                    check.statement_index,
                    json_string(&check.wrapper),
                    json_string(&check.result_variant),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let forced_entry = format!(
            "{{\"family\":{},\"line\":{},\"statement_index\":{},\"wrapper\":{},\"result_variant\":{}}}",
            json_string(&self.forced_entry.family),
            self.forced_entry.line,
            self.forced_entry.statement_index,
            json_string(&self.forced_entry.wrapper),
            json_string(&self.forced_entry.result_variant),
        );
        let view_arms = self
            .view_arms
            .iter()
            .map(|arm| {
                format!(
                    "{{\"family\":{},\"variant\":{},\"line\":{},\"arm_index\":{}}}",
                    json_string(&arm.family),
                    json_string(&arm.variant),
                    arm.line,
                    arm.arm_index,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let frame_groups = self
            .frame_groups
            .iter()
            .map(|group| {
                format!(
                    "{{\"tags\":[{}],\"behavior\":{},\"line\":{},\"arm_index\":{}}}",
                    group.tags.iter().map(|tag| json_string(tag)).collect::<Vec<_>>().join(","),
                    json_string(&group.behavior),
                    group.line,
                    group.arm_index,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let fallbacks = self
            .fallbacks
            .iter()
            .map(|fallback| {
                format!(
                    "{{\"applies_to\":[{}],\"scope\":{},\"outcome\":{},\"source\":{},\"line\":{},\"arm_index\":{}}}",
                    fallback.applies_to.iter().map(|tag| json_string(tag)).collect::<Vec<_>>().join(","),
                    json_string(&fallback.scope),
                    json_string(&fallback.outcome),
                    json_string(&fallback.source),
                    fallback.line,
                    fallback.arm_index,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let wrong_union_errors = self
            .wrong_union_errors
            .iter()
            .map(|error| {
                format!(
                    "{{\"family\":{},\"outcome\":{},\"line\":{}}}",
                    json_string(&error.family),
                    json_string(&error.outcome),
                    error.line,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let source_paths = self
            .source_paths
            .iter()
            .map(|(name, path)| format!("{}:{}", json_string(name), json_string(path)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema_rows\":[{schema_rows}],\"classifiers\":{{{classifiers}}},\"normal_checks\":[{normal_checks}],\"forced_entry\":{forced_entry},\"view_arms\":[{view_arms}],\"frame_groups\":[{frame_groups}],\"fallbacks\":[{fallbacks}],\"wrong_union_errors\":[{wrong_union_errors}],\"source_paths\":{{{source_paths}}}}}"
        )
    }
}

impl StatementInventory {
    pub fn to_json(&self) -> String {
        let tags = self
            .schema_tags
            .iter()
            .map(|tag| json_string(tag))
            .collect::<Vec<_>>()
            .join(",");
        let branches = self
            .branches
            .iter()
            .map(|branch| {
                format!(
                    "{{\"tag\":{},\"pattern\":{},\"line\":{},\"arm_index\":{}}}",
                    json_string(&branch.tag),
                    json_string(&branch.pattern),
                    branch.line,
                    branch.arm_index
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let fallbacks = self
            .fallbacks
            .iter()
            .map(|fallback| {
                format!(
                    "{{\"pattern\":{},\"outcome\":{},\"line\":{},\"arm_index\":{}}}",
                    json_string(&fallback.pattern),
                    json_string(&fallback.outcome),
                    fallback.line,
                    fallback.arm_index
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"owner\":{},\"match_line\":{},\"schema_tags\":[{}],\"branches\":[{}],\"fallbacks\":[{}]}}",
            json_string(&self.owner),
            self.match_line,
            tags,
            branches,
            fallbacks
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"
instruction_schema! {
    BuildStmtRow => StmtView [Stmt] {
        First => StmtFirst = 0 [0; Scheduled; FrameControl];
        Second { value: BuildExprId } => StmtSecond = 1 [0; Scheduled; FrameControl];
    }
}

"#;

    const DISPATCH: &str = r#"
struct Evaluator;
impl Evaluator {
    fn eval_statement(&mut self) {
        match tag {
            FullTag::StmtFirst | FullTag::StmtSecond => work(),
            _ => Err(frame_strategy_error(tag, span)),
        }
    }
}
"#;

    fn inventory(schema: &str, dispatch: &str) -> Result<StatementInventory, String> {
        extract_statement_inventory(schema, dispatch, "explicit_run.rs")
    }

    const INT_ROWS: [(&str, &str); 6] = [
        ("Int", "IntInt"),
        ("Slot", "IntSlot"),
        ("Binary", "IntBinary"),
        ("StrByteLenSlot", "IntStrByteLenSlot"),
        ("StrCountLinesSlot", "IntStrCountLinesSlot"),
        ("StrByteAtSlot", "IntStrByteAtSlot"),
    ];

    const BOOL_ROWS: [(&str, &str); 12] = [
        ("Bool", "BoolBool"),
        ("Slot", "BoolSlot"),
        ("Not", "BoolNot"),
        ("And", "BoolAnd"),
        ("Or", "BoolOr"),
        ("IntCompare", "BoolIntCompare"),
        ("StrPredicateSlot", "BoolStrPredicateSlot"),
        ("ContainsSlot", "BoolContainsSlot"),
        ("StrContainsSlot", "BoolStrContainsSlot"),
        ("TrimEmptySlot", "BoolTrimEmptySlot"),
        ("TrimStrPredicateSlot", "BoolTrimStrPredicateSlot"),
        ("LiteralCompareSlot", "BoolLiteralCompareSlot"),
    ];

    fn typed_fixture() -> (String, String, String, String) {
        let rows = INT_ROWS
            .iter()
            .enumerate()
            .map(|(ordinal, (variant, tag))| {
                format!("{variant} => {tag} = {ordinal} [0; Operands; FrameApply];")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let bool_rows = BOOL_ROWS
            .iter()
            .enumerate()
            .map(|(index, (variant, tag))| {
                format!("{variant} => {tag} = {} [0; Operands; FrameApply];", index + 6)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let schema = format!(
            "instruction_schema! {{\n BuildIntRow => IntView [Int] {{ {rows} }}\n BuildBoolRow => BoolView [Bool] {{ {bool_rows} }}\n}}"
        );
        let int_tags = INT_ROWS
            .iter()
            .map(|(_, tag)| format!("FullTag::{tag}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let bool_tags = BOOL_ROWS
            .iter()
            .map(|(_, tag)| format!("FullTag::{tag}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let indexed = format!(
            r#"
fn is_typed_int_tag(tag: FullTag) -> bool {{ matches!(tag, {int_tags}) }}
fn is_typed_bool_tag(tag: FullTag) -> bool {{ matches!(tag, {bool_tags}) }}
struct Evaluator;
impl Evaluator {{
 fn eval_indexed_expr_inner(&mut self) {{
  let (tag, payload) = indexed_value();
  if is_typed_int_tag(tag) {{ return self.eval_indexed_typed_int(a,b,c,d).map(|flow| flow.map_continue(LoweredValue::Int)); }}
  if is_typed_bool_tag(tag) {{ return self.eval_indexed_typed_bool(a,b,c,d).map(|flow| flow.map_continue(LoweredValue::Bool)); }}
  if tag.is_eager_atom() {{ }}
  if indexed_frame_apply_tag(tag) {{ }}
 }}
}}
"#
        );
        let int_views = INT_ROWS
            .iter()
            .map(|(variant, _)| format!("IntView::{variant}(..) => {{}},"))
            .collect::<Vec<_>>()
            .join("\n");
        let bool_views = BOOL_ROWS
            .iter()
            .map(|(variant, _)| format!("BoolView::{variant}(..) => {{}},"))
            .collect::<Vec<_>>()
            .join("\n");
        let values = format!(
            r#"
fn eval_indexed_typed_int() {{ match eval_indexed_typed_scalar() {{
 TypedScalarValue::Int(value) => Ok(value),
 TypedScalarValue::Bool(_) => Err(RuntimeError::new("indexed-ir", "typed integer evaluator produced Bool").with_span(call_span)),
}} }}
fn eval_indexed_typed_bool() {{ match eval_indexed_typed_scalar() {{
 TypedScalarValue::Bool(value) => Ok(value),
 TypedScalarValue::Int(_) => Err(RuntimeError::new("indexed-ir", "typed boolean evaluator produced Int").with_span(call_span)),
}} }}
fn eval_indexed_typed_scalar() {{
 match task {{ TypedScalarWork::Int(_) => {{ match view {{ {int_views} }} }},
 TypedScalarWork::Bool(_) => {{ match view {{ {bool_views} }} }} }}
}}
"#
        );
        let explicit = r#"
fn eval_expr(&mut self) {
 if tag.is_eager_atom() {}
 if strategy == FrameStrategy::FrameApply && (super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag)) { return self.eval_typed_scalar_expr(a,b,c,d,e,f,g,h); }
 if super::indexed_frame_apply_tag(tag) {}
}
fn eval_typed_scalar_expr(&mut self) {
 match tag {
  FullTag::IntBinary | FullTag::IntStrByteAtSlot => match IntView::decode() {
   IntView::Binary { .. } => Some(()),
   IntView::StrByteAtSlot { .. } => Some(()),
   _ => return Err(frame_strategy_error(tag, span)),
  },
  FullTag::BoolNot | FullTag::BoolAnd | FullTag::BoolOr | FullTag::BoolIntCompare => match BoolView::decode() {
   BoolView::Not(_) => Some(()),
   BoolView::And(..) | BoolView::Or(..) => Some(()),
   BoolView::IntCompare { .. } => Some(()),
   _ => return Err(frame_strategy_error(tag, span)),
  },
  tag if super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag) => None,
  _ => return Err(frame_strategy_error(tag, span)),
 }
 match operation {
  Some(_) => {},
  None => {
   let call = &mut self.calls[index];
   if !call.execution.is_in_place(instruction) { return Err(frame_strategy_error(tag, span)); }
   if super::is_typed_int_tag(tag) { self.evaluator.eval_indexed_typed_int(); }
   else { self.evaluator.eval_indexed_typed_bool(); }
  }
 }
}
"#;
        (schema, indexed, values, explicit.into())
    }

    fn typed_inventory(
        sources: &(String, String, String, String),
    ) -> Result<TypedScalarInventory, String> {
        extract_typed_scalar_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            &sources.3,
            "schema.rs",
            "indexed_run.rs",
            "values.rs",
            "explicit_run.rs",
        )
    }

    fn eager_fixture() -> (String, String, String) {
        let rows = EAGER_ATOM_ROWS
            .iter()
            .map(|(variant, tag, ordinal)| {
                let payload = match *variant {
                    "Null" | "Unit" => String::new(),
                    "Int" => "(value: i64)".into(),
                    "Float" => "(value: FloatValue)".into(),
                    "Duration" => "(value: DurationValue)".into(),
                    "Bool" => "(value: bool)".into(),
                    "Str" => "(value: Arc<str>)".into(),
                    "Bytes" => "(value: Arc<[u8]>)".into(),
                    "PreparedRegex" => "(value: RegexValue)".into(),
                    "PreparedConstant" => "(value: PreparedConstantValue)".into(),
                    "Path" => "(value: PathValue)".into(),
                    "Param" => "(slot: usize)".into(),
                    _ => unreachable!("closed EagerAtom rows"),
                };
                format!("{variant}{payload} => {tag} = {ordinal} [0; Operands; FrameApply; EagerAtom];")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let schema = format!(
            r#"
macro_rules! instruction_schema {{
    (@eager_atom) => {{ false }};
    (@eager_atom EagerAtom) => {{ true }};
    (
        $($row:ident => $view:ident [$kind:ident] {{
            $($variant:ident => $tag:ident = $ordinal:literal [$effects:expr; $class:ident; $strategy:ident $(; $classification:ident)?];)*
        }})*
    ) => {{
        fn is_eager_atom(self) -> bool {{
            match self {{ $($(Self::$tag => instruction_schema!(@eager_atom $($classification)?),)*)* }}
        }}
    }};
}}
instruction_schema! {{ BuildExprRow => ExprView [Expr] {{ {rows} }} }}
"#
        );
        let helper_arms = EAGER_VIEW_VARIANTS
            .iter()
            .map(|variant| match *variant {
                "Null" => "ExprView::Null => LoweredValue::Null,".to_string(),
                "Unit" => "ExprView::Unit => LoweredValue::Unit,".to_string(),
                "Int" => "ExprView::Int(value) => LoweredValue::Int(value),".to_string(),
                "Float" => "ExprView::Float(value) => LoweredValue::Float(value),".to_string(),
                "Duration" => "ExprView::Duration(value) => LoweredValue::Duration(value),".to_string(),
                "Bool" => "ExprView::Bool(value) => LoweredValue::Bool(value),".to_string(),
                "Str" => "ExprView::Str(value) => LoweredValue::Str(Arc::from(value)),".to_string(),
                "Bytes" => "ExprView::Bytes(value) => LoweredValue::Bytes(Arc::from(value)),".to_string(),
                "PreparedRegex" => "ExprView::PreparedRegex(value) => LoweredValue::Regex(Box::new(value.clone())),".to_string(),
                "PreparedConstant" => "ExprView::PreparedConstant(value) => value.0.clone(),".to_string(),
                "Path" => "ExprView::Path(value) => LoweredValue::Path(PathValue::new(value.to_vec()).map_err(|error| indexed_error(IrVerifyError { message: error.message }, span))?),".to_string(),
                "Param" => "ExprView::Param(slot) => { lowered_freeze_large_slot_list(&mut slots[slot]); slots[slot].clone() },".to_string(),
                _ => unreachable!("closed EagerAtom fixture"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let indexed = format!(
            r#"
fn indexed_eager_atom_value(execution: &E, tag: FullTag, payload: P, slots: &mut [V], span: Span) -> Result<V, E> {{
    let view = ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?;
    let value = match view {{
        {helper_arms}
        _ => unreachable!("only eager atom tags reach this value helper"),
    }};
    Ok(value)
}}
fn eval_indexed_expr_inner() {{
    let (tag, payload) = indexed_value();
    if tag.is_eager_atom() {{
        return Ok(ControlFlow::Continue(indexed_eager_atom_value(execution, tag, payload, slots, call_span)?));
    }}
    if indexed_frame_apply_tag(tag) {{}}
}}
"#
        );
        let eager_kinds = EAGER_VIEW_VARIANTS
            .iter()
            .map(|variant| format!("ExprViewKind::{variant}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let explicit = format!(
            r#"
fn eval_expr(&mut self) {{
    if tag.is_eager_atom() {{
        let call = &mut self.calls[index];
        let value = super::indexed_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)?;
        self.push_value(index, FrameValue::Value(value), next);
        return Ok(());
    }}
    if super::indexed_frame_apply_tag(tag) {{}}
}}
fn eval_expr_view(&mut self) {{
    match view_kind {{
        {eager_kinds} => {{ return Err(frame_strategy_error(tag, span)); }}
    }}
}}
"#
        );
        (schema, indexed, explicit)
    }

    fn eager_inventory(sources: &(String, String, String)) -> Result<EagerAtomInventory, String> {
        let (slot_codec, freeze) = eager_auxiliary_fixture();
        extract_eager_atom_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            &slot_codec,
            &freeze,
            "schema.rs",
            "indexed_run.rs",
            "explicit_run.rs",
            "full.rs",
            "lowered_run.rs",
        )
    }

    fn eager_inventory_with_aux(
        sources: &(String, String, String),
        slot_codec: &str,
        freeze: &str,
    ) -> Result<EagerAtomInventory, String> {
        extract_eager_atom_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            slot_codec,
            freeze,
            "schema.rs",
            "indexed_run.rs",
            "explicit_run.rs",
            "full.rs",
            "lowered_run.rs",
        )
    }

    fn eager_auxiliary_fixture() -> (String, String) {
        let slot_codec = r#"
impl FullCodec for usize {
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let slot = input.raw()? as usize;
        if slot >= decoder.slot_count as usize {
            return Err(IrVerifyError::new("slot is out of bounds"));
        }
        Ok(slot)
    }
}
"#;
        let freeze = r#"
const LOWERED_SHARED_LIST_THRESHOLD: usize = 16;
fn lowered_freeze_large_slot_list(slot: &mut LoweredValue) {
    let LoweredValue::List(items) = slot else { return; };
    if items.len() < LOWERED_SHARED_LIST_THRESHOLD { return; }
    let items = std::mem::take(items);
    *slot = LoweredValue::SharedList(Arc::new(items));
}
"#;
        (slot_codec.into(), freeze.into())
    }

    fn frame_constructor_fixture() -> (String, String, String) {
        let schema = r#"
instruction_schema! {
    BuildExprRow => ExprView [Expr] {
        FunctionRef { function: FunctionName, pure: bool } => ExprFunctionRef = 29 [0; Scheduled; FrameApply];
        PathFrom { value: BuildExprId, span: Span } => ExprPathFrom = 30 [0; Scheduled; FrameApply];
        EmptyMap => ExprEmptyMap = 49 [0; Scheduled; FrameApply];
    }
}
"#;
        let frame_apply = FRAME_APPLY_TAGS
            .iter()
            .map(|tag| format!("FullTag::{tag}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let prepared = FRAME_PREPARED_OPERAND_TAGS
            .iter()
            .map(|tag| format!("FullTag::{tag}"))
            .collect::<Vec<_>>()
            .join(" | ");
        let indexed = format!(
            r#"
fn indexed_frame_apply_tag(tag: FullTag) -> bool {{ matches!(tag, {frame_apply}) }}
fn indexed_frame_apply_prepares_operands(tag: FullTag) -> bool {{ matches!(tag, {prepared}) }}
fn eval_indexed_expr_inner() {{
    if indexed_frame_apply_tag(tag) {{
        return self.eval_indexed_expr_with_frames(execution, instruction, slots, call_span);
    }}
    let result = match tag {{ _ => Ok(()) }};
    result
}}
fn indexed_frame_apply_child(tag: FullTag, view: &mut ExprView, position: usize, tag_position: Option<usize>, error_position: Option<usize>, span: Span) -> Result<Option<Child>, E> {{
    let child = match view {{
        ExprView::FunctionRef {{ .. }} | ExprView::EmptyMap => None,
        ExprView::PathFrom {{ value, span }} => indexed_frame_apply_child_from_fields([(Some((value.raw(), None)))], position, *span),
        _ => unreachable!("frame apply tag has a matching typed view"),
    }};
    Ok(child)
}}
fn indexed_frame_apply_value(execution: Execution, tag: FullTag, payload: Payload) -> Result<Value, E> {{
    let view = ExprView::decode(execution, tag, payload)?;
    let view = match view {{ ExprView::Abort {{ .. }} => view, view => view }};
    match view {{
        ExprView::FunctionRef {{ function, pure }} => indexed_function_ref_value(function, pure),
        ExprView::PathFrom {{ span, .. }} => indexed_path_from_value(value, span),
        ExprView::EmptyMap => Ok(indexed_empty_map_value()),
        _ => unreachable!("frame apply tag has a matching typed view"),
    }}
}}
"#
        );
        let explicit = r#"
fn eval_expr(&mut self) {
    if super::indexed_frame_apply_tag(tag) {
        let mut view = ExprView::decode(&execution, tag, payload)?;
        let first = super::indexed_frame_apply_child(tag, &mut view, 0, None, None, span)?;
        let apply_span = first.as_ref().map_or(span, |child| child.span);
        self.push_apply_operands(index, instruction, tag, first, apply_span, next)?;
        return Ok(());
    }
    let view_kind = tag.expr_view_kind().ok_or_else(|| frame_strategy_error(tag, span))?;
    self.eval_expr_view(view_kind)
}
fn eval_expr_view(&mut self, view_kind: ExprViewKind) -> Result<(), E> {
    match view_kind {
        ExprViewKind::FunctionRef | ExprViewKind::PathFrom | ExprViewKind::EmptyMap => {
            return Err(frame_strategy_error(tag, span));
        }
    }
}
"#;
        (schema.into(), indexed, explicit.into())
    }

    fn frame_constructor_inventory(
        sources: &(String, String, String),
    ) -> Result<FrameConstructorInventory, String> {
        extract_frame_constructor_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            "schema.rs",
            "indexed_run.rs",
            "explicit_run.rs",
        )
    }

    #[test]
    fn frame_constructor_inventory_maps_schema_routes_helpers_and_fallbacks() {
        let sources = frame_constructor_fixture();
        let inventory = frame_constructor_inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 3);
        assert_eq!(inventory.classifiers["FrameApply"].len(), FRAME_APPLY_TAGS.len());
        assert_eq!(inventory.child_arms.len(), 3);
        assert_eq!(inventory.value_arms.len(), 3);
        assert_eq!(inventory.helper_fallbacks.len(), 2);
        assert_eq!(inventory.normal_route.mode, "normal");
        assert_eq!(inventory.forced_route.mode, "forced");
        assert!(inventory.normal_route.statement_index < inventory.normal_route.fallback_statement_index);
        assert!(inventory.forced_route.statement_index < inventory.forced_route.fallback_statement_index);
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
        assert!(inventory.to_json().contains("\"runtime_error_paths\":{\"status\":\"unreviewed\""));
    }

    #[test]
    fn frame_constructor_schema_add_remove_retag_duplicate_and_metadata_drift_fail_closed() {
        let sources = frame_constructor_fixture();
        let removed = sources.0.replace(
            "PathFrom { value: BuildExprId, span: Span } => ExprPathFrom = 30 [0; Scheduled; FrameApply];",
            "",
        );
        assert!(frame_constructor_inventory(&(removed, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema cohort drift"));

        let retagged = sources.0.replace("ExprPathFrom = 30", "ExprOther = 30");
        assert!(frame_constructor_inventory(&(retagged, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema cohort drift"));

        let wrong_metadata = sources.0.replace("Scheduled; FrameApply", "Operands; FrameControl");
        assert!(frame_constructor_inventory(&(wrong_metadata, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("must be pure Scheduled; FrameApply"));

        let duplicate = sources.0.replace(
            "EmptyMap => ExprEmptyMap = 49 [0; Scheduled; FrameApply];",
            "EmptyMap => ExprEmptyMap = 49 [0; Scheduled; FrameApply]; EmptyMap => ExprEmptyMap = 50 [0; Scheduled; FrameApply];",
        );
        assert!(frame_constructor_inventory(&(duplicate, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("duplicate Expr schema tag"));

        let added_duplicate = sources.0.replace(
            "EmptyMap => ExprEmptyMap = 49 [0; Scheduled; FrameApply];",
            "EmptyMap => ExprEmptyMap = 49 [0; Scheduled; FrameApply]; PathFrom => ExprPathFrom = 50 [0; Scheduled; FrameApply];",
        );
        assert!(frame_constructor_inventory(&(added_duplicate, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("duplicate Expr schema tag"));
    }

    #[test]
    fn frame_constructor_classifier_route_and_helper_drift_fail_closed() {
        let sources = frame_constructor_fixture();
        let missing_classifier = sources.1.replace("FullTag::ExprPathFrom | ", "");
        assert!(frame_constructor_inventory(&(sources.0.clone(), missing_classifier, sources.2.clone()))
            .unwrap_err().contains("missing"));

        let duplicate_classifier = sources.1.replace(
            "FullTag::ExprFunctionRef |",
            "FullTag::ExprFunctionRef | FullTag::ExprFunctionRef |",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), duplicate_classifier, sources.2.clone()))
            .unwrap_err().contains("duplicate"));

        let wrong_normal = sources.1.replace(
            "self.eval_indexed_expr_with_frames(execution, instruction, slots, call_span)",
            "self.eval_indexed_expr(execution, instruction, slots, call_span)",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), wrong_normal, sources.2.clone()))
            .unwrap_err().contains("unexpected frame call"));

        let missing_explicit = sources.2.replace("super::indexed_frame_apply_tag(tag)", "false");
        assert!(frame_constructor_inventory(&(sources.0.clone(), sources.1.clone(), missing_explicit))
            .unwrap_err().contains("one FrameApply constructor branch"));

        let conditional_child = sources.2.replace(
            "let first = super::indexed_frame_apply_child(tag, &mut view, 0, None, None, span)?;",
            "let first = if position == 0 { super::indexed_frame_apply_child(tag, &mut view, 0, None, None, span)? } else { None };",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), sources.1.clone(), conditional_child))
            .unwrap_err().contains("not evaluated unconditionally"));

        let pushed_other = sources.2.replace(
            "self.push_apply_operands(index, instruction, tag, first, apply_span, next)?;",
            "self.push_apply_operands(index, instruction, tag, second, apply_span, next)?;",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), sources.1.clone(), pushed_other))
            .unwrap_err().contains("does not consume the selected child result"));

        let wrong_child = sources.1.replace(
            "indexed_frame_apply_child_from_fields",
            "indexed_other_child_from_fields",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), wrong_child, sources.2.clone()))
            .is_err());

        let changed_value = sources.1.replace(
            "indexed_path_from_value(value, span)",
            "indexed_other_from_value(value, span)",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), changed_value, sources.2.clone()))
            .unwrap_err().contains("does not return indexed_path_from_value"));

        let discarded_value_call = sources.1.replace(
            "ExprView::PathFrom { span, .. } => indexed_path_from_value(value, span),",
            "ExprView::PathFrom { span, .. } => { indexed_path_from_value(value, span); Ok(indexed_empty_map_value()) },",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), discarded_value_call, sources.2.clone()))
            .unwrap_err().contains("does not return indexed_path_from_value"));

        let discarded_child_call = sources.1.replace(
            "ExprView::PathFrom { value, span } => indexed_frame_apply_child_from_fields([(Some((value.raw(), None)))], position, *span),",
            "ExprView::PathFrom { value, span } => { indexed_frame_apply_child_from_fields([(Some((value.raw(), None)))], position, *span); None },",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), discarded_child_call, sources.2.clone()))
            .unwrap_err().contains("does not return child selection"));
    }

    #[test]
    fn frame_constructor_unknown_wildcard_guard_and_duplicate_patterns_fail_closed() {
        let sources = frame_constructor_fixture();
        let unknown_classifier = sources.1.replace(
            "FullTag::ExprRange |",
            "FullTag::ExprMystery |",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), unknown_classifier, sources.2.clone()))
            .unwrap_err().contains("unknown typed FrameApply tag"));

        let wildcard_classifier = sources.1.replace(
            "FullTag::ExprArchiveTarCreate",
            "_",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), wildcard_classifier, sources.2.clone()))
            .is_err());

        let child_arm = "ExprView::PathFrom { value, span } => indexed_frame_apply_child_from_fields([(Some((value.raw(), None)))], position, *span),";
        let missing_child = sources.1.replace(child_arm, "");
        assert!(frame_constructor_inventory(&(sources.0.clone(), missing_child, sources.2.clone()))
            .unwrap_err().contains("view arm coverage differs"));

        let duplicate_child = sources.1.replace(
            child_arm,
            &format!("{child_arm} {child_arm}"),
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), duplicate_child, sources.2.clone()))
            .unwrap_err().contains("duplicate indexed_frame_apply_child view arm PathFrom"));

        let unknown_view = sources.1.replacen(
            "ExprView::PathFrom { value, span } =>",
            "ExprView::Mystery { value, span } =>",
            1,
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), unknown_view, sources.2.clone()))
            .unwrap_err().contains("unknown ExprView arm Mystery"));

        let guarded_view = sources.1.replacen(
            "ExprView::PathFrom { value, span } =>",
            "ExprView::PathFrom { value, span } if position == 0 =>",
            1,
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), guarded_view, sources.2.clone()))
            .unwrap_err().contains("guarded indexed_frame_apply_child target arm"));

        let guarded_catch_all = sources.1.replace(
            "let child = match view {\n        ExprView::FunctionRef",
            "let child = match view {\n        candidate if position == 0 => None,\n        ExprView::FunctionRef",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), guarded_catch_all, sources.2.clone()))
            .unwrap_err().contains("guarded catch-all pattern"));

        let guarded_wildcard = sources.1.replace(
            "_ => unreachable!(\"frame apply tag has a matching typed view\")",
            "_ if position == 0 => unreachable!(\"frame apply tag has a matching typed view\")",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), guarded_wildcard, sources.2.clone()))
            .unwrap_err().contains("guarded catch-all pattern"));
    }

    #[test]
    fn frame_constructor_helper_wildcards_and_explicit_error_group_fail_closed() {
        let sources = frame_constructor_fixture();
        let changed_wildcard = sources.1.replace(
            "unreachable!(\"frame apply tag has a matching typed view\")",
            "None",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), changed_wildcard, sources.2.clone()))
            .unwrap_err().contains("wildcard outcome changed"));

        let missing_explicit_error = sources.2.replace("ExprViewKind::PathFrom | ", "");
        assert!(frame_constructor_inventory(&(sources.0.clone(), sources.1.clone(), missing_explicit_error))
            .unwrap_err().contains("defensive group differs"));

        let changed_explicit_error = sources.2.replace(
            "return Err(frame_strategy_error(tag, span));",
            "return Ok(());",
        );
        assert!(frame_constructor_inventory(&(sources.0.clone(), sources.1.clone(), changed_explicit_error))
            .unwrap_err().contains("no longer returns frame_strategy_error"));
    }

    #[test]
    fn eager_atom_inventory_maps_schema_macro_normal_and_forced_paths() {
        let sources = eager_fixture();
        let inventory = eager_inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 12);
        assert_eq!(inventory.schema_rows[0].tag, "ExprNull");
        assert_eq!(inventory.schema_rows[11].tag, "ExprParam");
        assert!(inventory.default_false_line < inventory.eager_true_line);
        assert!(inventory.eager_true_line < inventory.expansion_line);
        assert_eq!(inventory.normal_route.mode, "normal");
        assert_eq!(inventory.forced_route.mode, "forced");
        assert_eq!(inventory.helper_arms.len(), 12);
        assert_eq!(inventory.helper_arms[2].input, "value");
        assert_eq!(inventory.helper_arms[2].output, "LoweredValue::Int");
        assert_eq!(inventory.helper_arms[11].transformation, "freeze_slot_then_clone");
        assert_eq!(inventory.helper_fallback.outcome, "unreachable!: only eager atom tags reach this value helper");
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
        assert_eq!(inventory.helper_boundaries.len(), 3);
        assert_eq!(inventory.slot_decode_bounds_line > 0, true);
        assert_eq!(inventory.freeze_contract.shared_threshold, "LOWERED_SHARED_LIST_THRESHOLD");
        assert!(inventory.to_json().contains("\"runtime_error_paths\":{\"status\":\"source_exhaustive_boundary_inventory\""));
    }

    #[test]
    fn eager_atom_schema_add_remove_retag_metadata_and_duplicate_fail_closed() {
        let sources = eager_fixture();
        let added = sources.0.replace(
            "BuildExprRow => ExprView [Expr] {",
            "BuildExprRow => ExprView [Expr] { Added => ExprAdded = 32 [0; Operands; FrameApply; EagerAtom];",
        );
        assert!(eager_inventory(&(added, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("EagerAtom row flags"));

        let removed = sources.0.replace(
            "Int(value: i64) => ExprInt = 20 [0; Operands; FrameApply; EagerAtom];",
            "",
        );
        assert!(eager_inventory(&(removed, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("EagerAtom row flags"));

        let retagged = sources.0.replace("ExprNull = 18", "ExprOther = 18");
        assert!(eager_inventory(&(retagged, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("EagerAtom schema cohort drift"));

        let duplicate = sources.0.replace(
            "Unit => ExprUnit = 19 [0; Operands; FrameApply; EagerAtom];",
            "Unit => ExprUnit = 19 [0; Operands; FrameApply; EagerAtom]; UnitAgain => ExprUnit = 19 [0; Operands; FrameApply; EagerAtom];",
        );
        assert!(eager_inventory(&(duplicate, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("duplicate Expr schema tag ExprUnit"));

        let wrong_metadata = sources.0.replace(
            "Int(value: i64) => ExprInt = 20 [0; Operands; FrameApply; EagerAtom];",
            "Int(value: i64) => ExprInt = 20 [1; Operands; FrameApply; EagerAtom];",
        );
        assert!(eager_inventory(&(wrong_metadata, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("must be pure Operands; FrameApply"));

        let wrong_field = sources.0.replace(
            "Int(value: i64) => ExprInt = 20",
            "Int(value: u64) => ExprInt = 20",
        );
        assert!(eager_inventory(&(wrong_field, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema fields for ExprInt differ"));

        let parameter_child = sources.0.replace(
            "Param(slot: usize) => ExprParam = 31",
            "Param(slot: BuildExprId) => ExprParam = 31",
        );
        assert!(eager_inventory(&(parameter_child, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema fields for ExprParam differ"));

        let unclassified = sources.0.replace(
            "Int(value: i64) => ExprInt = 20 [0; Operands; FrameApply; EagerAtom];",
            "Int(value: i64) => ExprInt = 20 [0; Operands; FrameApply];",
        );
        assert!(eager_inventory(&(unclassified, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("EagerAtom row flags"));

        let eager_on_scalar = sources.0.replace(
            "instruction_schema! { BuildExprRow",
            "instruction_schema! { BuildIntRow => IntView [Int] { Other => IntOther = 0 [0; Operands; FrameApply; EagerAtom]; } BuildExprRow",
        );
        assert!(eager_inventory(&(eager_on_scalar, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("13 EagerAtom row flags"));
    }

    #[test]
    fn eager_atom_macro_classification_expansion_fails_closed() {
        let sources = eager_fixture();
        for (index, (source, message)) in [
            (sources.0.replace("(@eager_atom) => { false }", "(@eager_atom) => { true }"), "default rule"),
            (sources.0.replace("(@eager_atom EagerAtom) => { true }", "(@eager_atom EagerAtom) => { false }"), "true"),
            (sources.0.replace("$(; $classification:ident)?", "; $classification:ident"), "optional classification"),
            (sources.0.replace("instruction_schema!(@eager_atom $($classification)?)", "false"), "expansion changed"),
        ].into_iter().enumerate() {
            assert_ne!(source, sources.0, "macro drift fixture {index} did not change its input");
            let error = match eager_inventory(&(source, sources.1.clone(), sources.2.clone())) {
                Err(error) => error,
                Ok(_) => panic!("macro drift fixture {index} was accepted"),
            };
            assert!(error.contains(message), "fixture {index}: {error}");
        }
    }

    #[test]
    fn eager_atom_routes_must_call_before_generic_frame_apply() {
        let sources = eager_fixture();
        let normal_removed = sources.1.replace(
            "    if tag.is_eager_atom() {\n        return Ok(ControlFlow::Continue(indexed_eager_atom_value(execution, tag, payload, slots, call_span)?));\n    }\n",
            "",
        );
        assert!(eager_inventory(&(sources.0.clone(), normal_removed, sources.2.clone()))
            .unwrap_err().contains("no eager atom route"));

        let normal_reordered = sources.1.replace(
            "    if tag.is_eager_atom() {\n        return Ok(ControlFlow::Continue(indexed_eager_atom_value(execution, tag, payload, slots, call_span)?));\n    }\n    if indexed_frame_apply_tag(tag) {}",
            "    if indexed_frame_apply_tag(tag) {}\n    if tag.is_eager_atom() {\n        return Ok(ControlFlow::Continue(indexed_eager_atom_value(execution, tag, payload, slots, call_span)?));\n    }",
        );
        assert!(eager_inventory(&(sources.0.clone(), normal_reordered, sources.2.clone()))
            .unwrap_err().contains("after generic FrameApply"));

        let forced_removed = sources.2.replace(
            "    if tag.is_eager_atom() {\n        let call = &mut self.calls[index];\n        let value = super::indexed_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)?;\n        self.push_value(index, FrameValue::Value(value), next);\n        return Ok(());\n    }\n",
            "",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), forced_removed))
            .unwrap_err().contains("eval_expr has no eager atom route"));

        let forced_reordered = sources.2.replace(
            "    if tag.is_eager_atom() {\n        let call = &mut self.calls[index];\n        let value = super::indexed_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)?;\n        self.push_value(index, FrameValue::Value(value), next);\n        return Ok(());\n    }\n    if super::indexed_frame_apply_tag(tag) {}",
            "    if super::indexed_frame_apply_tag(tag) {}\n    if tag.is_eager_atom() {\n        let call = &mut self.calls[index];\n        let value = super::indexed_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)?;\n        self.push_value(index, FrameValue::Value(value), next);\n        return Ok(());\n    }",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), forced_reordered))
            .unwrap_err().contains("after generic FrameApply"));

        let wrong_normal_helper = sources.1.replace(
            "indexed_eager_atom_value(execution, tag, payload, slots, call_span)",
            "other_eager_atom_value(execution, tag, payload, slots, call_span)",
        );
        assert!(eager_inventory(&(sources.0.clone(), wrong_normal_helper, sources.2.clone()))
            .unwrap_err().contains("does not call indexed_eager_atom_value"));

        let wrong_forced_helper = sources.2.replace(
            "super::indexed_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)",
            "super::other_eager_atom_value(&call.execution, tag, payload, &mut call.slots, span)",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), wrong_forced_helper))
            .unwrap_err().contains("does not call indexed_eager_atom_value"));
    }

    #[test]
    fn eager_atom_view_arms_and_both_fallbacks_fail_closed() {
        let sources = eager_fixture();
        let missing = sources.1.replace("ExprView::Bytes(value) => LoweredValue::Bytes(Arc::from(value)),", "");
        assert!(eager_inventory(&(sources.0.clone(), missing, sources.2.clone()))
            .unwrap_err().contains("arm coverage differs"));

        let unknown = sources.1.replace("ExprView::Bytes(value)", "ExprView::Mystery(value)");
        assert!(eager_inventory(&(sources.0.clone(), unknown, sources.2.clone()))
            .unwrap_err().contains("unknown eager ExprView arm"));

        let duplicate = sources.1.replace(
            "ExprView::Null => LoweredValue::Null,",
            "ExprView::Null => LoweredValue::Null, ExprView::Null => LoweredValue::Null,",
        );
        assert!(eager_inventory(&(sources.0.clone(), duplicate, sources.2.clone()))
            .unwrap_err().contains("duplicate eager ExprView arm Null"));

        let wrong_value = sources.1.replace(
            "ExprView::Int(value) => LoweredValue::Int(value),",
            "ExprView::Int(value) => LoweredValue::Bool(value),",
        );
        assert!(eager_inventory(&(sources.0.clone(), wrong_value, sources.2.clone()))
            .unwrap_err().contains("does not preserve its decoded value"));

        let wrong_slot_binding = sources.1.replace(
            "ExprView::Param(slot) => { lowered_freeze_large_slot_list(&mut slots[slot]); slots[slot].clone() },",
            "ExprView::Param(other) => { lowered_freeze_large_slot_list(&mut slots[slot]); slots[slot].clone() },",
        );
        assert!(eager_inventory(&(sources.0.clone(), wrong_slot_binding, sources.2.clone()))
            .unwrap_err().contains("expected schema input slot"));

        let wrong_slot = sources.1.replace("slots[slot].clone()", "slots[other].clone()");
        assert!(eager_inventory(&(sources.0.clone(), wrong_slot, sources.2.clone()))
            .unwrap_err().contains("clones a different slot"));

        let freeze_after_clone = sources.1.replace(
            "lowered_freeze_large_slot_list(&mut slots[slot]); slots[slot].clone()",
            "slots[slot].clone(); lowered_freeze_large_slot_list(&mut slots[slot])",
        );
        assert!(eager_inventory(&(sources.0.clone(), freeze_after_clone, sources.2.clone()))
            .unwrap_err().contains("does not freeze before cloning"));

        let decode_bypass = sources.1.replace(
            "ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?",
            "ExprView::decode(execution, tag, payload)?",
        );
        assert!(eager_inventory(&(sources.0.clone(), decode_bypass, sources.2.clone()))
            .unwrap_err().contains("no indexed-error conversion"));

        let path_error_bypass = sources.1.replace(
            "indexed_error(IrVerifyError { message: error.message }, span)",
            "RuntimeError::new(\"path\", error.message)",
        );
        assert!(eager_inventory(&(sources.0.clone(), path_error_bypass, sources.2.clone()))
            .unwrap_err().contains("Path error must map through indexed_error"));

        let changed_wildcard = sources.1.replace(
            "unreachable!(\"only eager atom tags reach this value helper\")",
            "unreachable!(\"changed eager atom fallback\")",
        );
        assert!(eager_inventory(&(sources.0.clone(), changed_wildcard, sources.2.clone()))
            .unwrap_err().contains("wildcard is not the exact terminal unreachable arm"));

        let discarded_wildcard = sources.1.replace(
            "_ => unreachable!(\"only eager atom tags reach this value helper\"),",
            "_ => { unreachable!(\"only eager atom tags reach this value helper\"); LoweredValue::Null },",
        );
        assert!(eager_inventory(&(sources.0.clone(), discarded_wildcard, sources.2.clone()))
            .unwrap_err().contains("wildcard is not the exact terminal unreachable arm"));

        let missing_explicit = sources.2.replace("ExprViewKind::Path | ", "");
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), missing_explicit))
            .unwrap_err().contains("error group differs"));

        let duplicate_explicit = sources.2.replace(
            "ExprViewKind::Path | ExprViewKind::Param",
            "ExprViewKind::Path | ExprViewKind::Path | ExprViewKind::Param",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), duplicate_explicit))
            .unwrap_err().contains("error group differs"));

        let wrong_explicit = sources.2.replace(
            "return Err(frame_strategy_error(tag, span));",
            "return Ok(());",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), wrong_explicit))
            .unwrap_err().contains("must directly return frame_strategy_error"));

        let discarded_explicit = sources.2.replace(
            "return Err(frame_strategy_error(tag, span));",
            "let _discarded = Err(frame_strategy_error(tag, span)); return Ok(());",
        );
        assert!(eager_inventory(&(sources.0.clone(), sources.1.clone(), discarded_explicit))
            .unwrap_err().contains("must directly return frame_strategy_error"));
    }

    #[test]
    fn eager_atom_slot_decode_and_freeze_boundaries_fail_closed() {
        let sources = eager_fixture();
        let (slot_codec, freeze) = eager_auxiliary_fixture();
        let wrong_slot_bound = slot_codec.replace(
            "if slot >= decoder.slot_count as usize",
            "if slot < decoder.slot_count as usize",
        );
        assert!(eager_inventory_with_aux(&sources, &wrong_slot_bound, &freeze)
            .unwrap_err().contains("unexpected slot bounds boundary"));

        let missing_slot_bound = slot_codec.replace(
            "        if slot >= decoder.slot_count as usize {\n            return Err(IrVerifyError::new(\"slot is out of bounds\"));\n        }\n",
            "",
        );
        assert!(eager_inventory_with_aux(&sources, &missing_slot_bound, &freeze)
            .unwrap_err().contains("must read, bounds-check, and return one slot"));

        let changed_threshold = freeze.replace(
            "items.len() < LOWERED_SHARED_LIST_THRESHOLD",
            "items.len() <= LOWERED_SHARED_LIST_THRESHOLD",
        );
        assert!(eager_inventory_with_aux(&sources, &slot_codec, &changed_threshold)
            .unwrap_err().contains("threshold must leave smaller lists unchanged"));

        let frozen_after_move = freeze.replace(
            "let items = std::mem::take(items);",
            "let items = std::mem::take(slot);",
        );
        assert!(eager_inventory_with_aux(&sources, &slot_codec, &frozen_after_move)
            .unwrap_err().contains("does not move list items"));
    }

    #[test]
    fn typed_scalar_inventory_maps_schema_normal_and_forced_paths() {
        let sources = typed_fixture();
        let inventory = typed_inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 18);
        assert_eq!(inventory.classifiers["Int"].len(), 6);
        assert_eq!(inventory.classifiers["Bool"].len(), 12);
        assert_eq!(inventory.normal_checks[0].wrapper, "eval_indexed_typed_int");
        assert_eq!(inventory.normal_checks[1].wrapper, "eval_indexed_typed_bool");
        assert_eq!(inventory.view_arms.len(), 18);
        assert_eq!(inventory.frame_groups.len(), 3);
        assert_eq!(inventory.fallbacks.len(), 4);
        assert_eq!(inventory.wrong_union_errors.len(), 2);
        assert!(inventory.to_json().contains("\"wrong_union_errors\""));
    }

    #[test]
    fn typed_scalar_schema_additions_and_removals_fail_closed() {
        let sources = typed_fixture();
        let added_schema = sources.0.replace(
            " }\n BuildBoolRow",
            " Added => IntAdded = 6 [0; Operands; FrameApply]; }\n BuildBoolRow",
        );
        assert!(typed_scalar_schema_rows(&syn::parse_file(&added_schema).unwrap())
            .unwrap_err()
            .contains("schema cohort drift"));

        let removed_schema = sources.0.replace(
            "Int => IntInt = 0 [0; Operands; FrameApply];",
            "",
        );
        assert!(typed_scalar_schema_rows(&syn::parse_file(&removed_schema).unwrap())
            .unwrap_err()
            .contains("schema cohort drift"));

        let wrong_strategy = sources.0.replace("FrameApply];", "FrameControl];");
        assert!(typed_scalar_schema_rows(&syn::parse_file(&wrong_strategy).unwrap())
            .unwrap_err()
            .contains("must be pure Operands; FrameApply"));

        let wrong_classification = sources.0.replace(
            "[0; Operands; FrameApply];",
            "[0; Operands; FrameApply; EagerAtom];",
        );
        assert!(typed_scalar_schema_rows(&syn::parse_file(&wrong_classification).unwrap()).is_err());
    }

    #[test]
    fn typed_scalar_classifier_removal_unknown_duplicate_wildcard_and_guard_fail() {
        let sources = typed_fixture();
        let int_match = "FullTag::IntInt | FullTag::IntSlot | FullTag::IntBinary | FullTag::IntStrByteLenSlot | FullTag::IntStrCountLinesSlot | FullTag::IntStrByteAtSlot";
        let missing = sources.1.replace(" | FullTag::IntStrByteAtSlot", "");
        assert!(typed_inventory(&(sources.0.clone(), missing, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("missing typed Int tags"));

        let unknown = sources.1.replace("FullTag::IntStrByteAtSlot", "FullTag::IntUnknown");
        assert!(typed_inventory(&(sources.0.clone(), unknown, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("unknown typed Int tag"));

        let duplicate = sources.1.replace(
            "FullTag::IntStrByteAtSlot",
            "FullTag::IntStrByteAtSlot | FullTag::IntInt",
        );
        assert!(typed_inventory(&(sources.0.clone(), duplicate, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("duplicate typed Int tag"));

        let wildcard = sources.1.replace(int_match, &format!("{int_match} | _"));
        assert!(typed_inventory(&(sources.0.clone(), wildcard, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("wildcard inside an or-pattern"));

        let guarded = sources.1.replace(
            "FullTag::IntStrByteAtSlot",
            "FullTag::IntStrByteAtSlot if tag.is_eager_atom()",
        );
        assert!(typed_inventory(&(sources.0.clone(), guarded, sources.2.clone(), sources.3.clone())).is_err());
    }

    #[test]
    fn typed_scalar_normal_route_order_and_wrapper_mapping_are_checked() {
        let sources = typed_fixture();
        let int_check = "if is_typed_int_tag(tag) { return self.eval_indexed_typed_int(a,b,c,d).map(|flow| flow.map_continue(LoweredValue::Int)); }";
        let bool_check = "if is_typed_bool_tag(tag) { return self.eval_indexed_typed_bool(a,b,c,d).map(|flow| flow.map_continue(LoweredValue::Bool)); }";
        let reversed = sources.1.replace(
            &format!("{int_check}\n  {bool_check}"),
            &format!("{bool_check}\n  {int_check}"),
        );
        assert!(typed_inventory(&(sources.0.clone(), reversed, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("route Int before Bool"));

        let wrong_wrapper = sources.1.replace(
            "map_continue(LoweredValue::Int)",
            "map_continue(LoweredValue::Bool)",
        );
        assert!(typed_inventory(&(sources.0.clone(), wrong_wrapper, sources.2.clone(), sources.3.clone()))
            .unwrap_err()
            .contains("maps to the wrong LoweredValue variant"));

        let ordered_checks = format!("{int_check}\n  {bool_check}");
        let delayed = sources.1.replace(&ordered_checks, "");
        let delayed = delayed.replace(
            "if tag.is_eager_atom() { }",
            &format!("if tag.is_eager_atom() {{ }}\n  {ordered_checks}"),
        );
        let error = typed_inventory(&(sources.0.clone(), delayed, sources.2.clone(), sources.3.clone()))
            .unwrap_err();
        assert!(error.contains("after a generic evaluator route"), "{error}");
    }

    #[test]
    fn typed_scalar_view_arm_drift_is_rejected() {
        let sources = typed_fixture();
        let removed = sources.2.replace("IntView::StrByteAtSlot(..) => {},", "");
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), removed, sources.3.clone()))
            .unwrap_err()
            .contains("missing Int view arms"));

        let unknown = sources.2.replace("IntView::StrByteAtSlot", "IntView::StrUnknown");
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), unknown, sources.3.clone()))
            .unwrap_err()
            .contains("unknown Int view arm"));

        let duplicate = sources.2.replace(
            "IntView::Int(..) => {},",
            "IntView::Int(..) => {}, IntView::Int(..) => {},",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), duplicate, sources.3.clone()))
            .unwrap_err()
            .contains("duplicate Int view arm"));
    }

    #[test]
    fn typed_scalar_frame_groups_delegation_and_fallbacks_fail_closed() {
        let sources = typed_fixture();
        let missing_group = sources.3.replace(
            "FullTag::IntBinary | FullTag::IntStrByteAtSlot",
            "FullTag::IntBinary",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), missing_group))
            .unwrap_err()
            .contains("unrecognized typed scalar frame arm"));

        let split_bool_view_group = sources.3.replace(
            "BoolView::And(..) | BoolView::Or(..) => Some(()),",
            "BoolView::And(..) => Some(()), BoolView::Or(..) => Some(()),",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), split_bool_view_group))
            .unwrap_err()
            .contains("BoolView frame-operation group shapes differ"));

        let changed_inner_wildcard = sources.3.replace(
            "_ => return Err(frame_strategy_error(tag, span)),",
            "_ => Ok(()),",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), changed_inner_wildcard))
            .unwrap_err()
            .contains("wildcard is not a direct error"));

        let discarded_error = sources.3.replace(
            "_ => return Err(frame_strategy_error(tag, span)),",
            "_ => { let _discarded = Err(frame_strategy_error(tag, span)); Ok(()) },",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), discarded_error))
            .unwrap_err()
            .contains("wildcard is not a direct error"));

        let missing_proof = sources.3.replace(
            "if !call.execution.is_in_place(instruction) { return Err(frame_strategy_error(tag, span)); }",
            "if !call.execution.is_in_place(instruction) { Ok(()) }",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), missing_proof))
            .unwrap_err()
            .contains("missing-in-place branch must directly return"));

        let missing_guard_family = sources.3.replace(
            "tag if super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag) => None",
            "tag if super::is_typed_int_tag(tag) => None",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), missing_guard_family)).is_err());

        let missing_wildcard = sources.3.replace(
            "  _ => return Err(frame_strategy_error(tag, span)),\n }\n match operation",
            " }\n match operation",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), missing_wildcard))
            .unwrap_err()
            .contains("missing typed scalar wildcard fallback"));
    }

    #[test]
    fn typed_scalar_explicit_dispatch_order_and_wrong_union_errors_are_checked() {
        let sources = typed_fixture();
        let reordered = sources.3.replace(
            "if strategy == FrameStrategy::FrameApply && (super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag)) { return self.eval_typed_scalar_expr(a,b,c,d,e,f,g,h); }\n if super::indexed_frame_apply_tag(tag) {}",
            "if super::indexed_frame_apply_tag(tag) {}\n if strategy == FrameStrategy::FrameApply && (super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag)) { return self.eval_typed_scalar_expr(a,b,c,d,e,f,g,h); }",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), reordered))
            .unwrap_err()
            .contains("out of order"));

        let wrong_union = sources.2.replace(
            "typed integer evaluator produced Bool",
            "typed integer evaluator discarded Bool",
        );
        assert!(typed_inventory(&(sources.0.clone(), sources.1.clone(), wrong_union, sources.3.clone()))
            .unwrap_err()
            .contains("wrong-union branch changed"));
    }

    #[test]
    fn statement_schema_rows_and_grouped_match_arm_are_enumerated() {
        let inventory = inventory(SCHEMA, DISPATCH).unwrap();
        assert_eq!(inventory.schema_tags, ["StmtFirst", "StmtSecond"]);
        assert_eq!(inventory.branches.len(), 2);
        assert!(inventory.branches.iter().all(|branch| branch.arm_index == 0));
        assert_eq!(inventory.fallbacks.len(), 1);
        assert_eq!(inventory.fallbacks[0].outcome, "frame_strategy_error");
        assert!(inventory.branches.iter().all(|branch| branch.line > 0));
    }

    #[test]
    fn semicolon_match_followed_by_success_is_rejected() {
        let dispatch = DISPATCH.replace(
            "            _ => Err(frame_strategy_error(tag, span)),\n        }\n    }",
            "            _ => Err(frame_strategy_error(tag, span)),\n        };\n        Ok(())\n    }",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("eval_statement match tag must be the final tail expression"));
    }

    #[test]
    fn removing_a_statement_arm_fails_coverage() {
        let dispatch = DISPATCH.replace(" | FullTag::StmtSecond", "");
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("missing eval_statement tags: [\"StmtSecond\"]"));
    }

    #[test]
    fn adding_a_schema_row_and_arm_adds_inventory() {
        let schema = SCHEMA.replace(
            "    }\n}",
            "        Third => StmtThird = 2 [0; Scheduled; FrameControl];\n    }\n}",
        );
        let dispatch = DISPATCH.replace(
            "FullTag::StmtFirst | FullTag::StmtSecond",
            "FullTag::StmtFirst | FullTag::StmtSecond | FullTag::StmtThird",
        );
        let inventory = inventory(&schema, &dispatch).unwrap();
        assert_eq!(inventory.schema_tags.len(), 3);
        assert!(inventory.branches.iter().any(|branch| branch.tag == "StmtThird"));
    }

    #[test]
    fn unknown_dispatch_tag_fails() {
        let dispatch = DISPATCH.replace(
            "FullTag::StmtFirst | FullTag::StmtSecond",
            "FullTag::StmtFirst | FullTag::StmtSecond | FullTag::StmtUnknown",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("unknown eval_statement tag StmtUnknown"));
    }

    #[test]
    fn duplicate_dispatch_tag_fails() {
        let dispatch = DISPATCH.replace(
            "FullTag::StmtFirst | FullTag::StmtSecond => work(),",
            "FullTag::StmtFirst => work(),\n            FullTag::StmtFirst | FullTag::StmtSecond => work(),",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("duplicate eval_statement tags"));
    }

    #[test]
    fn wildcard_is_recorded_but_cannot_cover_a_missing_exact_row() {
        let dispatch = DISPATCH.replace(" | FullTag::StmtSecond", "");
        let error = inventory(SCHEMA, &dispatch).unwrap_err();
        assert!(error.contains("missing eval_statement tags: [\"StmtSecond\"]"));
    }

    #[test]
    fn removing_wildcard_fallback_fails_inventory() {
        let dispatch = DISPATCH.replace("            _ => Err(frame_strategy_error(tag, span)),\n", "");
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("expected one wildcard eval_statement fallback arm, found 0"));
    }

    #[test]
    fn wildcard_with_wrong_outcome_is_rejected() {
        let dispatch = DISPATCH.replace(
            "_ => Err(frame_strategy_error(tag, span))",
            "_ => Ok(())",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("unrecognized fallback outcome"));
    }

    #[test]
    fn wildcard_with_discarded_error_is_rejected() {
        let dispatch = DISPATCH.replace(
            "_ => Err(frame_strategy_error(tag, span))",
            "_ => { let _discarded = Err(frame_strategy_error(tag, span)); Ok(()) }",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("unrecognized fallback outcome"));
    }

    #[test]
    fn wildcard_before_exact_arm_is_rejected() {
        let dispatch = DISPATCH.replace(
            "            FullTag::StmtFirst | FullTag::StmtSecond => work(),\n            _ => Err(frame_strategy_error(tag, span)),",
            "            _ => Err(frame_strategy_error(tag, span)),\n            FullTag::StmtFirst | FullTag::StmtSecond => work(),",
        );
        assert!(inventory(SCHEMA, &dispatch)
            .unwrap_err()
            .contains("wildcard fallback"));
    }

    #[test]
    fn guarded_and_unparsed_patterns_fail_closed() {
        let guarded = DISPATCH.replace(
            "FullTag::StmtFirst | FullTag::StmtSecond => work()",
            "FullTag::StmtFirst | FullTag::StmtSecond if enabled => work()",
        );
        assert!(inventory(SCHEMA, &guarded)
            .unwrap_err()
            .contains("guarded eval_statement arm"));

        let unparsed = DISPATCH.replace(
            "FullTag::StmtFirst | FullTag::StmtSecond => work()",
            "other => work()",
        );
        assert!(inventory(SCHEMA, &unparsed)
            .unwrap_err()
            .contains("unparsed eval_statement pattern"));
    }
}

#[cfg(test)]
mod collection_construction_tests {
    use super::extract_collection_construction_inventory;
    use std::path::Path;

    fn source_fixture() -> (String, String, String, String) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("scratch extractor lives four directories below the repo root");
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        (
            read("src/runtime/eval/indexed/full/schema.rs"),
            read("src/runtime/eval/lowered_run/indexed_run.rs"),
            read("src/runtime/eval/lowered_run/indexed_run/explicit_run.rs"),
            read("src/runtime/eval/lowered_run/indexed_run/operands.rs"),
        )
    }

    fn inventory(sources: &(String, String, String, String)) -> Result<super::CollectionConstructionInventory, String> {
        extract_collection_construction_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            &sources.3,
            "src/runtime/eval/indexed/full/schema.rs",
            "src/runtime/eval/lowered_run/indexed_run.rs",
            "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
            "src/runtime/eval/lowered_run/indexed_run/operands.rs",
        )
    }

    #[test]
    fn current_collection_constructor_rows_routes_and_fallbacks_are_exhaustive() {
        let sources = source_fixture();
        let inventory = inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 5);
        assert_eq!(inventory.normal_arms.len(), 5);
        assert_eq!(inventory.forced_arms.len(), 5);
        assert_eq!(inventory.normal_view_fallbacks.len(), 4);
        assert_eq!(inventory.helper_fallbacks.len(), 4);
        let update_fallback = inventory
            .helper_fallbacks
            .iter()
            .find(|fallback| fallback.helper == "decode_record_updates")
            .unwrap();
        let unreachable_line = sources
            .3
            .lines()
            .position(|line| line.contains("unreachable!(\"record-update tag has a matching borrowed expression view\")"))
            .unwrap()
            + 1;
        assert_eq!(unreachable_line, 44);
        assert_eq!(update_fallback.line, unreachable_line);
        assert!(inventory.normal_arms.iter().all(|arm| arm.arm_index < inventory.normal_fallback.arm_index));
        assert!(inventory.frame_apply_classifier_tags.iter().all(|tag| {
            !["ExprRecord", "ExprMapLiteral", "ExprRecordUpdate", "ExprList", "ExprListBuild"].contains(&tag.as_str())
        }));
        assert!(inventory.forced_route.frame_apply_statement_index < inventory.forced_route.view_kind_statement_index);
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
    }

    #[test]
    fn collection_constructor_dispatch_and_dataflow_drifts_fail_closed() {
        let sources = source_fixture();

        let removed = sources.0.replace("ListBuild(values:", "UnusedListBuild(values:");
        assert!(inventory(&(removed, sources.1.clone(), sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("schema cohort drift"));

        let wrong_list_value = sources.1.replace(
            "append_lowered_list_element(&mut result, value, false, span)",
            "append_lowered_list_element(&mut result, other, false, span)",
        );
        assert!(inventory(&(sources.0.clone(), wrong_list_value, sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("does not flow to the matching list append"));

        let wrong_map_order = sources.1.replace(
            "append_lowered_map_literal(&mut map, key, value, span)",
            "append_lowered_map_literal(&mut map, value, key, span)",
        );
        assert!(inventory(&(sources.0.clone(), wrong_map_order, sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("MapLiteral child results do not flow"));

        let key_start = sources.1.find("                    let key = if let Some(key) = key {").unwrap();
        let value_start = sources.1[key_start..].find("                    let value = match self.eval_indexed_expr(execution, value.raw(), slots, span)? {").unwrap() + key_start;
        let append_start = sources.1[value_start..].find("                    append_lowered_map_literal(&mut map, key, value, span)?;").unwrap() + value_start;
        assert!(key_start < value_start && value_start < append_start);
        let swapped_map_children = format!(
            "{}{}{}{}",
            &sources.1[..key_start],
            &sources.1[value_start..append_start],
            &sources.1[key_start..value_start],
            &sources.1[append_start..],
        );
        assert!(inventory(&(
            sources.0.clone(),
            swapped_map_children,
            sources.2.clone(),
            sources.3.clone(),
        )).unwrap_err().contains("MapLiteral key child evaluation must precede value child evaluation"));

        let wrong_update_value = sources.1.replace(
            "replacements.push((path, value, field_span))",
            "replacements.push((path, base, field_span))",
        );
        assert!(inventory(&(sources.0.clone(), wrong_update_value, sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("base and replacement child results"));

        let wrong_forced_route = sources.2.replace(
            "self.eval_expr_view(index, instruction, tag, view_kind, &execution, payload, span, next)",
            "self.eval_expr_view(index, instruction, tag, view_kind, &execution, payload, wrong_span, next)",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), wrong_forced_route, sources.3.clone()))
            .unwrap_err().contains("does not pass the selected ExprViewKind"));

        let selected_falls_back = sources.2.replace(
            "ExprViewKind::Null |",
            "ExprViewKind::List | ExprViewKind::Null |",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), selected_falls_back, sources.3.clone()))
            .unwrap_err().contains("intercepted by frame_strategy_error"));
    }

    #[test]
    fn collection_constructor_typed_view_fallback_drifts_fail_closed() {
        let sources = source_fixture();

        let changed_helper_arm = sources.2.replace(
            "ExprView::ListBuild(items) => Ok(IndexedListLiteralView::Build(items))",
            "ExprView::ListBuild(items) => Ok(IndexedListLiteralView::List(items))",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), changed_helper_arm, sources.3.clone()))
            .unwrap_err().contains("discards or changes its decoded view value"));

        let changed_helper_error = sources.2.replace(
            "list literal view is invalid",
            "another list view is invalid",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), changed_helper_error, sources.3.clone()))
            .unwrap_err().contains("typed-view fallback changed"));

        let changed_normal_view = sources.1.replace(
            "ExprView::Record(mut entries)",
            "ExprView::MapLiteral(mut entries)",
        );
        assert!(inventory(&(sources.0.clone(), changed_normal_view, sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("normal Record typed-view fallback changed"));

        let changed_update_guard = sources.3.replace(
            "record-update tag has a matching borrowed expression view",
            "wrong update view",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), sources.2.clone(), changed_update_guard))
            .unwrap_err().contains("typed-view fallback changed"));

        let changed_normal_fallback = sources.1.replace(
            "direct indexed evaluator does not support",
            "unsupported indexed tag",
        );
        assert!(inventory(&(sources.0.clone(), changed_normal_fallback, sources.2.clone(), sources.3.clone()))
            .unwrap_err().contains("unsupported-tag error"));
    }
}

#[cfg(test)]
mod result_constructor_tests {
    use super::extract_result_constructor_inventory;
    use std::path::Path;

    fn source_fixture() -> (String, String, String) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("scratch extractor lives four directories below the repo root");
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        (
            read("src/runtime/eval/indexed/full/schema.rs"),
            read("src/runtime/eval/lowered_run/indexed_run.rs"),
            read("src/runtime/eval/lowered_run/indexed_run/explicit_run.rs"),
        )
    }

    fn inventory(sources: &(String, String, String)) -> Result<super::ResultConstructorInventory, String> {
        extract_result_constructor_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            "src/runtime/eval/indexed/full/schema.rs",
            "src/runtime/eval/lowered_run/indexed_run.rs",
            "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        )
    }

    #[test]
    fn current_result_constructor_rows_routes_and_fallbacks_are_exhaustive() {
        let sources = source_fixture();
        let inventory = inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 2);
        assert_eq!(inventory.normal_arms.len(), 2);
        assert_eq!(inventory.forced_arms.len(), 2);
        assert_eq!(inventory.normal_view_fallbacks.len(), 2);
        assert_eq!(inventory.forced_view_fallbacks.len(), 2);
        assert!(inventory.normal_arms.iter().all(|arm| arm.arm_index < inventory.normal_fallback.arm_index));
        assert!(inventory.forced_arms.iter().all(|arm| arm.arm_index < inventory.explicit_error_group.arm_index));
        assert!(inventory.forced_route.frame_apply_statement_index < inventory.forced_route.view_kind_statement_index);
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
    }

    #[test]
    fn result_constructor_schema_and_child_dataflow_drifts_fail_closed() {
        let sources = source_fixture();

        let removed = sources.0.replace("Ok(value: BuildExprId) => ExprOk = 104", "Okay(value: BuildExprId) => ExprOk = 104");
        assert!(inventory(&(removed, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema cohort drift"));

        let duplicated = sources.0.replace(
            "Ok(value: BuildExprId) => ExprOk = 104",
            "Ok(value: BuildExprId) => ExprOk = 104;\n        OkAgain(value: BuildExprId) => ExprOk = 104",
        );
        assert!(inventory(&(duplicated, sources.1.clone(), sources.2.clone())).is_err());

        let wrong_ok_value = sources.1.replace(
            "LoweredValue::ResultOk(Box::new(value))",
            "LoweredValue::ResultOk(Box::new(other))",
        );
        assert!(inventory(&(sources.0.clone(), wrong_ok_value, sources.2.clone()))
            .unwrap_err().contains("Ok child result does not flow"));

        let wrong_err_arguments = sources.1.replace(
            "lowered_err_with_cause(value, cause, call_span)?",
            "lowered_err_with_cause(cause, value, call_span)?",
        );
        assert!(inventory(&(sources.0.clone(), wrong_err_arguments, sources.2.clone()))
            .unwrap_err().contains("Err value and optional cause"));

        let changed_cause_input = sources.1.replace(
            "let cause = match cause.map(|cause| cause.raw()) {",
            "let cause = match cause {",
        );
        assert!(inventory(&(sources.0.clone(), changed_cause_input, sources.2.clone()))
            .unwrap_err().contains("Err value and optional cause"));
    }

    #[test]
    fn result_constructor_forced_route_and_fallback_drifts_fail_closed() {
        let sources = source_fixture();

        let wrong_continuation = sources.2.replace(
            "FrameContinuation::WrapOk(Box::new(next))",
            "FrameContinuation::WrapOk(Box::new(other))",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), wrong_continuation))
            .unwrap_err().contains("forced Ok handler"));

        let intercepted = sources.2.replace(
            "            ExprViewKind::Null |",
            "            ExprViewKind::Ok |\n            ExprViewKind::Null |",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), intercepted))
            .unwrap_err().contains("intercepted by frame_strategy_error"));

        let changed_guard = sources.1.replace(
            "ok tag has an ok view",
            "ok tag has a different view",
        );
        assert!(inventory(&(sources.0.clone(), changed_guard, sources.2.clone()))
            .unwrap_err().contains("normal Ok typed-view fallback changed"));
    }
}

#[cfg(test)]
mod call_frame_tests {
    use super::extract_call_frame_inventory;
    use std::path::Path;

    fn source_fixture() -> (String, String, String) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("scratch extractor lives four directories below the repo root");
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        (
            read("src/runtime/eval/indexed/full/schema.rs"),
            read("src/runtime/eval/lowered_run/indexed_run.rs"),
            read("src/runtime/eval/lowered_run/indexed_run/explicit_run.rs"),
        )
    }

    fn inventory(sources: &(String, String, String)) -> Result<super::CallFrameInventory, String> {
        extract_call_frame_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            "src/runtime/eval/indexed/full/schema.rs",
            "src/runtime/eval/lowered_run/indexed_run.rs",
            "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        )
    }

    #[test]
    fn current_call_rows_use_scheduled_control_and_explicit_call_handlers() {
        let sources = source_fixture();
        let inventory = inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.len(), 4);
        assert_eq!(inventory.forced_arms.len(), 4);
        assert_eq!(inventory.typed_view_fallbacks.len(), 4);
        assert!(inventory.schema_rows.iter().all(|row| row.class == "Scheduled" && row.strategy == "FrameControl"));
        assert!(inventory.forced_arms.iter().all(|arm| arm.arm_index < inventory.explicit_error_group.arm_index));
        assert_eq!(inventory.forced_arms[0].arm_index, inventory.forced_arms[1].arm_index);
        assert_eq!(inventory.forced_arms[0].view_group, ["Call", "SelfCall", "DirectPureCall"]);
        assert_eq!(inventory.forced_arms[2].view_group, ["ExternalCall"]);
        assert_eq!(inventory.forced_arms[3].view_group, ["DynamicCall", "TypedCall"]);
        assert!(inventory.normal_route.scheduled_predicate_line > 0);
        assert!(inventory.normal_route.frame_schedule_line > 0);
        assert!(inventory.normal_route.frame_helper_line > 0);
        assert!(inventory.normal_route.inner_helper_line > 0);
        assert!(inventory.normal_route.view_kind_line > 0);
        assert!(inventory.normal_route.eval_expr_view_line > inventory.normal_route.view_kind_line);
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
    }

    #[test]
    fn call_schema_and_schedule_drifts_fail_closed() {
        let sources = source_fixture();
        let removed_row = sources.0.replace(
            "Call {\n            function: LoweredFunctionKey,",
            "MissingCall {\n            function: LoweredFunctionKey,",
        );
        assert!(inventory(&(removed_row, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema cohort drift"));

        let changed_strategy = sources.0.replace(
            "} => ExprCall = 108 [EFFECT_TRACE; Scheduled; FrameControl];",
            "} => ExprCall = 108 [EFFECT_TRACE; Operands; FrameControl];",
        );
        assert!(inventory(&(changed_strategy, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("schema row ExprCall"));

        let bypassed_schedule = sources.1.replace(
            "tag.is_scheduled_frame_control_expr()",
            "false",
        );
        assert!(inventory(&(sources.0.clone(), bypassed_schedule, sources.2.clone()))
            .unwrap_err().contains("schema FrameControl predicate"));

        let detached_tag = sources.1.replace(
            ".map(|(tag, _)| {",
            ".map(|(other, _)| {",
        );
        assert!(inventory(&(sources.0.clone(), detached_tag, sources.2.clone()))
            .unwrap_err().contains("predicate no longer uses the instruction tag"));

        let forced_route_bypass = sources.1.replace(
            "if self.force_frames || frame_scheduled {\n            self.eval_indexed_expr_with_frames",
            "if self.force_frames || frame_scheduled {\n            if self.force_frames { return Err(RuntimeError::new(\"test\", \"bypass\")); }\n            self.eval_indexed_expr_with_frames",
        );
        assert!(inventory(&(sources.0.clone(), forced_route_bypass, sources.2.clone()))
            .unwrap_err().contains("normal scheduled call branch has no frame evaluator"));

        let early_forced_return = sources.1.replace(
            "        let result = if self.force_frames || frame_scheduled {",
            "        if self.force_frames { return Err(RuntimeError::new(\"test\", \"bypass\")); }\n        let result = if self.force_frames || frame_scheduled {",
        );
        assert!(inventory(&(sources.0.clone(), early_forced_return, sources.2.clone()))
            .unwrap_err().contains("route can be bypassed before the scheduling branch"));
    }

    #[test]
    fn call_handlers_and_typed_view_fallbacks_fail_closed() {
        let sources = source_fixture();

        let missing_external = sources.2.replace(
            "ExprViewKind::ExternalCall => {",
            "ExprViewKind::ExternalCallMissing => {",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), missing_external))
            .unwrap_err().contains("forced call view coverage"));

        let changed_fallback = sources.2.replace(
            "static call tag has a typed call view",
            "static call view differs",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), changed_fallback))
            .unwrap_err().contains("unconditional unreachable!"));

        let wrong_dynamic_callee = sources.2.replace(
            "                    callee,\n                    value_span,\n                    FrameContinuation::DynamicCallee {",
            "                    wrong_callee,\n                    value_span,\n                    FrameContinuation::DynamicCallee {",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), wrong_dynamic_callee))
            .unwrap_err().contains("evaluates its callee before arguments"));

        let wrong_callee_source = sources.2.replace(
            "} => (callee.raw(), None, args, span),",
            "} => (other.raw(), None, args, span),",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), wrong_callee_source))
            .unwrap_err().contains("callee no longer flows from its decoded typed view"));

        let shadowed_callee = sources.2.replace(
            "                let capacity = args.len();\n                args.preflight()\n                    .map_err(|error| indexed_error(error, value_span))?;\n                let position = args.position();\n                self.push_expr(",
            "                let callee = wrong_callee;\n                let capacity = args.len();\n                args.preflight()\n                    .map_err(|error| indexed_error(error, value_span))?;\n                let position = args.position();\n                self.push_expr(",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), shadowed_callee))
            .unwrap_err().contains("callee no longer flows from its decoded typed view"));

        let reassigned_callee = sources.2.replace(
            "                let capacity = args.len();",
            "                callee = instruction;\n                let capacity = args.len();",
        );
        assert!(reassigned_callee.contains("callee = instruction;"));
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), reassigned_callee))
            .unwrap_err().contains("callee no longer flows from its decoded typed view"));

        let mutable_callee_binding = sources.2.replace(
            "let (callee, typed_pure, mut args, value_span) = match view {",
            "let (mut callee, typed_pure, mut args, value_span) = match view {",
        );
        assert!(mutable_callee_binding.contains("let (mut callee, typed_pure"));
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), mutable_callee_binding))
            .unwrap_err().contains("callee no longer flows from its decoded typed view"));

        let loop_shadowed_callee = sources.2
            .replace(
                "                self.push_expr(\n                    index,\n                    callee,\n                    value_span,\n                    FrameContinuation::DynamicCallee {",
                "                for callee in [instruction] {\n                    self.push_expr(\n                    index,\n                    callee,\n                    value_span,\n                    FrameContinuation::DynamicCallee {",
            )
            .replace(
                "                    },\n                );\n            }\n            ExprViewKind::Field =>",
                "                    },\n                    );\n                }\n            }\n            ExprViewKind::Field =>",
            );
        assert!(loop_shadowed_callee.contains("for callee in [instruction]"));
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), loop_shadowed_callee))
            .unwrap_err().contains("callee no longer flows from its decoded typed view"));

        let conditional_wildcard = sources.2.replace(
            "_ => unreachable!(\"dynamic call tag has a typed call view\"),",
            "_ => if true { unreachable!(\"dynamic call tag has a typed call view\") } else { unreachable!() },",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), conditional_wildcard))
            .unwrap_err().contains("typed-view fallback is not an unconditional unreachable!"));

        let intercepted = sources.2.replace(
            "            ExprViewKind::Null |",
            "            ExprViewKind::DynamicCall |\n            ExprViewKind::Null |",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), intercepted))
            .unwrap_err().contains("intercepted by frame_strategy_error"));

        let forced_liveness_bypass = sources.2.replace(
            "        let view_kind = tag\n            .expr_view_kind()",
            "        if matches!(tag, FullTag::ExprCall) { return Ok(()); }\n        let view_kind = tag\n            .expr_view_kind()",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), forced_liveness_bypass))
            .unwrap_err().contains("unreviewed early return before ExprViewKind dispatch"));
    }
}

#[cfg(test)]
mod field_index_tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn read(path: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../");
        fs::read_to_string(root.join(path)).unwrap()
    }

    fn source_fixture() -> (String, String, String) {
        (
            read("src/runtime/eval/indexed/full/schema.rs"),
            read("src/runtime/eval/lowered_run/indexed_run.rs"),
            read("src/runtime/eval/lowered_run/indexed_run/explicit_run.rs"),
        )
    }

    fn inventory(sources: &(String, String, String)) -> Result<FieldIndexInventory, String> {
        extract_field_index_inventory(
            &sources.0,
            &sources.1,
            &sources.2,
            "src/runtime/eval/indexed/full/schema.rs",
            "src/runtime/eval/lowered_run/indexed_run.rs",
            "src/runtime/eval/lowered_run/indexed_run/explicit_run.rs",
        )
    }

    #[test]
    fn current_field_index_rows_have_normal_and_forced_routes() {
        let sources = source_fixture();
        let inventory = inventory(&sources).unwrap();
        assert_eq!(inventory.schema_rows.iter().map(|row| row.tag.as_str()).collect::<Vec<_>>(), [
            "ExprField", "ExprIndex", "ExprIndexFromEnd",
        ]);
        assert_eq!(inventory.normal_arms.len(), 3);
        assert_eq!(inventory.forced_arms.len(), 3);
        assert_eq!(inventory.normal_view_fallbacks.len(), 3);
        assert_eq!(inventory.forced_view_fallbacks.len(), 3);
        assert_eq!(inventory.normal_arms[0].behavior, "try_record_field_chain_then_evaluate_base_and_lookup_field");
        assert_eq!(inventory.normal_arms[1].behavior, "evaluate_base_then_index_and_lookup");
        assert_eq!(inventory.normal_arms[2].behavior, "evaluate_base_then_lookup_from_end_distance");
        assert_eq!(inventory.forced_arms[0].behavior, "try_record_field_chain_then_schedule_base");
        assert_eq!(inventory.forced_arms[1].behavior, "schedule_base_then_index_continuation");
        assert_eq!(inventory.forced_arms[2].behavior, "schedule_base_then_from_end_continuation");
        assert!(inventory.normal_arms.iter().all(|arm| arm.arm_index < inventory.normal_fallback.arm_index));
        assert!(inventory.forced_arms.iter().all(|arm| arm.arm_index < inventory.explicit_error_group.arm_index));
        assert!(inventory.forced_route.frame_apply_statement_index < inventory.forced_route.view_kind_statement_index);
        assert_eq!(inventory.explicit_error_group.outcome, "frame_strategy_error(tag, span)");
    }

    #[test]
    fn field_index_schema_child_order_and_continuations_fail_closed() {
        let sources = source_fixture();

        let removed_schema_row = sources.0.replace(
            "} => ExprField = 56 [0; Operands; FrameApply];",
            "} => ExprFieldRemoved = 56 [0; Operands; FrameApply];",
        );
        assert!(inventory(&(removed_schema_row, sources.1.clone(), sources.2.clone()))
            .unwrap_err().contains("field/index schema cohort drift"));

        let wrong_index_child = sources.1.replace(
            "let index = match self.eval_indexed_expr(execution, index, slots, span)? {",
            "let index = match self.eval_indexed_expr(execution, base, slots, span)? {",
        );
        assert!(wrong_index_child.contains("let index = match self.eval_indexed_expr(execution, base"));
        assert!(inventory(&(sources.0.clone(), wrong_index_child, sources.2.clone()))
            .unwrap_err().contains("normal Index child order or source changed"));

        let wrong_lookup_order = sources.1.replace(
            "lowered_index_value(base, index, span)",
            "lowered_index_value(index, base, span)",
        );
        assert!(inventory(&(sources.0.clone(), wrong_lookup_order, sources.2.clone()))
            .unwrap_err().contains("normal index arm changed"));

        let wrong_continuation = sources.2.replace(
            "distance: distance.get(),",
            "distance: instruction.get(),",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), wrong_continuation))
            .unwrap_err().contains("forced IndexFromEnd continuation data flow changed"));
    }

    #[test]
    fn field_index_wrapper_child_liveness_and_forced_sources_fail_closed() {
        let sources = source_fixture();

        let scheduled_index = sources.1.replace(
            "| FullTag::ExprLastStatus",
            "| FullTag::ExprIndex\n                            | FullTag::ExprLastStatus",
        );
        assert!(scheduled_index.contains("FullTag::ExprIndex\n                            | FullTag::ExprLastStatus"));
        assert!(inventory(&(sources.0.clone(), scheduled_index, sources.2.clone()))
            .unwrap_err().contains("normal field/index wrapper"));

        let rebound_child = sources.1.replace(
            "ControlFlow::Continue(lowered_index_value(base, index, span)?)",
            "base = index;\n                ControlFlow::Continue(lowered_index_value(base, index, span)?)",
        );
        assert!(inventory(&(sources.0.clone(), rebound_child, sources.2.clone()))
            .unwrap_err().contains("normal Index child result is reassigned or shadowed"));

        let early_continue = sources.1.replace(
            "ControlFlow::Continue(lowered_index_value(base, index, span)?)",
            "return Ok(ControlFlow::Continue(base));\n                ControlFlow::Continue(lowered_index_value(base, index, span)?)",
        );
        assert!(inventory(&(sources.0.clone(), early_continue, sources.2.clone()))
            .unwrap_err().contains("normal Index arm can return before its lookup"));

        let field_fast_path_bypass = sources.1.replace(
            "let name = name.as_ref();",
            "return Ok(ControlFlow::Continue(LoweredValue::Nil));\n                let name = name.as_ref();",
        );
        assert!(inventory(&(sources.0.clone(), field_fast_path_bypass, sources.2.clone()))
            .unwrap_err().contains("normal Field fast path does not return its looked-up record value"));

        let forced_shadow = sources.2.replace(
            "                self.push_expr(\n                    index,\n                    base.raw(),\n                    value_span,\n                    FrameContinuation::IndexFromEnd",
            "                let base = distance.get();\n                self.push_expr(\n                    index,\n                    base.raw(),\n                    value_span,\n                    FrameContinuation::IndexFromEnd",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), forced_shadow))
            .unwrap_err().contains("forced IndexFromEnd source binding is shadowed or written"));

        let forced_bypass = sources.2.replace(
            "                self.push_expr(\n                    index,\n                    base.raw(),\n                    value_span,\n                    FrameContinuation::IndexFromEnd",
            "                return Ok(());\n                self.push_expr(\n                    index,\n                    base.raw(),\n                    value_span,\n                    FrameContinuation::IndexFromEnd",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), forced_bypass))
            .unwrap_err().contains("forced IndexFromEnd handler can return before scheduling"));

        let field_forced_bypass = sources.2.replace(
            "self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
            "return Ok(());\n                self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), field_forced_bypass))
            .unwrap_err().contains("forced Field handler can return before scheduling"));

        let forced_field_return_before_fast_push = sources.2.replace(
            "self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());",
            "return Ok(());\n                        self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), forced_field_return_before_fast_push))
            .unwrap_err().contains("forced Field fast path must push before returning"));
    }

    #[test]
    fn field_index_normal_and_forced_fast_paths_reject_ancestor_returns() {
        let sources = source_fixture();
        let reject_normal = |indexed: String| {
            let error = inventory(&(sources.0.clone(), indexed, sources.2.clone())).unwrap_err();
            assert!(error.contains("normal Field fast path does not return its looked-up record value"), "{error}");
        };
        let normal_before_fast_path = sources.1.replacen(
            "if let Some(base) = Self::indexed_field_chain_ref",
            "return Ok(ControlFlow::Continue(LoweredValue::Nil));\n                if let Some(base) = Self::indexed_field_chain_ref",
            1,
        );
        reject_normal(normal_before_fast_path);

        let normal_inside_fast_path = sources.1.replacen(
            "return Ok(ControlFlow::Continue(value));",
            "return Ok(ControlFlow::Continue(LoweredValue::Nil));\n                    return Ok(ControlFlow::Continue(value));",
            1,
        );
        reject_normal(normal_inside_fast_path);

        let normal_after_fast_path = sources.1.replacen(
            "return Ok(ControlFlow::Continue(value));\n                }\n                let base = match self.eval_indexed_expr(execution, base, slots, span)? {",
            "return Ok(ControlFlow::Continue(value));\n                }\n                return Ok(ControlFlow::Continue(LoweredValue::Nil));\n                let base = match self.eval_indexed_expr(execution, base, slots, span)? {",
            1,
        );
        reject_normal(normal_after_fast_path);

        let normal_ancestor = sources.1
            .replacen(
                "if let Some(base) = Self::indexed_field_chain_ref",
                "if true {\n                    return Ok(ControlFlow::Continue(LoweredValue::Nil));\n                    if let Some(base) = Self::indexed_field_chain_ref",
                1,
            )
            .replacen(
                "return Ok(ControlFlow::Continue(value));\n                }\n                let base = match self.eval_indexed_expr(execution, base, slots, span)? {",
                "return Ok(ControlFlow::Continue(value));\n                }\n                }\n                let base = match self.eval_indexed_expr(execution, base, slots, span)? {",
                1,
            );
        reject_normal(normal_ancestor);

        let reject_forced = |explicit: String, expected: &str| {
            let error = inventory(&(sources.0.clone(), sources.1.clone(), explicit)).unwrap_err();
            assert!(error.contains(expected), "{error}");
        };
        let outer_start = sources.2.replacen(
            "if !self.evaluator.force_frames {\n                    if let Some(base) = Evaluator::indexed_field_chain_ref(",
            "if !self.evaluator.force_frames {\n                    return Ok(());\n                    if let Some(base) = Evaluator::indexed_field_chain_ref(",
            1,
        );
        reject_forced(outer_start, "forced Field fast path must push before returning");

        let outer_end = sources.2.replacen(
            "                        return Ok(());\n                    }\n                }\n                self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
            "                        return Ok(());\n                    }\n                    return Ok(());\n                }\n                self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
            1,
        );
        reject_forced(outer_end, "forced Field fast path must push before returning");

        let before_outer = sources.2.replacen(
            "let name = name.to_string();\n                if !self.evaluator.force_frames {",
            "let name = name.to_string();\n                return Ok(());\n                if !self.evaluator.force_frames {",
            1,
        );
        reject_forced(before_outer, "forced Field handler can return before scheduling");

        let between_push_and_return = sources.2.replacen(
            "self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());",
            "self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());\n                        return Ok(());",
            1,
        );
        reject_forced(between_push_and_return, "forced Field fast path must push before returning");

        let after_inner_return = sources.2.replacen(
            "self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());\n                    }",
            "self.push_value(index, FrameValue::Value(value), next);\n                        return Ok(());\n                        return Ok(());\n                    }",
            1,
        );
        reject_forced(after_inner_return, "forced Field fast path must push before returning");

        let after_outer = sources.2.replacen(
            "                }\n                self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
            "                }\n                return Ok(());\n                self.push_expr(\n                    index,\n                    base,\n                    value_span,\n                    FrameContinuation::Field",
            1,
        );
        reject_forced(after_outer, "forced Field handler can return before scheduling");
    }

    #[test]
    fn field_index_typed_view_fallbacks_and_error_group_fail_closed() {
        let sources = source_fixture();

        let changed_normal_fallback = sources.1.replace(
            "field tag has a field view",
            "field tag has another view",
        );
        assert!(inventory(&(sources.0.clone(), changed_normal_fallback, sources.2.clone()))
            .unwrap_err().contains("unconditional"));

        let changed_forced_fallback = sources.2.replace(
            "index tag has an index view",
            "index tag has another view",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), changed_forced_fallback))
            .unwrap_err().contains("unconditional"));

        let intercepted = sources.2.replace(
            "            ExprViewKind::Null |",
            "            ExprViewKind::Index |\n            ExprViewKind::Null |",
        );
        assert!(inventory(&(sources.0.clone(), sources.1.clone(), intercepted))
            .unwrap_err().contains("intercepted by frame_strategy_error"));
    }
}
