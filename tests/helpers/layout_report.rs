use std::mem::{align_of, size_of};
use xsh::modules::signature as main_signature;
use xsh::runtime::value;
use xsh::sema;
use xsh::source;
use xsh::symbol;
use xsh::syntax::{arena, cst, lexer, parser, token};
use xsh::trace;
use xsh_registry::errors as registry_errors;
use xsh_registry::signature as registry_signature;
use xsh_registry::types as registry_types;

#[derive(Clone, Copy)]
struct LayoutItem {
    group: &'static str,
    name: &'static str,
    size: usize,
    align: usize,
}

macro_rules! item {
    ($group:literal, $ty:path) => {
        LayoutItem {
            group: $group,
            name: stringify!($ty),
            size: size_of::<$ty>(),
            align: align_of::<$ty>(),
        }
    };
}

fn main() {
    let items = layout_items();
    println!("{{");
    println!("  \"kind\": \"layout-baseline\",");
    println!("  \"target_arch\": \"{}\",", std::env::consts::ARCH);
    println!("  \"target_os\": \"{}\",", std::env::consts::OS);
    println!("  \"pointer_width\": {},", usize::BITS);
    println!("  \"items\": [");
    for (index, item) in items.iter().enumerate() {
        let suffix = if index + 1 == items.len() { "" } else { "," };
        println!(
            "    {{\"group\":\"{}\",\"name\":\"{}\",\"size\":{},\"align\":{}}}{suffix}",
            item.group, item.name, item.size, item.align
        );
    }
    println!("  ]");
    println!("}}");
}

fn layout_items() -> Vec<LayoutItem> {
    vec![
        item!("registry", registry_types::BuiltinTypeName),
        item!("registry", registry_types::Type),
        item!("registry", xsh_registry::RuntimeOp),
        item!("registry", registry_signature::ApiArgCheck),
        item!("registry", registry_signature::MethodReceiver),
        item!("registry", registry_signature::ApiSpec),
        item!("registry", registry_signature::ModuleEntry),
        item!("registry", registry_signature::ModuleSig),
        item!("registry", registry_signature::NamedModuleFns),
        item!("registry", registry_signature::ModuleFnSig),
        item!("registry", registry_signature::MethodSig),
        item!("registry", registry_signature::MethodReturn),
        item!("registry", registry_signature::ParamSig),
        item!("registry", registry_signature::MethodReceiverSig),
        item!("registry", registry_signature::NamedMethodSigs),
        item!("registry", registry_errors::ErrorField),
        item!("registry", registry_errors::ErrorVariant),
        item!("registry", registry_errors::ErrorFamily),
        item!("main-signature", main_signature::ApiSpec),
        item!("main-signature", main_signature::ModuleEntry),
        item!("main-signature", main_signature::ModuleSig),
        item!("main-signature", main_signature::NamedModuleFns),
        item!("main-signature", main_signature::ModuleFnSig),
        item!("main-signature", main_signature::MethodSig),
        item!("main-signature", main_signature::MethodReturn),
        item!("main-signature", main_signature::ParamSig),
        item!("main-signature", main_signature::MethodReceiverSig),
        item!("main-signature", main_signature::NamedMethodSigs),
        item!("sema", sema::types::Type),
        item!("sema", sema::types::ModuleExportType),
        item!("sema", sema::types::CallableType),
        item!("sema", sema::types::CallableParamType),
        item!("sema", sema::check::CompactDeclOutput),
        item!("sema", sema::check::CompactBodyProbeOutput),
        item!("sema", sema::check::CompactTypeDefInfo),
        item!("sema", sema::check::CompactFunctionSig),
        item!("source", source::SourceId),
        item!("source", source::Span),
        item!("source", source::SourceLocation),
        item!("symbol", symbol::Name),
        item!("symbol", symbol::QualifiedName),
        item!("syntax-arena", arena::SpanId),
        item!("syntax-arena", arena::StmtId),
        item!("syntax-arena", arena::BlockId),
        item!("syntax-arena", arena::ExprId),
        item!("syntax-arena", arena::PatternId),
        item!("syntax-arena", arena::BindingTargetId),
        item!("syntax-arena", arena::AssignTargetId),
        item!("syntax-arena", arena::TypeExprId),
        item!("syntax-arena", arena::RunFormId),
        item!("syntax-arena", arena::BuilderBlockId),
        item!("syntax-arena", arena::UseStmtId),
        item!("syntax-arena", arena::TypeDefId),
        item!("syntax-arena", arena::ErrorDefId),
        item!("syntax-arena", arena::FunctionDefId),
        item!("syntax-arena", arena::SignalHookId),
        item!("syntax-arena", arena::CommandStmtId),
        item!("syntax-arena", arena::IntLiteralId),
        item!("syntax-arena", arena::FloatLiteralId),
        item!("syntax-arena", arena::DurationLiteralId),
        item!("syntax-arena", arena::StringLiteralId),
        item!("syntax-arena", arena::BytesLiteralId),
        item!("syntax-arena", arena::TextLiteralId),
        item!("syntax-arena", arena::ArenaRange),
        item!("syntax-arena", arena::ArenaByteSpan),
        item!("syntax-arena", arena::ArenaSpanSource),
        item!("syntax-arena", arena::ArenaTypeExprTag),
        item!("syntax-arena", arena::ArenaTypeExprData),
        item!("syntax-arena", arena::ArenaProgram),
        item!("syntax-arena", arena::ArenaProgramBuilder),
        item!("syntax-arena", arena::AstArena),
        item!("syntax-arena", arena::ArenaStats),
        item!("syntax-arena", arena::ArenaUserModule),
        item!("syntax-arena", arena::ArenaStmt),
        item!("syntax-arena", arena::ArenaStmtTag),
        item!("syntax-arena", arena::ArenaStmtData),
        item!("syntax-arena", arena::ArenaStmtKind),
        item!("syntax-arena", arena::ArenaUseStmt),
        item!("syntax-arena", arena::ArenaTypeDef),
        item!("syntax-arena", arena::ArenaTypeDefBody),
        item!("syntax-arena", arena::ArenaSchemaField),
        item!("syntax-arena", arena::ArenaModuleContractEntry),
        item!("syntax-arena", arena::ArenaModuleContractEntryKind),
        item!("syntax-arena", arena::ArenaTagVariant),
        item!("syntax-arena", arena::ArenaErrorDef),
        item!("syntax-arena", arena::ArenaErrorVariant),
        item!("syntax-arena", arena::ArenaErrorField),
        item!("syntax-arena", arena::ArenaBlock),
        item!("syntax-arena", arena::ArenaExpr),
        item!("syntax-arena", arena::ArenaExprTag),
        item!("syntax-arena", arena::ArenaExprData),
        item!("syntax-arena", arena::ArenaExprKind),
        item!("syntax-arena", arena::ArenaPattern),
        item!("syntax-arena", arena::ArenaPatternKind),
        item!("syntax-arena", arena::ArenaBindingTarget),
        item!("syntax-arena", arena::ArenaBindingTargetKind),
        item!("syntax-arena", arena::ArenaDestructureField),
        item!("syntax-arena", arena::ArenaAssignTarget),
        item!("syntax-arena", arena::ArenaAssignTargetKind),
        item!("syntax-arena", arena::ArenaRunForm),
        item!("syntax-arena", arena::ArenaRunSegment),
        item!("syntax-arena", arena::ArenaText),
        item!("syntax-arena", arena::ArenaTextTag),
        item!("syntax-arena", arena::ArenaTextData),
        item!("syntax-arena", arena::ArenaFmtPart),
        item!("syntax-arena", arena::ArenaFmtPartTag),
        item!("syntax-arena", arena::ArenaFmtPartData),
        item!("syntax-arena", arena::ArenaCommandArg),
        item!("syntax-arena", arena::ArenaCommandArgKind),
        item!("syntax-arena", arena::ArenaWordPart),
        item!("syntax-arena", arena::ArenaWordPartTag),
        item!("syntax-arena", arena::ArenaWordPartData),
        item!("syntax-arena", arena::ArenaCallArg),
        item!("syntax-arena", arena::ArenaCallArgKind),
        item!("syntax-arena", arena::ArenaBuilderBlock),
        item!("syntax-arena", arena::ArenaBuilderEntry),
        item!("syntax-arena", arena::ArenaBuilderEntryKind),
        item!("syntax-arena", arena::ArenaFunctionDef),
        item!("syntax-arena", arena::ArenaSignalHook),
        item!("syntax-arena", arena::ArenaCommandStmt),
        item!("syntax-token", token::TokenKind),
        item!("syntax-token", token::TokenId),
        item!("syntax-token", token::TokenTag),
        item!("syntax-token", token::TokenPayload),
        item!("syntax-token", token::TokenTable),
        item!("syntax-token", token::TokenTableData),
        item!("syntax-token", token::TokenTableBuilder),
        item!("syntax-token", lexer::CompactLexerOutput),
        item!("syntax-parser", parser::ArenaParseOutput),
        item!("syntax-cst", cst::SyntaxNodeId),
        item!("syntax-cst", cst::SyntaxTokenId),
        item!("syntax-cst", cst::SyntaxTriviaId),
        item!("syntax-cst", cst::SyntaxKind),
        item!("syntax-cst", cst::SyntaxGroupKind),
        item!("syntax-cst", cst::SyntaxElement),
        item!("syntax-cst", cst::SyntaxNode),
        item!("syntax-cst", cst::SyntaxToken),
        item!("syntax-cst", cst::SyntaxTrivia),
        item!("syntax-cst", cst::TriviaKind),
        item!("syntax-cst", cst::SyntaxTree),
        item!("runtime", value::RecordMap),
        item!("runtime", value::SparseRecordMap),
        item!("runtime", value::Value),
        item!("runtime", xsh::runtime::eval::CompactLowerProbeOutput),
        item!("runtime", xsh::runtime::eval::CompactLowerBodyProbeOutput),
        item!(
            "runtime",
            xsh::runtime::eval::CompactLowerConstructProbeOutput
        ),
        item!("runtime", xsh::runtime::eval::CompactRuntimeDeclProbeOutput),
        item!("runtime", value::FloatValue),
        item!("runtime", value::DurationValue),
        item!("runtime", value::DigestValue),
        item!("runtime", value::RegexValue),
        item!("runtime", value::PathValue),
        item!("runtime", value::ResultValue),
        item!("runtime", value::CommandPlan),
        item!("runtime", value::ProcessHandleValue),
        item!("runtime", value::StreamValue),
        item!("trace", trace::TraceEvent),
        item!("trace", trace::TraceKind),
        item!("trace", trace::TraceTiming),
        item!("trace", trace::TracePayload),
        item!("trace", trace::TraceArg),
        item!("trace", trace::TraceEnv),
        item!("trace", trace::TraceStatus),
        item!("trace", trace::TraceStatusKind),
        item!("trace", trace::TraceError),
        item!("trace", trace::Traceback),
        item!("trace", trace::TracebackFrame),
        item!("trace", trace::TracebackFrameKind),
    ]
}
