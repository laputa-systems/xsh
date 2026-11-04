import Xsh.Conformance.Connected.FieldLink

namespace Xsh.Conformance.Connected

inductive SourceTokenKind where
  | word (text : String)
  | integer (text : String)
  | string (text : String)
  | symbol (text : String)
  | newline
  deriving DecidableEq, Repr

structure SourceToken where
  kind : SourceTokenKind
  site : SourceSite
  deriving DecidableEq

inductive AdmissionFailure where
  | unsupported
  | exhausted
  | malformed
  deriving DecidableEq, Repr

def asciiDigit (character : Char) : Bool := 48 ≤ character.toNat && character.toNat ≤ 57

def asciiNameStart (character : Char) : Bool :=
  (65 ≤ character.toNat && character.toNat ≤ 90) ||
    (97 ≤ character.toNat && character.toNat ≤ 122) || character == '_'

def asciiNamePart (character : Char) : Bool := asciiNameStart character || asciiDigit character

def takeWhileChars (predicate : Char → Bool) : List Char → List Char × List Char
  | [] => ([], [])
  | character :: rest =>
      if predicate character then
        let (taken, suffix) := takeWhileChars predicate rest
        (character :: taken, suffix)
      else ([], character :: rest)

def plainString : List Char → Except AdmissionFailure (List Char × List Char)
  | [] => .error .malformed
  | character :: rest =>
      if character == '"' then .ok ([], rest)
      else if character.toNat < 32 || 126 < character.toNat || character == '\\' ||
          (character == '$' && rest.head? = some '{') then .error .unsupported
      else do
        let (taken, suffix) ← plainString rest
        pure (character :: taken, suffix)

def singleSymbol (character : Char) : Bool :=
  ['(', ')', '{', '}', '[', ']', ',', ':', '.', ';', '?', '+', '-', '*', '/', '%', '=', '!', '<', '>'].contains character

def doubleSymbol (first second : Char) : Option String :=
  if first == '-' && second == '>' then some "->"
  else if first == '=' && second == '=' then some "=="
  else if first == '!' && second == '=' then some "!="
  else if first == '<' && second == '=' then some "<="
  else if first == '>' && second == '=' then some ">="
  else none

def sourceToken (source start length : Nat) (kind : SourceTokenKind) : SourceToken :=
  ⟨kind, ⟨source, start, length⟩⟩

def tokenizeSource : Nat → Nat → Nat → List Char → Except AdmissionFailure (List SourceToken)
  | _, _, _, [] => .ok []
  | 0, _, _, _ => .error .exhausted
  | fuel + 1, source, offset, character :: rest => do
      let emitToken (length : Nat) (kind : SourceTokenKind) (remaining : List Char) := do
        let tokens ← tokenizeSource fuel source (offset + length) remaining
        pure (sourceToken source offset length kind :: tokens)
      if character == ' ' || character == '\t' then
        tokenizeSource fuel source (offset + 1) rest
      else if character == '\n' then emitToken 1 .newline rest
      else if character == '\r' then
        match rest with
        | '\n' :: remaining => emitToken 2 .newline remaining
        | _ => .error .unsupported
      else if character == '#' then .error .unsupported
      else if asciiNameStart character then
        let (taken, suffix) := takeWhileChars asciiNamePart rest
        if suffix.head? = some '-' then .error .unsupported
        else
          let text := String.ofList (character :: taken)
          emitToken (1 + taken.length) (.word text) suffix
      else if asciiDigit character then
        let (taken, suffix) := takeWhileChars asciiDigit rest
        let text := String.ofList (character :: taken)
        emitToken (1 + taken.length) (.integer text) suffix
      else if character == '"' then
        let (content, suffix) ← plainString rest
        emitToken (content.length + 2) (.string (String.ofList content)) suffix
      else if character == '?' && [some '?', some '.', some '['].contains rest.head? then .error .unsupported
      else if singleSymbol character then
        match rest with
        | second :: suffix =>
            match doubleSymbol character second with
            | some text => emitToken 2 (.symbol text) suffix
            | none => emitToken 1 (.symbol (String.singleton character)) rest
        | [] => emitToken 1 (.symbol (String.singleton character)) []
      else .error .unsupported

def tokenizeText (source : Nat) (text : String) : Except AdmissionFailure (List SourceToken) :=
  tokenizeSource (text.toList.length + 1) source 0 text.toList

def sourceNames (tokens : List SourceToken) : List String :=
  (["Int", "Bool", "Str", "Result"] ++ tokens.filterMap (fun token =>
    match token.kind with | .word name => some name | _ => none)).eraseDups

def sourceName (names : List String) (name : String) : Option Xsh.Foundation.NameId := do
  if names.contains name then pure () else none
  Xsh.Foundation.NameId.decode? (names.idxOf name)

def reservedName (name : String) : Bool :=
  ["and", "assert", "atomically", "backoff", "break", "const", "continue", "defer", "else",
   "enum", "errdefer", "every", "export", "false", "for", "guard", "if", "in", "let", "loop",
   "match", "not", "null", "or", "proc", "pure", "retry", "return", "run", "spawn", "stream",
   "true", "try", "type", "unless", "until", "use", "var", "wait", "when", "while", "with",
   "within", "yield"].contains name

def SourceToken.finish (token : SourceToken) : Nat := token.site.start + token.site.length

def coveringSite (first last : SourceSite) : SourceSite :=
  ⟨first.source, first.start, last.start + last.length - first.start⟩

structure LocatedExpression where
  expression : Xsh.Syntax.Expr
  site : SourceSite
  nodes : List (SourceSite × Xsh.Syntax.Expr)
  statements : List (SourceSite × Xsh.Syntax.Statement)

def locatedExpression (site : SourceSite) (expression : Xsh.Syntax.Expr)
    (children : List (SourceSite × Xsh.Syntax.Expr))
    (statements : List (SourceSite × Xsh.Syntax.Statement)) : LocatedExpression :=
  ⟨expression, site, (site, expression) :: children, statements⟩

def expectSymbol (text : String) : List SourceToken → Except AdmissionFailure (SourceToken × List SourceToken)
  | token :: rest => if token.kind = .symbol text then .ok (token, rest) else .error .malformed
  | [] => .error .malformed

def expectWord (text : String) : List SourceToken → Except AdmissionFailure (SourceToken × List SourceToken)
  | token :: rest => if token.kind = .word text then .ok (token, rest) else .error .malformed
  | [] => .error .malformed

def expectName (names : List String) : List SourceToken → Except AdmissionFailure
    (Xsh.Foundation.NameId × SourceToken × List SourceToken)
  | token :: rest => match token.kind with
    | .word text =>
      if text = "_" then .error .unsupported
      else if reservedName text then .error .malformed
      else match sourceName names text with
        | some name => .ok (name, token, rest)
        | none => .error .unsupported
    | _ => .error .malformed
  | [] => .error .malformed

def binaryToken : SourceTokenKind → Option (Xsh.Syntax.BinaryOp × Nat)
  | .symbol "*" => some (.mul, 6)
  | .symbol "/" => some (.div, 6)
  | .symbol "%" => some (.rem, 6)
  | .symbol "+" => some (.add, 5)
  | .symbol "-" => some (.sub, 5)
  | .symbol "<" => some (.lt, 4)
  | .symbol "<=" => some (.le, 4)
  | .symbol ">" => some (.gt, 4)
  | .symbol ">=" => some (.ge, 4)
  | .symbol "==" => some (.eq, 3)
  | .symbol "!=" => some (.ne, 3)
  | .word "and" => some (.and, 2)
  | .word "or" => some (.or, 1)
  | _ => none

def expectLabel (names : List String) : List SourceToken → Except AdmissionFailure
    (Xsh.Foundation.NameId × SourceToken × List SourceToken)
  | token :: rest => match token.kind with
    | .word text => match sourceName names text with
      | some name => .ok (name, token, rest)
      | none => .error .unsupported
    | _ => .error .malformed
  | [] => .error .malformed

def comparisonOperator : Xsh.Syntax.BinaryOp → Bool
  | .lt | .le | .gt | .ge | .eq | .ne => true
  | _ => false

def hasTopComparison : Xsh.Syntax.Expr → Bool
  | .binary operator _ _ => comparisonOperator operator
  | _ => false

mutual
  def parseExpression : Nat → List String → Nat → List SourceToken →
      Except AdmissionFailure (LocatedExpression × List SourceToken)
    | 0, _, _, _ => .error .exhausted
    | fuel + 1, names, minimum, tokens => do
      let (first, rest) ← parseAtom fuel names tokens
      let (first, rest) ← parsePostfix fuel names first rest
      parseBinaryTail fuel names minimum first rest

  def parseAtom : Nat → List String → List SourceToken →
      Except AdmissionFailure (LocatedExpression × List SourceToken)
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, token :: rest => match token.kind with
      | .integer text =>
        let expression := Xsh.Syntax.Expr.literal (.integer text)
        pure (locatedExpression token.site expression [] [], rest)
      | .string text =>
        let expression := Xsh.Syntax.Expr.literal (.string text)
        pure (locatedExpression token.site expression [] [], rest)
      | .word "true" => pure (locatedExpression token.site (.literal (.bool true)) [] [], rest)
      | .word "false" => pure (locatedExpression token.site (.literal (.bool false)) [] [], rest)
      | .word "if" => do
        let (condition, afterCondition) ← parseExpression fuel names 0 rest
        let (yesOpen, afterOpen) ← expectSymbol "{" afterCondition
        let (yes, afterYes) ← parseExpression fuel names 0 afterOpen
        let (yesClose, afterYesClose) ← expectSymbol "}" afterYes
        let (_, afterElse) ← expectWord "else" afterYesClose
        let (noOpen, afterElseOpen) ← expectSymbol "{" afterElse
        let (no, afterNo) ← parseExpression fuel names 0 afterElseOpen
        let (close, remaining) ← expectSymbol "}" afterNo
        let yesStatement := match yes.expression with
          | .ident name => Xsh.Syntax.Statement.tailName name
          | _ => .expression yes.expression
        let noStatement := match no.expression with
          | .ident name => Xsh.Syntax.Statement.tailName name
          | _ => .expression no.expression
        let yesNodes := match yes.expression with | .ident _ => [] | _ => yes.nodes
        let noNodes := match no.expression with | .ident _ => [] | _ => no.nodes
        let yesBlock := Xsh.Syntax.Expr.valueBlock [yesStatement]
        let noBlock := Xsh.Syntax.Expr.valueBlock [noStatement]
        let yesSite := coveringSite yesOpen.site yesClose.site
        let noSite := coveringSite noOpen.site close.site
        let expression := Xsh.Syntax.Expr.ifExpr [(condition.expression, yesBlock)] noBlock
        pure (locatedExpression (coveringSite token.site close.site) expression
          (condition.nodes ++ [(yesSite, yesBlock)] ++ yesNodes ++ [(noSite, noBlock)] ++ noNodes)
          (condition.statements ++ [(yes.site, yesStatement)] ++ yes.statements ++
            [(no.site, noStatement)] ++ no.statements), remaining)
      | .word text =>
        if reservedName text then .error .unsupported
        else match sourceName names text with
          | some name => pure (locatedExpression token.site (.ident name) [] [], rest)
          | none => .error .unsupported
      | .symbol "{" => do
        let (fields, nodes, statements, close, remaining) ← parseRecordFields fuel names rest
        pure (locatedExpression (coveringSite token.site close.site) (.record fields) nodes statements, remaining)
      | _ => .error .unsupported
    | _ + 1, _, [] => .error .malformed

  def parsePostfix : Nat → List String → LocatedExpression → List SourceToken →
      Except AdmissionFailure (LocatedExpression × List SourceToken)
    | 0, _, _, _ => .error .exhausted
    | fuel + 1, names, base, token :: rest => match token.kind with
      | .symbol "." => do
        let (name, label, remaining) ← expectLabel names rest
        let specialRequire := [sourceName names "require", sourceName names "require_exact",
          sourceName names "require_stripped"].contains (some name)
        let codecReceiver := match base.expression with
          | .ident identifier => sourceName names "codec" = some identifier
          | _ => false
        match remaining with
        | openToken :: _ =>
          if openToken.kind = .symbol "(" && (specialRequire || codecReceiver) then throw .unsupported
          else pure ()
        | [] => pure ()
        match remaining with
        | minus :: _ =>
          if minus.kind = .symbol "-" && minus.site.start = label.finish then
            throw .unsupported
          else pure ()
        | [] => pure ()
        let expression := Xsh.Syntax.Expr.field base.expression name
        let next := locatedExpression (coveringSite base.site label.site) expression base.nodes base.statements
        parsePostfix fuel names next remaining
      | .symbol "(" => do
        let (arguments, nodes, statements, close, remaining) ← parseCallArguments fuel names rest
        let expression := Xsh.Syntax.Expr.call base.expression arguments
        let next := locatedExpression (coveringSite base.site close.site) expression (base.nodes ++ nodes)
          (base.statements ++ statements)
        parsePostfix fuel names next remaining
      | .symbol "?" =>
        if token.site.start != base.site.start + base.site.length then .error .unsupported
        else
          let expression := Xsh.Syntax.Expr.tryExpr base.expression
          let next := locatedExpression (coveringSite base.site token.site) expression base.nodes base.statements
          parsePostfix fuel names next rest
      | _ => pure (base, token :: rest)
    | _ + 1, _, base, [] => pure (base, [])

  def parseBinaryTail : Nat → List String → Nat → LocatedExpression → List SourceToken →
      Except AdmissionFailure (LocatedExpression × List SourceToken)
    | 0, _, _, _, _ => .error .exhausted
    | fuel + 1, names, minimum, left, token :: rest =>
      match binaryToken token.kind with
      | some (operator, precedence) =>
        if precedence < minimum then pure (left, token :: rest)
        else if comparisonOperator operator && hasTopComparison left.expression then .error .unsupported
        else do
          let (right, remaining) ← parseExpression fuel names (precedence + 1) rest
          if comparisonOperator operator && hasTopComparison right.expression then
            throw .unsupported
          else pure ()
          let expression := Xsh.Syntax.Expr.binary operator left.expression right.expression
          let next := locatedExpression (coveringSite left.site right.site) expression (left.nodes ++ right.nodes)
            (left.statements ++ right.statements)
          parseBinaryTail fuel names minimum next remaining
      | none => pure (left, token :: rest)
    | _ + 1, _, _, left, [] => pure (left, [])

  def parseCallArguments : Nat → List String → List SourceToken →
      Except AdmissionFailure (List Xsh.Syntax.CallArgument × List (SourceSite × Xsh.Syntax.Expr) ×
        List (SourceSite × Xsh.Syntax.Statement) × SourceToken × List SourceToken)
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, token :: rest =>
      if token.kind = .symbol ")" then pure ([], [], [], token, rest)
      else do
        let (value, afterValue) ← parseExpression fuel names 0 (token :: rest)
        match afterValue with
        | close :: remaining =>
          if close.kind = .symbol ")" then pure ([.positional value.expression], value.nodes, value.statements, close, remaining)
          else if close.kind = .symbol "," then do
            let (others, nodes, statements, final, suffix) ← parseCallArguments fuel names remaining
            pure (.positional value.expression :: others, value.nodes ++ nodes, value.statements ++ statements, final, suffix)
          else .error .malformed
        | [] => .error .malformed
    | _ + 1, _, [] => .error .malformed

  def parseRecordFields : Nat → List String → List SourceToken →
      Except AdmissionFailure (List Xsh.Syntax.RecordField × List (SourceSite × Xsh.Syntax.Expr) ×
        List (SourceSite × Xsh.Syntax.Statement) × SourceToken × List SourceToken)
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, token :: rest =>
      if token.kind = .symbol "}" then pure ([], [], [], token, rest)
      else do
        let (name, _, afterName) ← expectLabel names (token :: rest)
        let (_, afterColon) ← expectSymbol ":" afterName
        let (value, afterValue) ← parseExpression fuel names 0 afterColon
        match afterValue with
        | close :: remaining =>
          if close.kind = .symbol "}" then pure ([.named name value.expression], value.nodes, value.statements, close, remaining)
          else if close.kind = .symbol "," then do
            let (others, nodes, statements, final, suffix) ← parseRecordFields fuel names remaining
            pure (.named name value.expression :: others, value.nodes ++ nodes, value.statements ++ statements, final, suffix)
          else .error .malformed
        | [] => .error .malformed
    | _ + 1, _, [] => .error .malformed
end

def dropSeparators : List SourceToken → List SourceToken
  | token :: rest =>
      if token.kind = .newline || token.kind = .symbol ";" then dropSeparators rest else token :: rest
  | [] => []

def effectName : String → Option Xsh.Syntax.Effect
  | "fs" => some .fs
  | "net" => some .net
  | "process" => some .process
  | "env" => some .env
  | "time" => some .time
  | "error" => some .error
  | "io" => some .io
  | _ => none

structure SourceEffectRegion where
  head : SourceSite
  body : SourceSite
  excluded : List Xsh.Syntax.Effect

structure SourceParameter where
  name : Xsh.Foundation.NameId
  site : SourceSite
  annotation : Option Xsh.Syntax.TypeExpr

structure SourceFunction where
  kind : CallableKind
  declaredEffects : Option (List Xsh.Syntax.Effect)
  name : Xsh.Foundation.NameId
  nameSite : SourceSite
  bodySite : SourceSite
  parameters : List SourceParameter
  result : Option Xsh.Syntax.TypeExpr
  body : List Xsh.Syntax.Statement
  expressions : List (SourceSite × Xsh.Syntax.Expr)
  statements : List (SourceSite × Xsh.Syntax.Statement)
  regions : List SourceEffectRegion

inductive SourceDeclaration where
  | function (value : SourceFunction)
  | statement (value : Xsh.Syntax.Statement) (expressions : List (SourceSite × Xsh.Syntax.Expr))
      (statements : List (SourceSite × Xsh.Syntax.Statement))

structure SourceUnit where
  source : Nat
  names : List String
  declarations : List SourceDeclaration

mutual
  def parseType : Nat → List String → List SourceToken →
      Except AdmissionFailure (Xsh.Syntax.TypeExpr × List SourceToken)
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, tokens => do
      let (name, _, rest) ← expectName names tokens
      match rest with
      | openToken :: afterOpen =>
        if openToken.kind = .symbol "[" then do
          let (arguments, remaining) ← parseTypeArguments fuel names afterOpen
          pure (.applied (.named name) arguments, remaining)
        else pure (.named name, rest)
      | [] => pure (.named name, [])

  def parseTypeArguments : Nat → List String → List SourceToken →
      Except AdmissionFailure (List Xsh.Syntax.TypeExpr × List SourceToken)
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, tokens => do
      let (first, rest) ← parseType fuel names tokens
      match rest with
      | token :: remaining =>
        if token.kind = .symbol "]" then pure ([first], remaining)
        else if token.kind = .symbol "," then do
          let (others, suffix) ← parseTypeArguments fuel names remaining
          pure (first :: others, suffix)
        else .error .malformed
      | [] => .error .malformed
end

def parseOptionalAnnotation (fuel : Nat) (names : List String) (tokens : List SourceToken) :
    Except AdmissionFailure (Option Xsh.Syntax.TypeExpr × List SourceToken) := do
  match tokens with
  | token :: rest =>
    if token.kind = .symbol ":" then do
      let (annotation, remaining) ← parseType fuel names rest
      pure (some annotation, remaining)
    else pure (none, tokens)
  | [] => pure (none, [])

def parseParameters : Nat → List String → List SourceToken →
    Except AdmissionFailure (List SourceParameter × List SourceToken)
  | 0, _, _ => .error .exhausted
  | fuel + 1, names, token :: rest =>
    if token.kind = .symbol ")" then pure ([], rest)
    else do
      let (name, label, afterName) ← expectName names (token :: rest)
      let (annotation, afterType) ← parseOptionalAnnotation fuel names afterName
      let parameter : SourceParameter := ⟨name, label.site, annotation⟩
      match afterType with
      | close :: remaining =>
        if close.kind = .symbol ")" then pure ([parameter], remaining)
        else if close.kind = .symbol "," then do
          let (others, suffix) ← parseParameters fuel names remaining
          pure (parameter :: others, suffix)
        else .error .malformed
      | [] => .error .malformed
  | _ + 1, _, [] => .error .malformed

def spacedPostfix : List SourceToken → Bool
  | first :: second :: rest =>
    if second.kind = .newline || second.kind = .symbol ";" then false
    else if (second.kind = .symbol "." || second.kind = .symbol "(") && second.site.start != first.finish then true
    else spacedPostfix (second :: rest)
  | _ => false

structure StatementPart where
  statement : Xsh.Syntax.Statement
  expressions : List (SourceSite × Xsh.Syntax.Expr)
  nested : List (SourceSite × Xsh.Syntax.Statement)
  regions : List SourceEffectRegion
  remaining : List SourceToken

structure LocatedStatementPart where
  statement : Xsh.Syntax.Statement
  expressions : List (SourceSite × Xsh.Syntax.Expr)
  statements : List (SourceSite × Xsh.Syntax.Statement)
  regions : List SourceEffectRegion
  remaining : List SourceToken

def parseRegionHead : Nat → List SourceToken → Option (List Xsh.Syntax.Effect × SourceToken × SourceToken × List SourceToken)
  | 0, _ => none
  | _ + 1, [] => none
  | fuel + 1, token :: rest => do
    let .word name := token.kind | none
    let effect ← effectName name
    let next :: remaining := rest | none
    if next.kind = .symbol "{" then some ([effect], token, next, remaining)
    else if next.kind = .symbol "," then do
      let (others, last, openToken, suffix) ← parseRegionHead fuel remaining
      some (effect :: others, last, openToken, suffix)
    else none

mutual
  def parseStatement : Nat → List String → List SourceToken → Except AdmissionFailure LocatedStatementPart
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, tokens => do
      let parsed ← parseStatementPart fuel names tokens
      let some first := tokens.head? | throw .malformed
      let consumed := tokens.take (tokens.length - parsed.remaining.length)
      let some last := consumed.getLast? | throw .malformed
      let compound := match parsed.statement with | .ifStmt _ _ | .expression (.valueBlock _) => true | _ => false
      let finalSite := match parsed.remaining with
        | terminator :: _ =>
          if !compound && (terminator.kind = .newline || terminator.kind = .symbol ";") then terminator.site
          else last.site
        | [] => last.site
      let site := coveringSite first.site finalSite
      pure ⟨parsed.statement, parsed.expressions, (site, parsed.statement) :: parsed.nested, parsed.regions, parsed.remaining⟩

  def parseStatementPart : Nat → List String → List SourceToken → Except AdmissionFailure StatementPart
    | 0, _, _ => .error .exhausted
    | fuel + 1, names, token :: rest =>
      match token.kind with
      | .word "let" | .word "var" => do
        let (name, _, afterName) ← expectName names rest
        let (annotation, afterType) ← parseOptionalAnnotation fuel names afterName
        let (_, afterEquals) ← expectSymbol "=" afterType
        let (value, remaining) ← parseExpression fuel names 0 afterEquals
        let kind := if token.kind = .word "var" then Xsh.Syntax.BindingKind.var else .letBinding
        pure ⟨.binding kind (.name name) annotation value.expression, value.nodes, value.statements, [], remaining⟩
      | .word "return" => do
        let (value, remaining) ← parseExpression fuel names 0 rest
        pure ⟨.returnValue (some value.expression), value.nodes, value.statements, [], remaining⟩
      | .word "assert" => do
        let (value, remaining) ← parseExpression fuel names 0 rest
        pure ⟨.assert value.expression none, value.nodes, value.statements, [], remaining⟩
      | .word "if" => do
        let (condition, afterCondition) ← parseExpression fuel names 0 rest
        let (_, afterOpen) ← expectSymbol "{" afterCondition
        let (yes, yesNodes, yesStatements, yesRegions, _, afterYes) ← parseBlock fuel names afterOpen
        match afterYes with
        | otherwise :: following =>
          if otherwise.kind = .word "else" then do
            let (_, afterElseOpen) ← expectSymbol "{" following
            let (no, noNodes, noStatements, noRegions, _, suffix) ← parseBlock fuel names afterElseOpen
            pure ⟨.ifStmt [(condition.expression, yes)] (some no), condition.nodes ++ yesNodes ++ noNodes,
              condition.statements ++ yesStatements ++ noStatements, yesRegions ++ noRegions, suffix⟩
          else pure ⟨.ifStmt [(condition.expression, yes)] none, condition.nodes ++ yesNodes, condition.statements ++ yesStatements, yesRegions, afterYes⟩
        | [] => pure ⟨.ifStmt [(condition.expression, yes)] none, condition.nodes ++ yesNodes, condition.statements ++ yesStatements, yesRegions, []⟩
      | .word name =>
        let separated := match rest with | first :: _ => first.site.start != token.finish | [] => false
        let regionHead := if name = "without" && separated then parseRegionHead fuel rest else none
        match regionHead with
        | some (effects, last, openToken, afterOpen) => do
          let (body, bodyNodes, bodyStatements, nestedRegions, close, suffix) ← parseBlock fuel names afterOpen
          let bodySite := coveringSite openToken.site close.site
          let expression := Xsh.Syntax.Expr.valueBlock body
          let region : SourceEffectRegion := ⟨coveringSite token.site last.site, bodySite, effects⟩
          pure ⟨.expression expression, (bodySite, expression) :: bodyNodes, bodyStatements,
            region :: nestedRegions, suffix⟩
        | none =>
          match rest with
          | terminator :: _ =>
            if terminator.kind = .newline || terminator.kind = .symbol ";" || terminator.kind = .symbol "}" then do
              if ["print", "eprint"].contains name then throw .unsupported else pure ()
              if reservedName name then
                let (value, remaining) ← parseExpression fuel names 0 (token :: rest)
                pure ⟨.expression value.expression, value.nodes, value.statements, [], remaining⟩
              else
                let some target := sourceName names name | throw .unsupported
                pure ⟨.tailName target, [], [], [], rest⟩
            else if terminator.kind = .symbol "=" then do
              let _ :: afterEquals := rest | throw .malformed
              if reservedName name then throw .malformed else pure ()
              let some target := sourceName names name | throw .unsupported
              let (value, remaining) ← parseExpression fuel names 0 afterEquals
              pure ⟨.assign (.name target) .set value.expression, value.nodes, value.statements, [], remaining⟩
            else do
              let ambiguousMinus := match rest with
                | minus :: next :: _ => minus.kind = .symbol "-" && minus.site.start != token.finish &&
                    next.site.start = minus.finish
                | _ => false
              if ambiguousMinus || spacedPostfix (token :: rest) then throw .unsupported else pure ()
              let (value, remaining) ← parseExpression fuel names 0 (token :: rest)
              pure ⟨.expression value.expression, value.nodes, value.statements, [], remaining⟩
          | [] => throw .unsupported
      | _ => do
        let (value, remaining) ← parseExpression fuel names 0 (token :: rest)
        pure ⟨.expression value.expression, value.nodes, value.statements, [], remaining⟩
    | _ + 1, _, [] => .error .malformed

  def parseBlock : Nat → List String → List SourceToken →
    Except AdmissionFailure (List Xsh.Syntax.Statement × List (SourceSite × Xsh.Syntax.Expr) ×
      List (SourceSite × Xsh.Syntax.Statement) × List SourceEffectRegion × SourceToken × List SourceToken)
  | 0, _, _ => .error .exhausted
  | fuel + 1, names, tokens =>
    match dropSeparators tokens with
    | [] => .error .malformed
    | close :: rest =>
      if close.kind = .symbol "}" then pure ([], [], [], [], close, rest)
      else do
        let parsed ← parseStatement fuel names (close :: rest)
        match parsed.remaining with
        | [] => .error .malformed
        | separator :: _ =>
          let compound := match parsed.statement with | .ifStmt _ _ | .expression (.valueBlock _) => true | _ => false
          if compound || separator.kind = .newline || separator.kind = .symbol ";" || separator.kind = .symbol "}" then do
            let (following, followingNodes, followingStatements, followingRegions, final, suffix) ← parseBlock fuel names parsed.remaining
            match parsed.statement with
            | .tailName _ => if following.isEmpty then pure () else throw .unsupported
            | _ => pure ()
            pure (parsed.statement :: following, parsed.expressions ++ followingNodes,
              parsed.statements ++ followingStatements, parsed.regions ++ followingRegions, final, suffix)
          else .error .unsupported
end

def parseEffectClause : Nat → List SourceToken → Except AdmissionFailure (List Xsh.Syntax.Effect × List SourceToken)
  | 0, _ => .error .exhausted
  | _ + 1, [] => .error .malformed
  | fuel + 1, token :: rest =>
    if token.kind = .symbol "]" then .ok ([], rest)
    else match token.kind with
      | .word name => do
        let some effect := effectName name | throw .unsupported
        match rest with
        | separator :: remaining =>
          if separator.kind = .symbol "]" then pure ([effect], remaining)
          else if separator.kind = .symbol "," then do
            if remaining.head?.map SourceToken.kind = some (.symbol "]") then throw .unsupported else pure ()
            let (others, suffix) ← parseEffectClause fuel remaining
            if effect ∈ others then throw .unsupported else pure (effect :: others, suffix)
          else throw .malformed
        | [] => throw .malformed
      | _ => .error .malformed

def parseFunction (fuel : Nat) (names : List String) (tokens : List SourceToken) :
    Except AdmissionFailure (SourceFunction × List SourceToken) := do
  let (kind, afterKind) ← match tokens with
    | token :: rest =>
      if token.kind = .word "pure" then pure (CallableKind.pure, rest)
      else if token.kind = .word "proc" then pure (CallableKind.proc, rest)
      else throw .unsupported
    | [] => throw .malformed
  let (name, label, afterName) ← expectName names afterKind
  let (_, afterOpen) ← expectSymbol "(" afterName
  let (parameters, afterParameters) ← parseParameters fuel names afterOpen
  let (declaredEffects, afterEffects) ← match afterParameters with
    | openToken :: rest =>
      if openToken.kind = .symbol "[" && kind = .proc then do
        let (effects, remaining) ← parseEffectClause fuel rest
        pure (some effects, remaining)
      else pure (none, afterParameters)
    | [] => pure (none, [])
  let (result, afterResult) ← match afterEffects with
    | arrow :: rest =>
      if arrow.kind = .symbol "->" then do
        let (annotation, suffix) ← parseType fuel names rest
        pure (some annotation, suffix)
      else pure (none, afterEffects)
    | [] => pure (none, [])
  let (openToken, afterBodyOpen) ← expectSymbol "{" afterResult
  let (body, expressions, statements, regions, close, suffix) ← parseBlock fuel names afterBodyOpen
  pure (⟨kind, declaredEffects, name, label.site, coveringSite openToken.site close.site, parameters, result, body, expressions, statements, regions⟩, suffix)

def parseDeclarations : Nat → List String → List SourceToken → Except AdmissionFailure (List SourceDeclaration)
  | 0, _, _ => .error .exhausted
  | fuel + 1, names, tokens =>
    match dropSeparators tokens with
    | [] => .ok []
    | token :: rest =>
      if token.kind = .word "pure" || token.kind = .word "proc" then do
        let (declaration, remaining) ← parseFunction fuel names (token :: rest)
        let following ← parseDeclarations fuel names remaining
        pure (.function declaration :: following)
      else do
        let parsed ← parseStatement fuel names (token :: rest)
        if parsed.regions.isEmpty then pure () else throw .unsupported
        match parsed.statement with
        | .tailName _ => throw .unsupported
        | _ => pure ()
        match parsed.remaining with
        | [] => pure [.statement parsed.statement parsed.expressions parsed.statements]
        | separator :: _ =>
          if separator.kind = .newline || separator.kind = .symbol ";" then do
            let following ← parseDeclarations fuel names parsed.remaining
            pure (.statement parsed.statement parsed.expressions parsed.statements :: following)
          else .error .unsupported

def admitSource (source : Nat) (text : String) : Except AdmissionFailure SourceUnit := do
  let tokens ← tokenizeText source text
  let names := sourceNames tokens
  let declarations ← parseDeclarations (32 * (tokens.length + 1)) names tokens
  pure ⟨source, names, declarations⟩

def projectionSource : String :=
  "pure project(value) { value.amount }\npure use_project() -> Int { project({amount: 7, enabled: true}) }\nassert use_project() == 7\n"

def projectionAdmissionCheck : Bool :=
  match admitSource 0 projectionSource with
  | .ok unit =>
    match unit.declarations, sourceName unit.names "value", sourceName unit.names "amount" with
    | [.function first, .function second, .statement _ _ _], some value, some amount =>
      match first.parameters, first.body, first.expressions with
      | [parameter], [.expression (.field (.ident seen) field)], (site, _) :: _ =>
        decide (parameter.name = value) && decide (seen = value) && decide (field = amount) &&
          decide (first.bodySite = ⟨0, 20, 16⟩) && decide (site = ⟨0, 22, 12⟩) &&
          decide (second.parameters.isEmpty)
      | _, _, _ => false
    | _, _, _ => false
  | .error _ => false

theorem admits_located_projection_source : projectionAdmissionCheck = true := by
  decide +kernel

def withoutAdmissionCheck : Bool :=
  match admitSource 0 "pure f() -> Int { var x = 0; without time { x = 1 }; return x }\n" with
  | .ok unit =>
    match unit.declarations, sourceName unit.names "x" with
    | [.function source], some name =>
      match source.regions, source.body with
      | [region], [.binding .var (.name binder) _ _,
          .expression (.valueBlock [.assign (.name target) .set (.literal (.integer "1"))]),
          .returnValue (some (.ident returned))] =>
        decide (region.head = ⟨0, 29, 12⟩) && decide (region.body = ⟨0, 42, 9⟩) &&
          decide (region.excluded = [.time]) && decide (binder = name ∧ target = name ∧ returned = name)
      | _, _ => false
    | _, _ => false
  | .error _ => false

theorem admits_without_as_a_located_lexical_block : withoutAdmissionCheck = true := by
  decide +kernel

def ordinaryWithoutNameCheck : Bool :=
  match admitSource 0 "pure f(without: Int) -> Int { return without }\n" with
  | .ok unit => match unit.declarations with
    | [.function source] => source.regions.isEmpty
    | _ => false
  | .error _ => false

theorem without_remains_an_ordinary_expression_name : ordinaryWithoutNameCheck = true := by
  decide +kernel

theorem admitted_source_has_tokenization (source : Nat) (text : String) (unit : SourceUnit)
    (accepted : admitSource source text = .ok unit) :
    ∃ tokens, tokenizeText source text = .ok tokens ∧ unit.source = source ∧
      unit.names = sourceNames tokens ∧
      parseDeclarations (32 * (tokens.length + 1)) (sourceNames tokens) tokens = .ok unit.declarations := by
  unfold admitSource at accepted
  cases tokenized : tokenizeText source text with
  | error error => simp [tokenized, Xsh.Typing.Connected.except_bind_ok] at accepted
  | ok tokens =>
    cases parsed : parseDeclarations (32 * (tokens.length + 1)) (sourceNames tokens) tokens with
    | error error => simp [tokenized, parsed, Xsh.Typing.Connected.except_bind_ok, Xsh.Typing.Connected.except_map_ok] at accepted
    | ok declarations =>
      simp [tokenized, parsed, Xsh.Typing.Connected.except_bind_ok, Xsh.Typing.Connected.except_map_ok] at accepted
      subst unit
      exact ⟨tokens, rfl, rfl, rfl, parsed⟩

theorem source_name_injective (names : List String) (first second : String) (id : Xsh.Foundation.NameId)
    (left : sourceName names first = some id) (right : sourceName names second = some id) : first = second := by
  have leftFacts : first ∈ names ∧ Xsh.Foundation.NameId.decode? (names.idxOf first) = some id := by
    simpa [sourceName] using left
  have rightFacts : second ∈ names ∧ Xsh.Foundation.NameId.decode? (names.idxOf second) = some id := by
    simpa [sourceName] using right
  have leftIndex : names.idxOf first = id.value := by
    simp [Xsh.Foundation.NameId.decode?, Xsh.Foundation.Word32.decode?] at leftFacts
    obtain ⟨_, _, equal⟩ := leftFacts
    cases equal
    rfl
  have rightIndex : names.idxOf second = id.value := by
    simp [Xsh.Foundation.NameId.decode?, Xsh.Foundation.Word32.decode?] at rightFacts
    obtain ⟨_, _, equal⟩ := rightFacts
    cases equal
    rfl
  have firstBound := List.idxOf_lt_length_of_mem leftFacts.1
  have secondBound := List.idxOf_lt_length_of_mem rightFacts.1
  have sameIndex : (⟨names.idxOf first, firstBound⟩ : Fin names.length) =
      ⟨names.idxOf second, secondBound⟩ := Fin.ext (leftIndex.trans rightIndex.symm)
  have same := congrArg names.get sameIndex
  simpa only [List.get_eq_getElem, List.getElem_idxOf] using same

def statementIfCheck : Bool :=
  match admitSource 0 "pure f(flag: Bool) -> Int { if flag { 1 } else { 2 } }\n" with
  | .ok unit => match unit.declarations with
    | [.function function] => match function.body with
      | [.ifStmt [(_, [.expression (.literal (.integer "1"))])] (some [.expression (.literal (.integer "2"))])] => true
      | _ => false
    | _ => false
  | _ => false

def expressionIfCheck : Bool :=
  match admitSource 0 "pure f(flag: Bool) -> Int { return if flag { 1 } else { 2 } }\n" with
  | .ok unit => match unit.declarations with
    | [.function function] => match function.body with
      | [.returnValue (some (.ifExpr [(_, .valueBlock [.expression (.literal (.integer "1"))])]
          (.valueBlock [.expression (.literal (.integer "2"))])))] =>
        decide (function.statements.map Prod.fst = [⟨0, 28, 31⟩, ⟨0, 45, 1⟩, ⟨0, 56, 1⟩]) &&
          decide (function.expressions.map Prod.fst =
            [⟨0, 35, 24⟩, ⟨0, 38, 4⟩, ⟨0, 43, 5⟩, ⟨0, 45, 1⟩, ⟨0, 54, 5⟩, ⟨0, 56, 1⟩])
      | _ => false
    | _ => false
  | _ => false

def admissionIsUnsupported (text : String) : Bool :=
  match admitSource 0 text with | .error .unsupported => true | _ => false

theorem source_admission_refuses_bare_core_commands :
    admissionIsUnsupported "proc f() { print }\n" = true ∧
    admissionIsUnsupported "pure f(eprint: Int) -> Int { eprint }\n" = true ∧
    admissionIsUnsupported "pure f(print: Int) -> Int { return print }\n" = false := by
  constructor <;> decide +kernel

theorem source_admission_refuses_discard_names :
    admissionIsUnsupported "pure f() { let _ = 1; 2 }\n" = true ∧
    admissionIsUnsupported "pure f(_) { 1 }\n" = true := by
  constructor <;> decide +kernel

theorem source_admission_keeps_if_forms_and_rejects_ambiguous_syntax :
    statementIfCheck = true ∧ expressionIfCheck = true ∧
    admissionIsUnsupported "pure f(x) { x ?? 1 }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x?.value }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x?[0] }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x < 1 < 2 }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x.amount-1 }\n" = true := by
  constructor <;> decide +kernel

def sourceNameResolver (unit : SourceUnit) : NameResolver := fun source name =>
  if source = unit.source then sourceName unit.names name else none

def decodeAdmittedFacts (fuel : Nat) (unit : SourceUnit) (variables : VariableResolver)
    (codes : Xsh.Conformance.Transport.ExchangeCodes) : Option FactsDocument :=
  decodeFactsDocument fuel (sourceNameResolver unit) variables codes

theorem source_resolver_is_injective (unit : SourceUnit) (source : Nat)
    (first second : String) (id : Xsh.Foundation.NameId)
    (left : sourceNameResolver unit source first = some id)
    (right : sourceNameResolver unit source second = some id) : first = second := by
  by_cases same : source = unit.source
  · simp [sourceNameResolver, same] at left right
    exact source_name_injective unit.names first second id left right
  · simp [sourceNameResolver, same] at left

theorem source_resolver_rejects_other_sources (unit : SourceUnit) (source : Nat)
    (different : source ≠ unit.source) (name : String) :
    sourceNameResolver unit source name = none := by
  simp [sourceNameResolver, different]

def admissionIsAccepted (text : String) : Bool :=
  match admitSource 0 text with | .ok _ => true | _ => false

theorem source_admission_checks_try_adjacency :
    admissionIsAccepted "pure f(x) { return x? + 1 }\n" = true ∧
    admissionIsUnsupported "pure f(x) { return x ? + 1 }\n" = true := by
  constructor <;> decide +kernel

def tailNameAdmissionCheck : Bool :=
  match admitSource 0 "pure f(x) { x }\n" with
  | .ok unit => match unit.declarations with
    | [.function function] => match function.body, sourceName unit.names "x" with
      | [.tailName seen], some expected => decide (seen = expected) && function.expressions.isEmpty
      | _, _ => false
    | _ => false
  | _ => false

def terminatorSitesCheck : Bool :=
  match admitSource 0 "pure f() { let x = 1; return x }\n" with
  | .ok unit => match unit.declarations with
    | [.function function] =>
      decide (function.statements.map Prod.fst = [⟨0, 11, 10⟩, ⟨0, 22, 8⟩])
    | _ => false
  | _ => false

theorem source_admission_retains_tail_and_terminator_sites :
    tailNameAdmissionCheck = true ∧ terminatorSitesCheck = true ∧
    admissionIsUnsupported "pure f(a: Int) -> Int { return a-1 }\n" = true ∧
    admissionIsUnsupported "pure\rf() { 1 }\n" = true ∧
    admissionIsUnsupported "pure f() -> Str { \"${1}\" }\n" = true ∧
    admissionIsUnsupported "pure f(x) { return x.require(Int) }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x; 1 }\n" = true ∧
    admissionIsUnsupported "pure f(x) { x -1 }\n" = true ∧
    admissionIsUnsupported "pure f(x) { f (x) }\n" = true ∧
    admissionIsUnsupported "pure f(x) { foo.bar (x) }\n" = true ∧
    admissionIsAccepted "pure f(x) { return f (x) }\n" = true ∧
    admissionIsUnsupported "print\n" = true := by
  constructor <;> decide +kernel

end Xsh.Conformance.Connected
