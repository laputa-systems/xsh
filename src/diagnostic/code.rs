//! The closed set of diagnostic codes.
//!
//! A code's string name (such as `check.type-mismatch`) is a user-visible
//! contract: `xsht lint --only` selects by it, documentation and tests cite
//! it, and rendered and machine-readable diagnostics print it. Each code is
//! declared exactly once in the table below, together with its family, its
//! default severity, whether `xsht lint` may apply its fix, and a one-line
//! summary, so the enum, its name lookup and every catalog derived from it
//! cannot drift apart.

use super::Severity;

/// The pipeline stage or tool that reports a code; every code name begins
/// with its family prefix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticFamily {
    /// Reading source files before lexing.
    Source,
    Lex,
    Parse,
    Check,
    /// Lowering a checked program into the compact executable form.
    Compact,
    /// Failures reported as diagnostics while evaluating a program.
    Runtime,
    /// `xsht lint` quality checks; every lint code is selectable with
    /// `xsht lint --only`.
    Lint,
    /// `xsht fmt` refusing a rewrite that would change the program.
    Format,
}

impl DiagnosticFamily {
    /// The leading segment shared by every code name in this family.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Lex => "lex",
            Self::Parse => "parse",
            Self::Check => "check",
            Self::Compact => "compact",
            Self::Runtime => "runtime",
            Self::Lint => "lint",
            Self::Format => "format",
        }
    }
}

/// Maps a table severity keyword to the code's default severity; `mixed`
/// marks a code whose emission sites choose the severity per finding.
macro_rules! code_severity {
    (error) => {
        Some(Severity::Error)
    };
    (warning) => {
        Some(Severity::Warning)
    };
    (mixed) => {
        None
    };
}

macro_rules! code_fixable {
    () => {
        false
    };
    (fixable) => {
        true
    };
}

/// Declares `DiagnosticCode` and its property tables from one list. Each
/// entry is `Variant = "name", severity [, fixable], "summary";`, grouped
/// under its family.
macro_rules! diagnostic_codes {
    ($($family:ident {
        $($variant:ident = $name:literal, $severity:ident $(, $fixable:ident)?, $summary:literal;)*
    })*) => {
        /// A stable diagnostic code. Serialized and displayed as its
        /// [`name`](Self::name).
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum DiagnosticCode {
            $($($variant,)*)*
        }

        impl DiagnosticCode {
            /// Every code, grouped by family in declaration order.
            pub const ALL: &[Self] = &[$($(Self::$variant,)*)*];

            /// The stable user-visible name.
            pub const fn name(self) -> &'static str {
                match self {
                    $($(Self::$variant => $name,)*)*
                }
            }

            /// The code with this exact stable name.
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($($name => Some(Self::$variant),)*)*
                    _ => None,
                }
            }

            /// One line describing what the code reports, for catalogs such as
            /// `xsht lint --list`.
            pub const fn summary(self) -> &'static str {
                match self {
                    $($(Self::$variant => $summary,)*)*
                }
            }

            pub const fn family(self) -> DiagnosticFamily {
                match self {
                    $($(Self::$variant => DiagnosticFamily::$family,)*)*
                }
            }

            /// The severity every emission site uses, or `None` when sites
            /// choose it per finding.
            pub const fn severity(self) -> Option<Severity> {
                match self {
                    $($(Self::$variant => code_severity!($severity),)*)*
                }
            }

            /// Whether a non-lint code's diagnostics carry a safe source fix
            /// that `xsht lint --fix` applies, which makes the code selectable
            /// with `xsht lint --only` for a scoped migration.
            pub const fn fixable(self) -> bool {
                match self {
                    $($(Self::$variant => code_fixable!($($fixable)?),)*)*
                }
            }
        }
    };
}

impl DiagnosticCode {
    /// Whether `xsht lint --only` accepts this code: every lint code, plus
    /// the fixable codes of other stages.
    pub const fn lint_selectable(self) -> bool {
        matches!(self.family(), DiagnosticFamily::Lint) || self.fixable()
    }

    /// The candidate whose name `name` most plausibly misspells, for a
    /// "did you mean" hint on an unknown code.
    pub fn nearest(name: &str, candidates: impl Iterator<Item = Self>) -> Option<Self> {
        crate::sema::check::nearest_name(name, candidates.map(Self::name))
            .and_then(|nearby| Self::from_name(&nearby))
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

diagnostic_codes! {
    Source {
        SourceInvalidUtf8 = "source.invalid-utf8", error, "Reject a source file that is not valid UTF-8";
    }
    Lex {
        LexInvalidBytesEscape = "lex.invalid-bytes-escape", error, "Reject a `\\u` unicode escape inside a bytes literal";
        LexInvalidEscape = "lex.invalid-escape", error, "Reject an unsupported escape sequence in a string, bytes, or interpolated literal";
        LexInvalidFloat = "lex.invalid-float", error, "Reject a float literal whose exponent has no digits";
        LexInvalidOctal = "lex.invalid-octal", error, "Reject an octal literal with non-octal digits or no digits after `0o`";
        LexInvalidString = "lex.invalid-string", error, "Reject a string literal whose bytes are not valid UTF-8";
        LexUnexpectedCharacter = "lex.unexpected-character", error, "Reject a character not valid in source, including `'...'` strings and `$(...)`";
        LexUnterminatedBytes = "lex.unterminated-bytes", error, "Reject a bytes literal with no closing quote";
        LexUnterminatedFmtString = "lex.unterminated-fmt-string", error, "Reject an f-string literal with no closing quote";
        LexUnterminatedPathFmtString = "lex.unterminated-path-fmt-string", error, "Reject an `fp` path f-string literal with no closing quote";
        LexUnterminatedString = "lex.unterminated-string", error, "Reject a string literal with no closing quote";
    }
    Parse {
        ParseBlockHeaderMigration = "parse.block-header-migration", error, "Reject error-handler parameters written before the block instead of inside it";
        ParseBlockParams = "parse.block-params", error, "Reject a block that has two parameter headers";
        ParseBlockStringMargin = "parse.block-string-margin", error, "Reject a block string line that lacks the closing delimiter's exact indentation";
        ParseCaptureMode = "parse.capture-mode", error, "Reject a `run` capture mode other than `--text` or `--bytes`";
        ParseCaptureModeInterpolation = "parse.capture-mode-interpolation", error, "Reject an interpolated `run` capture mode instead of a literal `--text` or `--bytes`";
        ParseCdArity = "parse.cd-arity", error, "Reject `cd` with more than one path argument";
        ParseCliEntryScope = "parse.cli-entry-scope", error, "Reject `cli main` declared anywhere but the entry module's top level";
        ParseCommandCallExpr = "parse.command-call-expr", error, "Reject a call expression used as a command argument";
        ParseCompatibilityVocabulary = "parse.compatibility-vocabulary", error, "Reject a removed compatibility spelling such as `run.builtin`";
        ParseDetachedElse = "parse.detached-else", error, "Reject `else` or `else if` on a new line after the closing `}` of an `if` block";
        ParseEmptyEnum = "parse.empty-enum", error, "Reject an `enum` declaration with no variants";
        ParseEnumBacking = "parse.enum-backing", error, "Reject a wire enum backed by any type other than `Str`";
        ParseEnumMigration = "parse.enum-migration", error, "Reject a tagged-union `type` declaration in favor of `enum Name { A, B }`";
        ParseEnumWirePayload = "parse.enum-wire-payload", error, "Reject payload fields on a `Str`-backed enum variant";
        ParseEnvAssignment = "parse.env-assignment", error, "Reject an `env` block entry that is not a `NAME=value` assignment";
        ParseEnvScopeMigration = "parse.env-scope-migration", error, "Reject an expression `env { ... }` without an explicit `env ({NAME: value}) { ... }` overlay";
        ParseErrorVariant = "parse.error-variant", error, "Reject an `error` definition with a missing or malformed variant";
        ParseExpectedBuilderEntry = "parse.expected-builder-entry", error, "Reject a builder block entry that is not a valid entry form";
        ParseExpectedCommand = "parse.expected-command", error, "Reject a missing command name where a command is expected";
        ParseExpectedCommandArg = "parse.expected-command-arg", error, "Reject a missing command argument where one is required";
        ParseExpectedExpression = "parse.expected-expression", error, "Reject a missing expression, including `$name` and `${...}` in expression context";
        ParseExpectedIdent = "parse.expected-ident", error, "Reject a missing identifier or name where one is required";
        ParseExpectedKeyword = "parse.expected-keyword", error, "Reject a missing required keyword";
        ParseExpectedLabel = "parse.expected-label", error, "Reject a missing field or argument label name";
        ParseExpectedParamType = "parse.expected-param-type", error, "Reject a parameter that has neither a `: Type` annotation nor a default value";
        ParseExpectedPattern = "parse.expected-pattern", error, "Reject a missing pattern where one is expected";
        ParseExpectedRecordUpdateValue = "parse.expected-record-update-value", error, "Reject a dotted record update path without `:` and a replacement value";
        ParseExpectedStreamStage = "parse.expected-stream-stage", error, "Reject a missing stream stage name";
        ParseExpectedTerminator = "parse.expected-terminator", error, "Reject a statement that is not followed by a terminator";
        ParseExpectedToken = "parse.expected-token", error, "Reject a missing required token such as an assignment operator or the `{` after `cd PATH`";
        ParseExpectedWord = "parse.expected-word", error, "Reject a command word that is not a literal word";
        ParseEnvStringName = "parse.env-string-name", error, "Reject an `e\"...\"` literal whose contents are not one identifier-shaped environment variable name";
        ParseExportTarget = "parse.export-target", error, "Reject `export` applied to anything but a definition";
        ParseExprStringInterpolation = "parse.expr-string-interpolation", error, "Reject interpolation in an expression string literal";
        ParseFallbackBlockParams = "parse.fallback-block-params", error, "Reject an error fallback block with no error parameter";
        ParseFmtDollarInterpolation = "parse.fmt-dollar-interpolation", error, "Reject `${expr}` in an f-string where `{expr}` interpolates";
        ParseFmtEmptyInterpolation = "parse.fmt-empty-interpolation", error, "Reject an empty `{}` interpolation in an f-string";
        ParseFmtInterpolationComment = "parse.fmt-interpolation-comment", error, "Reject a comment inside an f-string interpolation";
        ParseFmtInterpolationLineBreak = "parse.fmt-interpolation-line-break", error, "Reject a line break inside an interpolation of a single-line f-string";
        ParseFmtInterpolationTrailing = "parse.fmt-interpolation-trailing", error, "Reject unexpected tokens after an f-string interpolation expression";
        ParseFmtLoneBrace = "parse.fmt-lone-brace", error, "Reject an unmatched `}` in an f-string";
        ParseFmtSpec = "parse.fmt-spec", error, "Reject an f-string format spec other than `:>N`, `:<N`, or `:0N`";
        ParseForeignSyntax = "parse.foreign-syntax", error, "Reject syntax from other languages such as `++`, `catch`, `? :`, `elif`, or `[ ... ]` tests";
        ParseGenericErrorFamily = "parse.generic-error-family", error, "Reject generic parameters on an `error` family declaration";
        ParseIfExpressionElse = "parse.if-expression-else", error, "Reject an `if` expression without an `else` branch";
        ParseInferredVariantArm = "parse.inferred-variant-arm", error, "Reject a match arm head that begins with a target-typed `.Name` variant";
        ParseKeywordLabelBinding = "parse.keyword-label-binding", error, "Reject a field label that is a keyword used as an implicit binding name";
        ParseLegacyStderrRedirection = "parse.legacy-stderr-redirection", error, "Reject the legacy stderr redirection spelling in favor of `2>` or `2>>`";
        ParseLineContinuation = "parse.line-continuation", error, "Reject a `\\` line continuation that is not between the parts of a command";
        ParseListPatternRest = "parse.list-pattern-rest", error, "Reject a list pattern whose rest element is repeated or not last";
        ParseMapComprehensionEntries = "parse.map-comprehension-entries", error, "Reject entries written before a map comprehension in the same braces";
        ParseMatchElseArm = "parse.match-else-arm", error, "Reject an `else` match arm that has a guard or is not the last arm";
        ParseMixedComparison = "parse.mixed-comparison", error, "Reject ordering mixed with equality, membership, or pattern tests without parentheses";
        ParseModuleCycle = "parse.module-cycle", error, "Reject a module import that forms a cycle";
        ParseModuleLoad = "parse.module-load", error, "Reject an imported module that has parse errors";
        ParseModuleRead = "parse.module-read", error, "Reject an imported module that cannot be found or read";
        ParsePathStringInterpolation = "parse.path-string-interpolation", error, "Reject interpolation in a `p\"...\"` path string";
        ParsePatternAliasName = "parse.pattern-alias-name", error, "Reject the discard name `_` as a pattern alias after `as`";
        ParsePatternTestAlternation = "parse.pattern-test-alternation", error, "Reject an ungrouped alternation in a pattern test; write `(P | Q)`";
        ParsePipelineHole = "parse.pipeline-hole", error, "Reject a value pipeline call without exactly one `_` argument placeholder";
        ParsePipelineRun = "parse.pipeline-run", error, "Reject a byte pipeline segment that does not start with `run`";
        ParseRequiredReturn = "parse.required-return", error, "Reject a `stream` producer without a return type annotation";
        ParseRequiredSignature = "parse.required-signature", error, "Reject a function or `stream` producer declared without a parameter list";
        ParseRequiredValue = "parse.required-value", error, "Reject `yield` without a value";
        ParseRestDefault = "parse.rest-default", error, "Reject a default value on a rest parameter";
        ParseRunOption = "parse.run-option", error, "Reject a duplicate `run` option such as `--timeout`";
        ParseSignalHook = "parse.signal-hook", error, "Reject a malformed `on SIGNAL` hook: missing signal, effect list, or bad option";
        ParseStdlibCatalog = "parse.stdlib-catalog", error, "Reject an import of a standard-library module missing from the embedded catalog";
        ParseStreamMode = "parse.stream-mode", error, "Reject a `run.stream` mode other than `--text` or `--bytes`";
        ParseStreamOptionMigration = "parse.stream-option-migration", error, "Reject stream stage flags in favor of ordinary named arguments";
        ParseTestNested = "parse.test-nested", error, "Reject a `test` declaration that is not at the top level";
        ParseTestParams = "parse.test-params", error, "Reject a `test` with more than one parameter or one that is not an immutable `TestContext`";
        ParseUnknownEffect = "parse.unknown-effect", error, "Reject an unknown effect name in an effect list";
        ParseUnknownRunForm = "parse.unknown-run-form", error, "Reject an unknown `run.NAME` form";
        ParseUnknownStreamStage = "parse.unknown-stream-stage", error, "Reject an unknown stream stage name";
        ParseUnsupportedBooleanOperator = "parse.unsupported-boolean-operator", error, "Reject `&&`, `||`, `|`, or `&` in favor of the `and` and `or` word forms";
        ParseUnsupportedIntegerDivision = "parse.unsupported-integer-division", error, "Reject a `//` or `div` integer-division operator; use `/` on Int operands";
        ParseUnsupportedThen = "parse.unsupported-then", error, "Reject the `then` keyword after an `if`, `while`, or `for` head";
        ParseUnterminatedInterpolation = "parse.unterminated-interpolation", error, "Reject a string interpolation or f-string `{` with no closing delimiter";
        ParseForIndex = "parse.for-index", error, "Reject a destructured index in `for INDEX, ITEM in SOURCE`";
        ParseNestingDepth = "parse.nesting-depth", error, "Reject a construct nested more deeply than the limit every later pass is sized for";
    }
    Check {
        CheckAcceptPolicy = "check.accept-policy", error, "Reject an invalid `accept` exit-code list: empty, outside 0..255, or with duplicates";
        CheckAmbiguousGrouping = "check.ambiguous-grouping", error, fixable, "Group an `if` or `match` operand, or a pipeline that an operator applies to";
        CheckAmbiguousOverload = "check.ambiguous-overload", error, "Reject a standard API call that matches more than one overload";
        CheckArgvConversion = "check.argv-conversion", error, "Reject a value that cannot convert to a command argument or argv item";
        CheckArity = "check.arity", error, "Reject a call, stream stage, or standard API call with the wrong number of arguments";
        CheckAssertCondition = "check.assert-condition", error, "Reject an `assert` condition that is not `Bool`";
        CheckAssertMessage = "check.assert-message", error, "Reject an `assert` message that is not `Str`";
        CheckAssignLet = "check.assign-let", error, "Reject assignment to a binding that is not declared with `var`";
        CheckAssignTarget = "check.assign-target", error, "Reject field or index assignment on a value that is not a record, `List`, or `Map`";
        CheckBarePrintIdent = "check.bare-print-ident", error, "Reject bare identifiers, bare words, and field access in `print` without `$` or quotes";
        CheckBlockParams = "check.block-params", error, "Reject parameters on a block that does not receive any";
        CheckBoolStatement = "check.bool-statement", error, fixable, "Suggest `assert` for a Bool expression used as a statement, or `let _ =` to discard";
        CheckBuilderCall = "check.builder-call", error, "Reject a builder block on a call that does not accept one";
        CheckBuilderCheck = "check.builder-check", error, "Reject a `process.command` builder block without exactly one `run` entry";
        CheckBuilderEntry = "check.builder-entry", error, "Reject an unknown builder entry, task, or statement in a builder block";
        CheckBuilderField = "check.builder-field", error, "Reject a duplicate or unknown builder field, or a non-positive `cpu_max`";
        CheckBytesChunks = "check.bytes-chunks", error, "Reject a non-positive `size` for a bytes `chunks` stage";
        CheckCallSplice = "check.call-splice", error, "Reject an `@` splice in a call argument position that does not allow one";
        CheckCallTarget = "check.call-target", error, "Reject a call to an unresolved target or a local that shadows a function";
        CheckCallableAliasExport = "check.callable-alias-export", error, "Reject exporting a callable alias without an explicit return and effect contract";
        CheckCliDescriptor = "check.cli-descriptor", error, "Reject an invalid `cli` descriptor or command table";
        CheckCliEntry = "check.cli-entry", error, "Reject an invalid signature `cli main` entry declaration";
        CheckCommandPure = "check.command-pure", error, "Reject calling a `pure` function with command syntax";
        CheckCommandWordConversion = "check.command-word-conversion", error, "Reject a command word that cannot convert to the declared parameter type";
        CheckCompatibilityVocabulary = "check.compatibility-vocabulary", error, "Reject a removed compatibility name and suggest its canonical replacement";
        CheckConst = "check.const", error, "Reject a `const` declaration that cannot be evaluated to a constant at check time";
        CheckConstructorInference = "check.constructor-inference", error, "Reject a record constructor whose type parameters cannot be inferred";
        CheckContextScopeEscape = "check.context-scope-escape", error, "Reject a live producer or host handle escaping its context scope";
        CheckContextScopeInput = "check.context-scope-input", error, "Reject a `cwd` or `env` context scope input of the wrong type";
        CheckCoreCdArity = "check.core-cd-arity", error, "Reject `cd` without exactly one path argument";
        CheckCoreCommandShadow = "check.core-command-shadow", error, "Reject a `proc` named like a core command";
        CheckCoreEnvArity = "check.core-env-arity", error, "Reject positional arguments to `env`, which accepts only assignments";
        CheckCpumax = "check.cpumax", error, "Reject a non-positive `--cpumax` value";
        CheckDefaultParam = "check.default-param", error, "Reject a required parameter that follows a defaulted parameter";
        CheckDeferControlFlow = "check.defer-control-flow", error, "Reject `break`, `continue`, `return`, or `yield` leaving a `defer` cleanup block";
        CheckDeferType = "check.defer-type", error, "Reject a `defer` cleanup that does not produce `Unit`, `Status`, or `Result[Unit]`";
        CheckDestructureField = "check.destructure-field", error, "Reject a duplicate or unknown field in record destructuring";
        CheckDestructureType = "check.destructure-type", error, "Reject record destructuring of a non-record value or an unchecked `Any`";
        CheckDesugar = "check.desugar", error, "Reject pipeline sugar that reached the checker without being desugared";
        CheckDisplayConversion = "check.display-conversion", error, "Reject a value that cannot be displayed by `print`, an f-string, or a context value";
        CheckDuplicateModuleDoc = "check.duplicate-module-doc", error, "Reject more than one `##!` module doc comment";
        CheckDuplicateName = "check.duplicate-name", error, "Reject a duplicate or conflicting name in a scope, module, or top level";
        CheckDuplicateRecordField = "check.duplicate-record-field", error, "Reject a duplicate field in a record literal, schema, or error payload";
        CheckDuplicateSignalHook = "check.duplicate-signal-hook", error, "Reject registering the same signal hook more than once";
        CheckDurationLiteral = "check.duration-literal", error, "Reject a duration literal that overflows the maximum millisecond count";
        CheckDynamicBoundary = "check.dynamic-boundary", error, fixable, "Validate an `Any` value with `.require(T)?` where the context names its concrete type";
        CheckEachTail = "check.each-tail", error, "Reject an `each` or `tee` block with the wrong result type";
        CheckEffectViolation = "check.effect-violation", error, "Reject an operation that requires an effect the enclosing callable does not declare";
        CheckEmptyMatch = "check.empty-match", error, "Reject a `match` expression with no arms";
        CheckEnumWireMapping = "check.enum-wire-mapping", error, "Reject an invalid Str-backed enum variant wire mapping";
        CheckEnvName = "check.env-name", error, "Reject an environment assignment name that is not an identifier";
        CheckEnvValue = "check.env-value", error, "Reject an environment value that is not a single scalar value";
        CheckErrArguments = "check.err-arguments", error, "Reject `Err` arguments that are not a positional error and an optional named `cause`";
        CheckErrorCause = "check.error-cause", warning, "Warn when an error translation keeps only `.message` and drops the typed `cause`";
        CheckErrorConstructor = "check.error-constructor", error, "Reject an error constructor with an unknown variant, unknown, duplicate, or missing field";
        CheckErrorRemoved = "check.error-removed", error, "Reject removed error APIs: `.kind`, `Error(kind: ...)`, and record matching on error fields";
        CheckExportDestructure = "check.export-destructure", error, "Reject exporting a destructuring binding";
        CheckFallbackBlockContext = "check.fallback-block-context", error, "Reject a parameter value block outside a `Result` fallback";
        CheckFallbackBlockParams = "check.fallback-block-params", error, "Reject an error fallback block that does not take exactly one parameter";
        CheckFallbackBlockResult = "check.fallback-block-result", error, "Reject an error fallback block whose left side is not a `Result`";
        CheckFieldAccess = "check.field-access", error, "Reject field access on a value that is not record-like";
        CheckFlatMap = "check.flat-map", error, "Reject a `flat-map` block that does not produce a `List` or `Stream`";
        CheckFmtDollarName = "check.fmt-dollar-name", error, "Reject `$name` in an f-string, which interpolates with `{name}`";
        CheckForIterator = "check.for-iterator", error, "Reject a `for` loop over a value that is not a `List`, `Stream`, `Map`, `Str`, or `Bytes`";
        CheckGuardBinding = "check.guard-binding", error, "Reject a `guard let` binding that is neither a `Result` nor an optional value";
        CheckGuardFallthrough = "check.guard-fallthrough", error, "Reject a `guard` else block that can fall through to the enclosing code";
        CheckHandlerBlockParams = "check.handler-block-params", error, "Reject an error handler block with more than one parameter";
        CheckHyphenatedModuleAlias = "check.hyphenated-module-alias", error, "Reject a hyphenated module path segment imported without an `as` alias";
        CheckIfCondition = "check.if-condition", error, "Reject an `if` condition that is not `Bool` or `Status`";
        CheckIfValueElse = "check.if-value-else", error, "Reject a value-producing `if` without an `else` branch";
        CheckIgnoredResult = "check.ignored-result", error, fixable, "Discard a value a statement would otherwise drop silently with `let _ =`";
        CheckIndexType = "check.index-type", error, "Reject indexing a value that is not a `List` or record";
        CheckIndexOutOfRange = "check.index-out-of-range", error, "Reject a negative literal index that reaches past the start of a list literal";
        CheckInferParam = "check.infer-param", error, "Reject a parameter default that does not establish a concrete type without an annotation";
        CheckInferReturn = "check.infer-return", error, "Reject a function whose return shape is underdetermined or inferred inconsistently across paths";
        CheckInferredVariant = "check.inferred-variant", error, "Reject a leading-dot variant whose expected type names no single enum or error family with that variant";
        CheckIntLiteral = "check.int-literal", error, "Reject an integer literal outside the 64-bit signed range";
        CheckIrrefutablePatternCondition = "check.irrefutable-pattern-condition", error, "Reject a pattern condition that cannot fail over a subject that is not optional, instead of binding with `let`";
        CheckJsonCompatible = "check.json-compatible", error, "Reject a value that is not JSON-compatible, such as `Path`, `Bytes`, `Status`, or `Result`";
        CheckLastStatus = "check.last-status", error, "Reject `$?` read before any status has been set";
        CheckLegacyTestProc = "check.legacy-test-proc", error, "Reject a legacy `test_` proc and require migrating it to a `test NAME { |ctx| ... }` declaration";
        CheckListSpliceType = "check.list-splice-type", error, "Reject a list literal splice whose operand is not a `List`";
        CheckListcompCondition = "check.listcomp-condition", error, "Reject a comprehension `if` condition that is not `Bool` or `Status`";
        CheckListcompIterator = "check.listcomp-iterator", error, "Reject a comprehension that iterates over a value that is not a List, Stream, Map, Str, or Bytes";
        CheckLocalInference = "check.local-inference", error, "Reject a local or intermediate value whose type cannot be inferred and needs an annotation";
        CheckLoopControl = "check.loop-control", error, "Reject `break` or `continue` outside a `while` or `for` loop or targeting a stream stage";
        CheckLoopNoBreak = "check.loop-no-break", error, "Reject a `loop` with no `break` that would run forever";
        CheckMapKey = "check.map-key", error, "Reject a built-in method call that instantiates a `Map` with an unsupported key type";
        CheckMapKeyType = "check.map-key-type", error, "Reject a `Map` key type that is not an ordered scalar such as `Str`, `Int`, or `Path`";
        CheckMapSpreadType = "check.map-spread-type", error, "Reject a map literal spread whose operand is not a `Map`";
        CheckMapTail = "check.map-tail", error, "Reject a `map` or `par-map` stage block that has no tail value";
        CheckMapUpdatePath = "check.map-update-path", error, "Reject record update paths or computed map keys where they are not permitted";
        CheckMapcompCondition = "check.mapcomp-condition", error, "Reject a map comprehension `if` condition that is not `Bool` or `Status`";
        CheckMapcompIterator = "check.mapcomp-iterator", error, "Reject a map comprehension that iterates over a value that is not a List, Stream, Map, Str, or Bytes";
        CheckMatchValueExhaustive = "check.match-value-exhaustive", error, "Reject a value-producing `match` that does not cover every case";
        CheckMembershipType = "check.membership-type", error, "Reject an `in` test whose container or element type does not support membership";
        CheckMigrationError = "check.migration-error", warning, "Warn that `ProcessError(...)` is not a source constructor when migration diagnostics are on";
        CheckMissingModuleDoc = "check.missing-module-doc", error, "Reject an exported module that has no preceding `##!` module doc comment";
        CheckMissingPublicDoc = "check.missing-public-doc", error, "Reject an exported declaration that has no preceding `##` doc comment";
        CheckMissingReturn = "check.missing-return", error, "Reject a function that can fall through without returning its declared type";
        CheckMixedLogical = "check.mixed-logical", error, fixable, "Group `and` mixed with `or`, or `??` mixed with either, around the tighter operand";
        CheckModuleCommandArg = "check.module-command-arg", error, "Reject a splice argument in a module command";
        CheckModuleCommandFlag = "check.module-command-flag", error, "Reject an unknown, duplicate, or non-`Bool` module command flag";
        CheckModuleCommandValue = "check.module-command-value", error, "Reject module command syntax for an API that is not an effectful `Result[Unit]` command";
        CheckModuleContract = "check.module-contract", error, "Reject a `module contract` declaration that lists no exports";
        CheckModuleMember = "check.module-member", error, "Reject use of a standard module as a value or access to a missing or uncalled module function";
        CheckModuleTopLevel = "check.module-top-level", error, "Reject top-level mutation or commands in an imported module";
        CheckNamedArg = "check.named-arg", error, "Reject a named argument that is unexpected, repeated, or required in a different form";
        CheckNamedSpread = "check.named-spread", error, "Reject named-argument spreading that cannot be checked against the callable signature";
        CheckNestedDeclaration = "check.nested-declaration", error, "Reject a declaration nested inside a block or function body";
        CheckNonExhaustiveMatch = "check.non-exhaustive-match", error, "Reject a statement `match` that misses list lengths, enum or error family variants, or union members";
        CheckNullSafeField = "check.null-safe-field", error, "Reject `?.` on a value that is not an `Optional` or `Result`";
        CheckNullSafeIndex = "check.null-safe-index", error, "Reject guarded indexing on a receiver that is not a checked `Optional` or `Result`";
        CheckOperatorType = "check.operator-type", error, "Reject an operator or compound assignment applied to operand types it does not support";
        CheckOptionalMethod = "check.optional-method", error, "Reject a method call on a nullable receiver or result without a `?.` hop";
        CheckOrphanDocComment = "check.orphan-doc-comment", error, "Reject a doc comment that does not precede an export or serve as the module `##!` doc";
        CheckPatternAlternativeBinding = "check.pattern-alternative-binding", error, "Reject pattern alternatives that bind different names or types";
        CheckPatternArity = "check.pattern-arity", error, "Reject a constructor or tag pattern with the wrong number of payload arguments";
        CheckPatternBinding = "check.pattern-binding", error, "Reject a duplicate name bound within a single pattern";
        CheckPatternCapitalizedBinding = "check.pattern-capitalized-binding", error, "Reject a capitalized pattern name that would bind a new name instead of matching a variant";
        CheckPatternConstructor = "check.pattern-constructor", error, "Reject an unknown constructor, error variant, or error facet in a pattern";
        CheckPatternField = "check.pattern-field", error, "Reject a duplicate or unknown field in a record or error payload pattern";
        CheckPatternRest = "check.pattern-rest", error, "Reject a list pattern rest that is not a wildcard or name";
        CheckPatternTestAlternation = "check.pattern-test-alternation", error, "Reject ungrouped alternatives in a pattern test";
        CheckPatternTestAmbiguous = "check.pattern-test-ambiguous", error, "Reject a pattern test name that matches more than one type or constructor";
        CheckPatternTestBinding = "check.pattern-test-binding", error, "Reject a pattern test that binds names or contains aliases";
        CheckPatternType = "check.pattern-type", error, "Reject a pattern whose kind does not match the type of the matched value";
        CheckPipelineCapture = "check.pipeline-capture", error, "Reject a capture form on a byte pipeline segment other than the first";
        CheckPipelineCpumax = "check.pipeline-cpumax", error, "Reject `--cpumax` on a byte pipeline segment other than the first";
        CheckPipelineHole = "check.pipeline-hole", error, "Reject `_` used as anything other than a whole-argument placeholder in an immediate pipeline call";
        CheckPipelineStage = "check.pipeline-stage", error, "Reject a value pipeline stage that is not a call";
        CheckPipelineStdin = "check.pipeline-stdin", error, "Reject stdin redirection on a byte pipeline segment other than the first";
        CheckPositionalErrorArguments = "check.positional-error-arguments", error, "Reject positional error constructor arguments that follow a named one or fill fields a single value fits two of";
        CheckProcCommandSyntax = "check.proc-command-syntax", error, "Reject calling a `proc` with command syntax instead of expression-call syntax";
        CheckProcessArgvEmpty = "check.process-argv-empty", error, "Reject an empty argv list in `process.command_argv`";
        CheckPublicResultError = "check.public-result-error", error, "Require an exported signature or module contract to spell the error type of each Result";
        CheckPureAssignment = "check.pure-assignment", error, "Reject assignment in a `pure` function to anything but its own local `var`";
        CheckPureCommand = "check.pure-command", error, "Reject a command statement inside a `pure` function";
        CheckPureDefer = "check.pure-defer", error, "Reject `defer` inside a `pure` function";
        CheckPureEffect = "check.pure-effect", error, "Reject an effectful call, lookup, or scope inside a `pure` function";
        CheckPureRun = "check.pure-run", error, "Reject a `run` form inside a `pure` function";
        CheckRecordConstructor = "check.record-constructor", error, "Reject a record constructor call with unnamed, duplicate, unknown, or missing fields";
        CheckRecordDefault = "check.record-default", error, "Reject a record field default that is not a literal or an immutable literal constant";
        CheckRecordUpdateBase = "check.record-update-base", error, "Reject a nested record update without exactly one leading record spread";
        CheckRecordUpdateField = "check.record-update-field", error, "Reject a record update target that is not an existing field of a known record";
        CheckRecordUpdateOverlap = "check.record-update-overlap", error, "Reject record update targets that overlap";
        CheckRecordUpdateShape = "check.record-update-shape", error, "Reject a nested record update on a record without a statically known shape";
        CheckRecordUpdateValue = "check.record-update-value", error, "Reject a record update replacement whose field type is not checked";
        CheckRecursiveType = "check.recursive-type", error, "Reject a recursive type alias or recursive generic schema application";
        CheckRedundantParens = "check.redundant-parens", error, fixable, "Remove parentheses that do not change the parse";
        CheckRegexLiteral = "check.regex-literal", error, "Reject a regex literal that does not compile";
        CheckRemovedMembership = "check.removed-membership", error, "Reject the removed standard membership API; use `in` or `not in`";
        CheckRemovedRecordRequire = "check.removed-record-require", error, "Reject the removed `record.require`; declare a schema and use `.require(Schema)`";
        CheckRequireTarget = "check.require-target", error, "Reject `.require` when no schema or typed boundary supplies the target type";
        CheckRequiredReturn = "check.required-return", error, "Reject an exported or recursive `pure` or boundary `proc` that needs a return annotation";
        CheckRestPosition = "check.rest-position", error, "Reject a rest parameter that is not the last parameter";
        CheckRestType = "check.rest-type", error, "Reject a rest parameter whose type is not a `List`";
        CheckResultFallback = "check.result-fallback", error, "Reject `??` on a non-Result/Optional value and `or` on a Result";
        CheckRetryPattern = "check.retry-pattern", error, "Reject a `retry` selection that is not a nominal error, facet, type, or wildcard pattern";
        CheckReturnOutsideCallable = "check.return-outside-callable", error, "Reject `return` outside a callable body";
        CheckRevealType = "check.reveal-type", mixed, "Report the type of an expression via `reveal_type`; reject it outside `xsht check`";
        CheckRunTarget = "check.run-target", error, "Reject a `run` target that splices an empty list literal, which names no program";
        CheckSchema = "check.schema", error, "Reject a record schema declared with no fields";
        CheckSchemaField = "check.schema-field", error, "Reject an unknown or missing field in a schema-checked record";
        CheckSignalHook = "check.signal-hook", error, "Reject an invalid signal hook declaration, placement, option, or body";
        CheckSignalHookModule = "check.signal-hook-module", error, "Reject a signal hook declared in a module instead of the entry script";
        CheckSizeLiteral = "check.size-literal", error, "Reject a size literal whose byte count exceeds the 64-bit signed range";
        CheckSliceType = "check.slice-type", error, "Reject slicing a value that is not a `List`, `Str`, or `Bytes`";
        CheckSpawnRunKind = "check.spawn-run-kind", error, "Reject `spawn run` with a form other than `run` or `run.status`";
        CheckSpawnRunShape = "check.spawn-run-shape", error, "Reject `spawn run` without exactly one run segment";
        CheckSpliceTarget = "check.splice-target", error, "Reject an `@` splice of a value that is not a `List` or in a position that disallows splices";
        CheckSpreadNotRecord = "check.spread-not-record", error, "Reject a record spread of a value that is not a record";
        CheckStandardModuleAlias = "check.standard-module-alias", error, "Reject aliasing a standard module in `use`";
        CheckStandardModuleShadow = "check.standard-module-shadow", error, "Reject a binding that shadows a standard module or the built-in `args`";
        CheckStdinSource = "check.stdin-source", error, "Reject `Bytes` input combined with another stdin source on one command";
        CheckStreamAdapter = "check.stream-adapter", error, "Reject an adapter stage that is not the first structured pipeline stage";
        CheckStreamBatch = "check.stream-batch", error, "Reject an invalid `batch` stage option or non-argv items in byte-bounded batches";
        CheckStreamBlockParams = "check.stream-block-params", error, "Reject a stream stage block with too many parameters";
        CheckStreamCallable = "check.stream-callable", error, "Reject a stage callable that is not a statically resolved named function or proc";
        CheckStreamCallableSignature = "check.stream-callable-signature", error, "Reject a stage callable without a unique checked one-item signature";
        CheckStreamCountKey = "check.stream-count-key", error, "Reject a `count` stage key that is not `Str`, `Int`, or `Bool`";
        CheckStreamInput = "check.stream-input", error, "Reject a structured pipeline whose input is not a `Stream` or `List`";
        CheckStreamItem = "check.stream-item", error, "Reject the stream item `.` outside a stream stage block";
        CheckStreamJobs = "check.stream-jobs", error, "Reject a non-positive `jobs` stream option";
        CheckStreamReduceMode = "check.stream-reduce-mode", error, "Reject `reduce-by` without exactly one enabled reduction mode";
        CheckStreamReturn = "check.stream-return", error, "Reject a stream producer whose return type is not `Stream[T]` or that returns a value";
        CheckStreamSort = "check.stream-sort", error, "Reject `sort` or `sort-by` over items or keys that are not sortable";
        CheckStreamStageBlock = "check.stream-stage-block", error, "Reject a missing or unexpected block on a stream stage";
        CheckStreamTerminalStage = "check.stream-terminal-stage", error, "Reject a stream stage that follows a terminal stage";
        CheckTablePrint = "check.table-print", error, "Reject `table.print` over stream items that are not records";
        CheckTestExport = "check.test-export", error, "Reject exporting a `test` declaration";
        CheckTestFeatureDisabled = "check.test-feature-disabled", error, "Reject `test` declarations in a build without native-test support";
        CheckTestNested = "check.test-nested", error, "Reject a `test` declaration that is not top-level";
        CheckTestTopLevel = "check.test-top-level", error, "Reject top-level commands, mutation, or control flow in a test file";
        CheckTryContext = "check.try-context", error, "Reject `?` or a propagating statement outside a Result-returning context";
        CheckTryError = "check.try-error", error, "Reject `?` that propagates an error type the enclosing Result does not accept";
        CheckTryResult = "check.try-result", error, "Reject `?` applied to a value that is not a `Result`";
        CheckTrySuccessType = "check.try-success-type", error, "Reject a `try` block whose success type cannot be inferred";
        CheckTypeApplication = "check.type-application", error, "Reject type arguments applied to something other than a record schema or alias";
        CheckTypeArity = "check.type-arity", error, "Reject a generic type used with the wrong number of type arguments";
        CheckTypeMismatch = "check.type-mismatch", error, "Reject a value whose type does not match the expected type";
        CheckTypeParameters = "check.type-parameters", error, "Reject duplicate, reserved, or unsupported type parameters";
        CheckUndefinedName = "check.undefined-name", error, "Reject assignment to a name that was never declared with `let` or `var`";
        CheckUnknownEnvNamespace = "check.unknown-env-namespace", error, "Reject an unknown `env` namespace";
        CheckUnknownField = "check.unknown-field", error, "Reject access to a field or export that the known type does not have";
        CheckUnknownMethod = "check.unknown-method", error, "Reject a method call the receiver type does not have";
        CheckUnknownModule = "check.unknown-module", error, "Reject `use` of an unknown module or a module path that is not a standard module";
        CheckUnknownModuleApi = "check.unknown-module-api", error, "Reject a call to a function the standard module does not provide";
        CheckUnknownType = "check.unknown-type", error, "Reject a type name, type namespace, or exported type that does not exist";
        CheckUnreachableMatchArm = "check.unreachable-match-arm", warning, "Warn about a `match` arm already covered by earlier unguarded patterns";
        CheckUnresolvedCall = "check.unresolved-call", error, "Reject a call to a pure function that does not resolve";
        CheckUnresolvedName = "check.unresolved-name", error, "Reject a reference to a name that does not resolve";
        CheckUnresolvedProcCommand = "check.unresolved-proc-command", error, "Reject a command statement naming no core command, standard API, or proc";
        CheckUnsupportedApi = "check.unsupported-api", error, "Reject an unsupported or removed standard API such as `env.get_path` or `path.display`";
        CheckWaitTarget = "check.wait-target", error, "Reject `wait` on a value that is not a `ProcessHandle` or list of them";
        CheckWhileCondition = "check.while-condition", error, "Reject a `while` condition that is not `Bool` or `Status`";
        CheckYield = "check.yield", error, "Reject `yield` outside a stream producer, inside a `retry` attempt, or inside a `within` block";
        CheckYieldDelegation = "check.yield-delegation", error, "Reject `yield @` of a value that is not a `List` or `Stream`";
        CheckYieldStream = "check.yield-stream", error, "Reject `yield` of a stream value; use `yield @stream`";
        CheckWithoutEffect = "check.without-effect", error, "Reject `without error`: a local bound subtracts host effects, and `try` bounds errors";
        CheckUnionType = "check.union-type", error, "Reject a `Union[...]` whose members are fewer than two, repeat or contain one another, or are `Any`, `Null`, optional, a stream, a callable type, or another union";
        CheckUnionNarrow = "check.union-narrow", error, "Reject an operation on a `Union[...]` value that has not been narrowed to one member by `is` or a type pattern";
        CheckCallableType = "check.callable-type", error, "Reject a callable type whose parameters have a default, a rest marker, a repeated name, or no type, and a callable type used as a runtime type test";
        CheckCallableMismatch = "check.callable-mismatch", error, "Reject a function whose kind, parameters, return type, or effects do not fit the callable type expected of it, and a call through a callable type that splices its arguments";
        CheckValidatedLiteral = "check.validated-literal", error, "Reject a literal that fails the validation of the type expected of it, such as a list literal that may be empty where a `NonEmpty[T]` is expected";
    }
    Compact {
        CompactCliArgs = "compact.cli-args", error, "Reject script arguments that are not a `List[Str]` when preparing a compact `cli main`";
        CompactCliDefault = "compact.cli-default", error, "Reject a `cli main` parameter default that cannot be lowered to a compact constant";
        CompactIndexedBuild = "compact.indexed-build", error, "Reject a construct that cannot be encoded when building the compact indexed IR";
        CompactIndexedDriver = "compact.indexed-driver", error, "Reject a program whose compact indexed driver steps fail verification, when it is prepared or when a step runs";
        CompactIndexedSource = "compact.indexed-source", error, "Reject a compact build whose source text is unavailable for indexed IR";
        CompactMainArgs = "compact.main-args", error, "Reject script arguments that cannot be converted for compact `proc main` dispatch";
        CompactMainMissingSpread = "compact.main-missing-spread", error, "Reject a `proc main` taking script arguments without the `(...argv: List[Str])` form";
        CompactStatementCount = "compact.statement-count", error, "Reject an indexed driver whose statement count differs from the source program, when it is prepared or run";
        CompactUnloweredMain = "compact.unlowered-main", error, "Reject a `proc main` that cannot be encoded in the compact indexed IR";
        CompactUnloweredStatement = "compact.unlowered-statement", error, "Reject a top-level statement that cannot be encoded in the compact indexed IR";
    }
    Runtime {
        RuntimeCliArgs = "runtime.cli-args", error, "Report a failure binding command-line arguments to the `cli main` signature";
        RuntimeCompactUnsupportedMain = "runtime.compact-unsupported-main", error, "Report a `proc main` that cannot run in the compact runtime";
        RuntimeCompactUnsupportedStatement = "runtime.compact-unsupported-statement", error, "Report a statement that cannot run in the compact runtime";
        RuntimeDeferControlFlow = "runtime.defer-control-flow", error, "Report `return`, `break`, or `continue` escaping a deferred cleanup";
        RuntimeError = "runtime.error", error, "Report an uncaught runtime error raised while running a script or test";
        RuntimeExitStatus = "runtime.exit-status", error, "Report a script exit status outside the integer range 0 to 255";
        RuntimeLoopControl = "runtime.loop-control", error, "Report `break` or `continue` used outside a loop at run time";
        RuntimeReturnOutsideFunction = "runtime.return-outside-function", error, "Report `return` used outside a function at run time";
        RuntimeTestMissing = "runtime.test-missing", error, "Report a native test whose proc is not found";
        RuntimeTestSetup = "runtime.test-setup", error, "Report a native-test program that cannot be installed or driven for setup";
    }
    Lint {
        LintAtomicallyNeverReplaces = "lint.atomically-never-replaces", warning, "Report an `atomically replace` whose body leaves on every path, so its destination is never replaced";
        LintBlockHeader = "lint.block-header", warning, "Move error-handler parameters inside the block of an `else` block";
        LintBooleanGuard = "lint.boolean-guard", warning, "Rewrite a leading failure branch on a Bool condition as `guard ... else`";
        LintBooleanPatternTest = "lint.boolean-pattern-test", warning, "Replace a match yielding `true`/`false` per arm with a pattern test";
        LintCommandValue = "lint.command-value", warning, "Write a parenthesized name or field path command argument as `$name` or `$record.field`";
        LintCompatibilityVocabulary = "lint.compatibility-vocabulary", warning, "Replace removed vocabulary with its canonical name, such as dropping `run.builtin`";
        LintCoreAssert = "lint.core-assert", warning, "Use an `assert` statement instead of a core assertion call in statement position";
        LintDeadCode = "lint.dead-code", warning, "Flag unreachable statements after code that always exits";
        LintDefaultParamType = "lint.default-param-type", warning, "Drop a parameter type annotation when its default already establishes that type";
        LintDollarInExpressionString = "lint.dollar-in-expression-string", warning, "Warn that `$name` in an expression string literal is literal text, not interpolation";
        LintDurationArithmetic = "lint.duration-arithmetic", warning, "Use Duration arithmetic such as `(n * 1s)` for a bounded numeric conversion";
        LintEnumDeclaration = "lint.enum-declaration", warning, "Replace a legacy tag-union `type` declaration with `enum Name { A, B }`";
        LintEnvScope = "lint.env-scope", warning, "Write expression environment assignments as `env ({NAME: value}) { body }`";
        LintErrorFallbackBlock = "lint.error-fallback-block", warning, "Replace an identity success match with a `??` error fallback block";
        LintFsRootReceiver = "lint.fs-root-receiver", warning, "Call removed filesystem operations as methods on the `FsRoot` receiver";
        LintIdenticalMatchArms = "lint.identical-match-arms", warning, "Merge adjacent match arms with identical bodies into one alternative pattern";
        LintInferredRequireTarget = "lint.inferred-require-target", warning, "Drop a `require` schema target the checked boundary already supplies";
        LintInteractiveCommand = "lint.interactive-command", error, "Flag interactive-only commands in scripts and suggest the scripting replacement";
        LintJsonRoundtrip = "lint.json-roundtrip", warning, "Flag a JSON encode then decode round trip as usually redundant";
        LintLegacyTestProc = "lint.legacy-test-proc", warning, "Replace a legacy native test proc taking `TestContext` with a `test` declaration";
        LintLexicalBlock = "lint.lexical-block", warning, "Use a lexical block for an unconditional scope instead of a value block";
        LintLookupAbsence = "lint.lookup-absence", warning, "Compare a lookup against `null` for absence, not the `-1` numeric sentinel";
        LintLookupFallback = "lint.lookup-fallback", warning, "Remove the fallback argument from lookup calls, which no longer accept one";
        LintMissingEffects = "lint.missing-effects", warning, "Flag a proc whose declared effects are incomplete and suggest the full effect list";
        LintMissingFPrefix = "lint.missing-f-prefix", warning, "Add the `f` prefix to a string whose `{name}` names a binding in scope";
        LintNeedlessAnnotation = "lint.needless-annotation", warning, "Remove a type annotation that the initializer or checked constraints already fix";
        LintOrganizeTopLevelConsts = "lint.organize-top-level-consts", warning, "Group safe immutable top-level constants after imports and before functions";
        LintPathConstructor = "lint.path-constructor", warning, "Prefer a `p` string literal or path interpolation over `Path(...)`";
        LintPathDisplayEquality = "lint.path-display-equality", warning, "Compare a Path with a string literal directly instead of through `.display()`";
        LintPathDisplaySink = "lint.path-display-sink", warning, "Pass a Path itself to an argv, environment, or bytes sink instead of its display text";
        LintPathTextQuery = "lint.path-text-query", warning, "Test a Path for a root component instead of testing its display text for a leading `/`";
        LintPatternConditional = "lint.pattern-conditional", warning, "Use `if let` for a two-arm match with a complementary pattern";
        LintPositionalErrorArguments = "lint.positional-error-arguments", warning, "Name error constructor arguments that follow a named one or fill fields a single value fits two of";
        LintPreferAtomicallyReplace = "lint.prefer-atomically-replace", warning, "Use `atomically replace DEST as NAME` for a file produced at a temporary path and renamed into place";
        LintPreferBareFieldLabel = "lint.prefer-bare-field-label", warning, "Write identifier-shaped record field labels without quotes";
        LintPreferBlockString = "lint.prefer-block-string", warning, "Use a block string for a constant multiline string concatenation";
        LintPreferCallableAlias = "lint.prefer-callable-alias", warning, "Use an immutable alias for a callable that exactly forwards to another";
        LintPreferComparisonChain = "lint.prefer-comparison-chain", warning, "Use an ordering chain like `a < b < c` for repeated adjacent comparisons";
        LintPreferConst = "lint.prefer-const", warning, "Declare inert module-level `let` data as `const`";
        LintPreferContextScopeValue = "lint.prefer-context-scope-value", warning, "Let a scope such as `cd` or `env` yield the value instead of a placeholder assigned inside";
        LintPreferDeferBlock = "lint.prefer-defer-block", warning, "Replace a single-use literal cleanup helper with a `defer` block";
        LintPreferEmptyMapLiteral = "lint.prefer-empty-map-literal", warning, "Use `{}` for an empty map in map-typed contexts";
        LintPreferEnvString = "lint.prefer-env-string", warning, "Read an environment variable with a literal identifier name as `e\"NAME\"`";
        LintExplicitRunCapture = "lint.explicit-run-capture", warning, "Write `try` on a value-position run form whose `Result` is kept as a value";
        LintPreferFail = "lint.prefer-fail", warning, "Return a failure that only carries a message with `fail MESSAGE` instead of a one-variant error family";
        LintPreferFileLines = "lint.prefer-file-lines", warning, "Use `path.lines()?` instead of `read_text()?.lines()` in a loop";
        LintPreferFsFiles = "lint.prefer-fs-files", warning, "Use `fs.files()` instead of `fs.walk()` filtered to `kind == file`";
        LintPreferGenericRecordConstructor = "lint.prefer-generic-record-constructor", warning, "Let a constructor infer its concrete schema from the supplied fields";
        LintPreferGuard = "lint.prefer-guard", warning, "Use `guard` instead of a single-action `if`";
        LintPreferImplicitMessage = "lint.prefer-implicit-message", warning, "Declare a variant whose only payload is `message: Str` without a payload and pass its message positionally";
        LintPreferIn = "lint.prefer-in", warning, "Use `in` or `not in` instead of a membership method call";
        LintPreferInferredPrivateEffects = "lint.prefer-inferred-private-effects", warning, "Drop a private proc or stream effect clause that names exactly its inferred effects";
        LintPreferInferredPureReturn = "lint.prefer-inferred-pure-return", warning, "Drop a private pure return type when it is inferred exactly";
        LintPreferInferredVariant = "lint.prefer-inferred-variant", warning, "Drop a variant qualifier that the expected type already selects, as in `.Symlink`";
        LintPreferItemShorthand = "lint.prefer-item-shorthand", warning, "Leave a one-parameter callback's parameter implicit as `.` when it is only read through its fields";
        LintPreferKnownFieldAccess = "lint.prefer-known-field-access", warning, "Select a guaranteed record field directly instead of through a lookup";
        LintPreferListComp = "lint.prefer-list-comp", warning, "Use a list comprehension instead of a for loop that only builds a list";
        LintPreferListCompoundAssignment = "lint.prefer-list-compound-assignment", warning, "Use `+=` for a local list update that reassigns the list";
        LintPreferListElementAssignment = "lint.prefer-list-element-assignment", warning, "Assign a list element directly when the index is known valid";
        LintPreferListPattern = "lint.prefer-list-pattern", warning, "Use a list pattern with `if let` for bounded element extraction after a length check";
        LintPreferListSplicing = "lint.prefer-list-splicing", warning, "Build a list with one literal and explicit splices instead of chained concatenation";
        LintPreferMapComp = "lint.prefer-map-comp", warning, "Use a map comprehension instead of a for loop that only builds a map";
        LintPreferMapEntryIteration = "lint.prefer-map-entry-iteration", warning, "Iterate map entries instead of looping over keys and looking each value up";
        LintPreferMapLiteral = "lint.prefer-map-literal", warning, "Construct a fresh Map with one literal instead of incremental insertion";
        LintPreferMatchElse = "lint.prefer-match-else", warning, "Write a last `_ =>` match arm as the catch-all `else =>`";
        LintPreferMethod = "lint.prefer-method", warning, "Use method form `receiver.func(...)` instead of calling `module.func(receiver, ...)`";
        LintPreferNamedArgumentPun = "lint.prefer-named-argument-pun", warning, "Use the named-argument shorthand when the argument repeats its value name";
        LintPreferNamedArgumentSpread = "lint.prefer-named-argument-spread", warning, "Forward record fields with a named argument spread such as `...record`";
        LintPreferNestedRecordUpdate = "lint.prefer-nested-record-update", warning, "Use disjoint static field paths instead of nested record spreads";
        LintPreferOptionalBinding = "lint.prefer-optional-binding", warning, "Use `guard let` instead of an exiting null test followed by a binding that names the optional again";
        LintPreferOptionalPostfix = "lint.prefer-optional-postfix", warning, "Use a guarded postfix and `??` instead of an explicit null branch";
        LintPreferPositionalConstructor = "lint.prefer-positional-constructor", warning, "Pass leading schema constructor fields positionally when no two of them can hold the same value";
        LintPreferPropagation = "lint.prefer-propagation", warning, "Use `?` instead of a `match` whose `Err` arm only returns the same error";
        LintPreferReadLines = "lint.prefer-read-lines", warning, "Read a file's lines with `Path.read_lines()?` instead of `read_text()?.lines()`";
        LintPreferRecordConstructor = "lint.prefer-record-constructor", warning, "Use the named schema constructor for a record literal of a schema type";
        LintPreferRecordDestructuring = "lint.prefer-record-destructuring", warning, "Bind adjacent fields of one record together with a destructuring `let`";
        LintPreferRegexLiteral = "lint.prefer-regex-literal", warning, "Prepare a static regex pattern with an `rx` literal instead of a call";
        LintPreferRepeat = "lint.prefer-repeat", warning, "Use `repeat N times` instead of `for _ in range(N)`";
        LintPreferScalarIteration = "lint.prefer-scalar-iteration", warning, "Iterate a Str by scalars or bytes without a split List or unused offsets";
        LintPreferRunArgv = "lint.prefer-run-argv", warning, "Run a command vector with `run.status @argv ?` instead of rebuilding it with `process.command_argv(argv[0], argv)`";
        LintPreferSignatureCli = "lint.prefer-signature-cli", warning, "Declare a literal CLI schema as a `cli main(...)` entry signature";
        LintPreferSizeLiteral = "lint.prefer-size-literal", warning, "Write a byte count that is a product of literals and powers of 1024 as a size literal";
        LintPreferSlice = "lint.prefer-slice", warning, "Use half-open slicing where offset/count method bounds are equivalent";
        LintPreferStreamProducer = "lint.prefer-stream-producer", warning, "Suggest a `stream` producer with `yield` for a proc that builds a list item by item";
        LintPreferStringConcat = "lint.prefer-string-concat", warning, "Use `+` instead of joining literal pieces with an empty separator";
        LintPreferTempdirScope = "lint.prefer-tempdir-scope", warning, "Use a `tempdir NAME { ... }` scope for a temporary directory used only through its path";
        LintPreferTempdir = "lint.prefer-tempdir", warning, "Use `tempdir NAME at PATH` for a directory that is cleared, created, and removed on exit";
        LintPreferTryCapture = "lint.prefer-try-capture", warning, "Replace a single-use closed helper with a local `try` block capture";
        LintPreferValuePipeline = "lint.prefer-value-pipeline", warning, "Use a value pipeline for nested calls or a single-use temporary";
        LintPreferWithin = "lint.prefer-within", warning, "Note a block whose `run` forms all carry the same `--timeout`, which one `within` scope would state once";
        LintPreferWriteLines = "lint.prefer-write-lines", warning, "Write a `List[Str]` with `Path.write_lines` instead of joining it and appending a newline";
        LintPreferYieldDelegation = "lint.prefer-yield-delegation", warning, "Replace a transparent forwarding loop with `yield @iterable`";
        LintPublicResultError = "lint.public-result-error", warning, "Spell the error type of each Result in an exported signature or module contract";
        LintRedundantBareReturn = "lint.redundant-bare-return", warning, "Remove a bare `return` at the end of a `Result[Unit]` function";
        LintRedundantCommandFmt = "lint.redundant-command-fmt", warning, "Use command value syntax directly for a single-value command f-string";
        LintRedundantCommandInterpolation = "lint.redundant-command-interpolation", warning, "Use expression syntax directly for a single interpolation in command args";
        LintRedundantDefault = "lint.redundant-default", warning, "Remove a named Bool argument that equals the call default, such as `parents: true`";
        LintRedundantDisplayParse = "lint.redundant-display-parse", warning, "Remove a display then parse round trip on a value already of the parsed type";
        LintRedundantFmtWrapper = "lint.redundant-fmt-wrapper", warning, "Remove redundant `${}` or `()` wrapping around an f-string";
        LintRedundantMainCall = "lint.redundant-main-call", warning, "Remove an explicit `main(@args)` since main is invoked implicitly";
        LintRedundantNewlineTripleString = "lint.redundant-newline-triple-string", warning, "Write a single-newline triple string as an escaped newline literal";
        LintRedundantOkReturn = "lint.redundant-ok-return", warning, "Remove `return Ok()` in a `Result[Unit]` function, using bare `return` or none";
        LintRedundantOkTail = "lint.redundant-ok-tail", warning, "Remove `return Ok(...)` at a function tail since plain values are wrapped";
        LintRedundantOptionalFallback = "lint.redundant-optional-fallback", warning, "Drop a `??` fallback on an Optional receiver proved present";
        LintRedundantPathDisplay = "lint.redundant-path-display", warning, "Drop `.display()` from an interpolated Path; interpolation already renders it";
        LintRedundantPathInterpolation = "lint.redundant-path-interpolation", warning, "Remove a single-value path interpolation that wraps one value";
        LintRedundantPathParse = "lint.redundant-path-parse", warning, "Remove a Path display then parse round trip on a value already a Path";
        LintRedundantPipelineStage = "lint.redundant-pipeline-stage", warning, "Remove no-op `where true` and `map .` pipeline stages";
        LintRedundantPropagation = "lint.redundant-propagation", warning, "Remove `?` from a statement-position `Result[Unit]` call, which already propagates";
        LintRedundantRequire = "lint.redundant-require", warning, "Remove a schema `require` on an expression that already has the required type";
        LintRedundantResultUnit = "lint.redundant-result-unit", warning, "Remove a `Result[Unit]` return annotation that a proc without a value tail infers";
        LintRedundantScopePropagation = "lint.redundant-scope-propagation", warning, "Remove `?` from a statement-position `cd`, `env`, `try`, or `retry` block, which already propagates";
        LintRedundantStringInterpolation = "lint.redundant-string-interpolation", warning, "Remove a string interpolation containing only a single value";
        LintRedundantTailReturnBinding = "lint.redundant-tail-return-binding", warning, "Return the initializer implicitly instead of binding it and returning at the tail";
        LintRedundantTailReturn = "lint.redundant-tail-return", warning, "Use the tail value implicitly instead of a final `return`";
        LintRemovedRecordRequire = "lint.removed-record-require", warning, "Replace the removed `record.require` with a named schema and `.require(Schema)`";
        LintRunStatus = "lint.run-status", warning, "Remove `?` from a `run` of a command whose nonzero status is expected";
        LintRunless = "lint.runless", error, "Reject external commands when linting with `--runless`, except allowed names";
        LintShadowing = "lint.shadowing", error, "Flag a binding that shadows a name from an outer scope";
        LintStageCallable = "lint.stage-callable", warning, "Name the callable directly instead of a transparent stream stage block";
        LintStreamOptions = "lint.stream-options", warning, "Replace stream stage flags with ordinary named arguments";
        LintStringlyTypedMatch = "lint.stringly-typed-match", warning, "Suggest a tag union type for a match with three or more string-literal arms";
        LintUnsortedImports = "lint.unsorted-imports", warning, "Sort a contiguous import block by module path and alias";
        LintUnusedCallable = "lint.unused-callable", warning, "Flag an unexported callable not reachable from a bundle entry point";
        LintUnusedLocal = "lint.unused-local", warning, "Flag a local variable that is never read";
        LintUnusedType = "lint.unused-type", warning, "Flag a type declaration that is never referenced";
        LintPreferInferredProcReturn = "lint.prefer-inferred-proc-return", warning, "Drop a private proc return type when it is inferred exactly";
        LintRedundantUseAlias = "lint.redundant-use-alias", warning, "Drop a `use` alias that repeats the last path segment, as in `use a.b as b`";
        LintListAnyUnion = "lint.list-any-union", warning, "Name the closed `List[Union[...]]` type of an immutable `List[Any]` whose literal elements have a few concrete types";
        LintPreferEnvPathList = "lint.prefer-env-path-list", warning, "Write a search-path environment value as a `List[Path]` instead of formatting a `:`-separated string";
        LintPreferWriteMode = "lint.prefer-write-mode", warning, "Merge a write directly followed by a `chmod` of the same path into `write(..., mode: M)`";
        LintPreferForIndex = "lint.prefer-for-index", warning, "Use `for index, item in list` (or `for item in list`) instead of a counter loop that only walks a list";
        LintPreferPathKind = "lint.prefer-path-kind", warning, "Test a path's kind with `is_dir()`, `is_file()`, or `is_symlink()` instead of comparing `metadata()?.kind`";
        LintPreferPathMethod = "lint.prefer-path-method", warning, "Call the `Path` method instead of the `fs` function that takes the path first";
        LintPreferTypedCallable = "lint.prefer-typed-callable", warning, "Name the callable type of a private function's `Proc` or `Pure` parameter when every call passes a function with one signature";
        LintPreferTestExpect = "lint.prefer-test-expect", warning, "State the status and output fragments of a script run with `test.expect` instead of asserting on each field of `test.run_script`";
        LintExplicitMissingOk = "lint.explicit-missing-ok", warning, "Write `missing_ok: false` on a `remove` that relies on the default, before the default changes (opt-in)";
        LintPreferIsEmpty = "lint.prefer-is-empty", warning, "Use `is_empty()` instead of comparing a length with zero";
        LintPreferNegativeIndex = "lint.prefer-negative-index", warning, "Use `list[-N]` instead of `list[list.len() - N]`";
        LintPreferNonEmptyArgv = "lint.prefer-non-empty-argv", warning, "Report a spliced command vector (`run @argv`) whose type is a plain `List[T]`, which may be empty; `NonEmpty[T]` cannot";
    }
    Format {
        FormatEquivalence = "format-equivalence", error, "Refuse to rewrite a file when formatting would change its parse";
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticCode, DiagnosticFamily};
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    const FAMILY_PREFIXES: &[DiagnosticFamily] = &[
        DiagnosticFamily::Source,
        DiagnosticFamily::Lex,
        DiagnosticFamily::Parse,
        DiagnosticFamily::Check,
        DiagnosticFamily::Compact,
        DiagnosticFamily::Runtime,
        DiagnosticFamily::Lint,
    ];

    fn files_with_extension(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_with_extension(&path, extension, files);
            } else if path.extension().is_some_and(|found| found == extension) {
                files.push(path);
            }
        }
    }

    #[test]
    fn names_are_unique_family_prefixed_and_round_trip() {
        let mut names = BTreeSet::new();
        for &code in DiagnosticCode::ALL {
            let name = code.name();
            assert!(names.insert(name), "duplicate code name {name}");
            assert_eq!(DiagnosticCode::from_name(name), Some(code));
            let prefix = code.family().prefix();
            let separator = if code.family() == DiagnosticFamily::Format {
                '-'
            } else {
                '.'
            };
            assert!(
                name.strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with(separator)),
                "{name} is not in the {prefix} family"
            );
            assert!(
                name.bytes().all(|byte| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-')),
                "{name} is not a lowercase kebab-case code"
            );
        }
        assert_eq!(DiagnosticCode::from_name("check.no-such-code"), None);
    }

    // `xsht lint --list` and the generated lint reference print one summary
    // line per code.
    #[test]
    fn every_code_has_a_one_line_summary() {
        for &code in DiagnosticCode::ALL {
            let summary = code.summary();
            assert!(
                !summary.is_empty() && !summary.contains('\n') && !summary.ends_with('.'),
                "{code}: {summary:?}"
            );
        }
    }

    // Lint codes are always selectable; another stage's code is selectable
    // only when its fix is safe to apply in isolation.
    #[test]
    fn only_non_lint_codes_declare_fixable() {
        for &code in DiagnosticCode::ALL {
            assert!(
                !(code.family() == DiagnosticFamily::Lint && code.fixable()),
                "{code}"
            );
        }
    }

    // A code the table declares but no emission site uses is dead vocabulary
    // that `--only` and the catalogs would still advertise.
    #[test]
    fn every_code_is_referenced_outside_the_table() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        files_with_extension(&root.join("src"), "rs", &mut files);
        files_with_extension(&root.join("crates"), "rs", &mut files);
        let table = root.join("src/diagnostic/code.rs");
        let sources = files
            .iter()
            .filter(|path| **path != table)
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        let unused = DiagnosticCode::ALL
            .iter()
            .filter(|code| {
                let path = format!("DiagnosticCode::{code:?}");
                !sources.iter().any(|source| {
                    source.match_indices(&path).any(|(start, _)| {
                        !source[start + path.len()..]
                            .starts_with(|next: char| next.is_ascii_alphanumeric())
                    })
                })
            })
            .collect::<Vec<_>>();
        assert!(unused.is_empty(), "codes with no emission site: {unused:?}");
    }

    // Documentation cites codes by name; a renamed or deleted code must not
    // leave a stale citation behind.
    #[test]
    fn every_code_cited_in_docs_exists() {
        let mut files = Vec::new();
        files_with_extension(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"),
            "md",
            &mut files,
        );
        let mut unknown = BTreeSet::new();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            for family in FAMILY_PREFIXES {
                let prefix = format!("{}.", family.prefix());
                for (start, _) in text.match_indices(&prefix) {
                    let at_word_start = text[..start].chars().next_back().is_none_or(|before| {
                        !(before.is_ascii_alphanumeric() || matches!(before, '.' | '_' | '-' | '/'))
                    });
                    let rest = &text[start + prefix.len()..];
                    let len = rest
                        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
                        .unwrap_or(rest.len());
                    let name = text[start..start + prefix.len() + len].trim_end_matches('-');
                    if at_word_start && len > 0 && DiagnosticCode::from_name(name).is_none() {
                        unknown.insert(format!("{}: {name}", path.display()));
                    }
                }
            }
        }
        assert!(
            unknown.is_empty(),
            "documentation cites unknown diagnostic codes: {unknown:#?}"
        );
    }
}
