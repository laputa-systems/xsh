import Xsh.Conformance.Vertical.SourceAdmissionBase
import Xsh.Conformance.Vertical.LoweringSound

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

theorem take_codes_complete (keep : Nat → Bool) (leading rest : List Nat)
    (kept : leading.all keep = true) (boundary : Boundary keep rest) :
    takeCodesWhile keep (leading ++ rest) = (leading, rest) := by
  induction leading with
  | nil =>
    cases rest with
    | nil => rfl
    | cons head tail =>
      have stopped := boundary head tail rfl
      simp [takeCodesWhile, stopped]
  | cons head tail ih =>
    simp only [List.all_cons, Bool.and_eq_true] at kept
    simp [takeCodesWhile, kept.1, ih kept.2]

theorem lexeme_nonempty {rest leading : List Nat} {token : Token}
    (lexeme : Lexeme rest leading token) : leading ≠ [] := by
  cases lexeme with
  | punctuation punctuation maximal =>
    simp [Punctuation] at punctuation
    rcases punctuation with h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h
    all_goals rcases h with ⟨rfl, rfl⟩ <;> simp
  | decimal nonempty digits boundary => exact nonempty
  | word first letters boundary => simp

theorem lexed_tokens_fit_in_source {codes : List Nat} {tokens : List Token}
    (lexed : Lexed codes tokens) : tokens.length ≤ codes.length := by
  induction lexed with
  | empty => simp
  | space lexed ih => simp only [List.length_cons]; omega
  | @token leading rest token tokens lexical tail ih =>
    have positive : 0 < leading.length := List.length_pos_iff.mpr (lexeme_nonempty lexical)
    simp only [List.length_cons, List.length_append]
    omega

theorem lexeme_consumes_source {rest leading : List Nat} {token : Token}
    (lexeme : Lexeme rest leading token) : rest.length < (leading ++ rest).length := by
  have positive := List.length_pos_iff.mpr (lexeme_nonempty lexeme)
  simp only [List.length_append]
  omega

theorem tokenizer_complete_fuel {codes : List Nat} {tokens : List Token}
    (lexical : Lexed codes tokens) : ∀ fuel, codes.length < fuel →
      tokenize fuel codes = some tokens := by
  induction lexical with
  | empty => intro fuel enough; cases fuel <;> rfl
  | @space codes tokens tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : codes.length < fuel := by simp at enough; omega
      simpa [tokenize] using ih fuel bound
  | @token leading rest token tokens lexeme tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : rest.length < fuel := by
        have consumed := lexeme_consumes_source lexeme
        omega
      have parsed := ih fuel bound
      cases lexeme with
      | punctuation punctuation maximal =>
        simp [Punctuation] at punctuation
        rcases punctuation with h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h
        all_goals rcases h with ⟨rfl, rfl⟩
        all_goals simp [tokenize, parsed] at *
        all_goals cases rest with
          | nil => simp [tokenize, parsed]
          | cons head tail =>
            cases head with
            | zero => simp [tokenize, parsed]
            | succ head =>
              cases head <;> simp_all [tokenize]
      | decimal nonempty allDigits boundary =>
        cases leading with
        | nil => contradiction
        | cons head more =>
          have digit : isDigitCode head = true := (List.all_eq_true.mp allDigits) head (by simp)
          have bounds : 48 ≤ head ∧ head ≤ 57 := by simpa [isDigitCode] using digit
          have taken := take_codes_complete isDigitCode (head :: more) rest allDigits boundary
          simp only [List.cons_append] at taken ⊢
          simp [tokenize, digit, taken, parsed, show head ≠ 32 by omega, show head ≠ 10 by omega, show head ≠ 40 by omega, show head ≠ 41 by omega, show head ≠ 123 by omega, show head ≠ 125 by omega, show head ≠ 91 by omega, show head ≠ 93 by omega, show head ≠ 61 by omega, show head ≠ 58 by omega, show head ≠ 44 by omega, show head ≠ 63 by omega, show head ≠ 62 by omega, show head ≠ 60 by omega, show head ≠ 43 by omega, show head ≠ 42 by omega, show head ≠ 47 by omega, show head ≠ 37 by omega, show head ≠ 45 by omega, show head ≠ 46 by omega]
      | @word head more first letters boundary =>
        have bounds : ((97 ≤ head ∧ head ≤ 122) ∨ (65 ≤ head ∧ head ≤ 90)) ∨ head = 95 := by
          simpa [isWordStartCode, Bool.or_eq_true, Bool.and_eq_true] using first
        have nondigit : isDigitCode head = false := by simp [isDigitCode]; omega
        have taken := take_codes_complete isWordPartCode (head :: more) rest letters boundary
        simp only [List.cons_append] at taken ⊢
        simp [tokenize, first, nondigit, taken, parsed, show head ≠ 32 by omega, show head ≠ 10 by omega, show head ≠ 40 by omega, show head ≠ 41 by omega, show head ≠ 123 by omega, show head ≠ 125 by omega, show head ≠ 91 by omega, show head ≠ 93 by omega, show head ≠ 61 by omega, show head ≠ 58 by omega, show head ≠ 44 by omega, show head ≠ 63 by omega, show head ≠ 62 by omega, show head ≠ 60 by omega, show head ≠ 43 by omega, show head ≠ 42 by omega, show head ≠ 47 by omega, show head ≠ 37 by omega, show head ≠ 45 by omega, show head ≠ 46 by omega]

theorem tokenizer_complete {codes : List Nat} {tokens : List Token}
    (lexical : Lexed codes tokens) : tokenize (codes.length + 1) codes = some tokens :=
  tokenizer_complete_fuel lexical _ (by omega)

theorem atom_syntax_complete {tokens rest : List Token} {atom : SourceAtom}
    (derivation : AtomSyntax tokens atom rest) : parseAtom tokens = some (atom, rest) := by
  cases derivation with
  | number => rfl
  | name text rest valid => simp [parseAtom, valid]

theorem value_syntax_complete {tokens rest : List Token} {value : SourceValue}
    (derivation : ValueSyntax tokens value rest)
    (boundary : ∀ op more, rest ≠ .operator op :: more) :
    parseValue tokens = some (value, rest) := by
  cases derivation with
  | atom atom =>
    have parsed := atom_syntax_complete atom
    cases rest with
    | nil => simp [parseValue, parsed]
    | cons head tail =>
      cases head <;> simp_all [parseValue]
  | binary left valid right =>
    simp [parseValue, atom_syntax_complete left, valid, atom_syntax_complete right]

theorem record_operand_complete {tokens rest : List Token} {value : RecordOperand}
    (derivation : RecordOperandSyntax tokens value rest)
    (boundary : ∀ label more, rest ≠ .dot :: .word label :: more) :
    parseRecordOperand tokens = some (value, rest) := by
  cases derivation with
  | atom atom =>
    cases atom with
    | number value rest => simp [parseRecordOperand, parseAtom]
    | name name rest valid =>
      cases rest with
      | nil => simp [parseRecordOperand, parseAtom, valid]
      | cons head tail =>
        cases head <;> simp_all [parseRecordOperand, parseAtom]
  | field target label rest validTarget validLabel =>
    simp [parseRecordOperand, validTarget, validLabel]

theorem record_value_line_complete {tokens rest : List Token} {value : RecordValue}
    (derivation : RecordValueSyntax tokens value (.newline :: rest)) :
    parseRecordValue tokens = some (value, .newline :: rest) := by
  cases derivation with
  | operand operand =>
    have parsed := record_operand_complete operand (by intro label more impossible; cases impossible)
    simp [parseRecordValue, parsed]
  | binary left valid right =>
    have parsedLeft := record_operand_complete left (by intro label more impossible; cases impossible)
    have parsedRight := record_operand_complete right (by intro label more impossible; cases impossible)
    simp [parseRecordValue, parsedLeft, valid, parsedRight]

theorem value_consumes_tokens {tokens rest : List Token} {value : SourceValue}
    (derivation : ValueSyntax tokens value rest) : rest.length < tokens.length := by
  cases derivation with
  | atom atom => cases atom <;> simp
  | binary left valid right => cases left <;> cases right <;> simp <;> omega

theorem record_fields_fit_tokens {tokens rest : List Token} {fields : List (String × SourceValue)}
    (derivation : RecordFieldsSyntax tokens fields rest) : fields.length ≤ tokens.length := by
  induction derivation with
  | single => simp
  | cons valid value tail ih =>
    have consumed := value_consumes_tokens value
    simp_all <;> omega

theorem record_fields_complete {tokens rest : List Token} {fields : List (String × SourceValue)}
    (derivation : RecordFieldsSyntax tokens fields rest) :
    ∀ fuel, fields.length ≤ fuel → parseRecordFields fuel tokens = some (fields, rest) := by
  induction derivation with
  | @single name value tokens rest valid field =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have parsed := value_syntax_complete field (by intro op more impossible; cases impossible)
      simp [parseRecordFields, valid, parsed]
  | @cons name value tokens afterValue rest others valid field tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : others.length ≤ fuel := by simp at enough; omega
      have parsed := value_syntax_complete field (by intro op more impossible; cases impossible)
      simp [parseRecordFields, valid, parsed, ih fuel bound]

theorem record_let_complete {tokens rest : List Token} {target : String}
    {fields : List (String × SourceValue)} (derivation : RecordLetSyntax tokens target fields rest) :
    parseRecordLet tokens = some ((target, fields), rest) := by
  cases derivation with
  | @record target fields tokens rest valid fieldSyntax =>
    have bound := record_fields_fit_tokens fieldSyntax
    have parsed := record_fields_complete fieldSyntax (tokens.length + 1) (by omega)
    simp [parseRecordLet, valid, parsed]

theorem arguments_fit_tokens {tokens rest : List Token} {arguments : List SourceAtom}
    (derivation : ArgumentsSyntax tokens arguments rest) : arguments.length ≤ tokens.length := by
  induction derivation with
  | empty => simp
  | single first => cases first <;> simp
  | cons first tail nonempty ih => cases first <;> simp_all <;> omega

theorem parameters_fit_tokens {tokens rest : List Token} {parameters : List String}
    (derivation : ParametersSyntax tokens parameters rest) : parameters.length ≤ tokens.length := by
  induction derivation with
  | empty => simp
  | single valid => simp
  | cons valid tail nonempty ih => simp_all <;> omega

theorem argument_list_complete {tokens rest : List Token} {arguments : List SourceAtom}
    (derivation : ArgumentsSyntax tokens arguments rest) (nonempty : arguments ≠ []) :
    ∀ fuel, arguments.length ≤ fuel → parseArgumentList fuel tokens = some (arguments, rest) := by
  induction derivation with
  | empty rest => contradiction
  | @single tokens rest atom first =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel => simp [parseArgumentList, atom_syntax_complete first]
  | @cons tokens afterFirst rest atom others first tail othersNonempty ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : others.length ≤ fuel := by simp at enough; omega
      simp [parseArgumentList, atom_syntax_complete first, ih othersNonempty fuel bound]

theorem arguments_complete {tokens rest : List Token} {arguments : List SourceAtom}
    (derivation : ArgumentsSyntax tokens arguments rest) :
    ∀ fuel, arguments.length ≤ fuel → parseArguments fuel tokens = some (arguments, rest) := by
  intro fuel enough
  cases derivation with
  | empty rest => rfl
  | @single tokens rest atom first =>
    have parsed := argument_list_complete (ArgumentsSyntax.single first) (by simp) fuel enough
    cases first <;> simpa [parseArguments] using parsed
  | @cons tokens afterFirst rest atom others first tail othersNonempty =>
    have parsed := argument_list_complete (ArgumentsSyntax.cons first tail othersNonempty) (by simp) fuel enough
    cases first <;> simpa [parseArguments] using parsed

theorem parameter_list_complete {tokens rest : List Token} {parameters : List String}
    (derivation : ParametersSyntax tokens parameters rest) (nonempty : parameters ≠ []) :
    ∀ fuel, parameters.length ≤ fuel → parseParameterList fuel tokens = some (parameters, rest) := by
  induction derivation with
  | empty rest => contradiction
  | @single name rest valid =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel => simp [parseParameterList, valid]
  | @cons name tokens rest others valid tail othersNonempty ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : others.length ≤ fuel := by simp at enough; omega
      simp [parseParameterList, valid, ih othersNonempty fuel bound]

theorem parameters_complete {tokens rest : List Token} {parameters : List String}
    (derivation : ParametersSyntax tokens parameters rest) :
    ∀ fuel, parameters.length ≤ fuel → parseParameters fuel tokens = some (parameters, rest) := by
  intro fuel enough
  cases derivation with
  | empty rest => rfl
  | @single name rest valid =>
    simpa [parseParameters] using parameter_list_complete (ParametersSyntax.single valid) (by simp) fuel enough
  | @cons name tokens rest others valid tail othersNonempty =>
    simpa [parseParameters] using parameter_list_complete (ParametersSyntax.cons valid tail othersNonempty) (by simp) fuel enough

theorem expression_line_complete {tokens rest : List Token} {expression : SourceExpression}
    (derivation : ExpressionSyntax tokens expression (.newline :: rest)) :
    parseExpression tokens = some (expression, .newline :: rest) := by
  cases derivation with
  | value value =>
    cases value with
    | atom atom => cases atom <;> simp_all [parseExpression, parseAtom, isKeyword]
    | binary left valid right =>
      cases left <;> cases right <;> simp_all [parseExpression, parseAtom, isKeyword]
  | comparison left valid right =>
    cases left <;> cases right <;> simp_all [parseExpression, parseAtom, isKeyword]
  | @call name ts _ arguments valid notOk notErr args =>
    have parsed := arguments_complete args (ts.length + 1) (by have := arguments_fit_tokens args; omega)
    unfold parseExpression
    split <;> simp_all [isKeyword]
  | ok value =>
    have parsed := value_syntax_complete value (by intro op more impossible; cases impossible)
    simp [parseExpression, parseConstructor, parsed]
  | err value =>
    have parsed := value_syntax_complete value (by intro op more impossible; cases impossible)
    simp [parseExpression, parseConstructor, parsed]
  | conditional left valid right thenValue elseValue =>
    have parsedThen := value_syntax_complete thenValue (by intro op more impossible; cases impossible)
    have parsedElse := value_syntax_complete elseValue (by intro op more impossible; cases impossible)
    simp [parseExpression, atom_syntax_complete left, valid, atom_syntax_complete right, parsedThen, parsedElse]

def beginsLiteralLet : List Token → Bool
  | .word keyword :: .word _ :: .equals :: .number _ :: .newline :: _ =>
      keyword == "let" || keyword == "var"
  | _ => false

theorem lets_complete {tokens rest : List Token} {lets : List (String × Nat)} {vars : List String}
    (derivation : LetsSyntax tokens lets vars rest) (boundary : beginsLiteralLet rest = false) :
    ∀ fuel, lets.length < fuel → parseLets fuel tokens = some (lets, vars, rest) := by
  induction derivation with
  | empty tokens =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      unfold beginsLiteralLet at boundary
      unfold parseLets
      split <;> simp_all
  | @immutable name value tokens rest lets vars tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : lets.length < fuel := by simp at enough; omega
      simp [parseLets, ih boundary fuel bound]
  | @mutable name value tokens rest lets vars tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : lets.length < fuel := by simp at enough; omega
      simp [parseLets, ih boundary fuel bound]

def ComputedBoundary (tokens : List Token) : Prop :=
  ∀ name op left right valueTokens after,
    tokens = .word "let" :: .word name :: .equals :: valueTokens →
    isKeyword name = false →
    ValueSyntax valueTokens (.binary op left right) (.newline :: after) → False

theorem computed_complete {tokens rest : List Token} {bindings : List (String × SourceValue)}
    (derivation : ComputedSyntax tokens bindings rest) (boundary : ComputedBoundary rest) :
    ∀ fuel, bindings.length < fuel → parseComputedLets fuel tokens = some (bindings, rest) := by
  induction derivation with
  | empty tokens =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      unfold parseComputedLets
      split
      · rename_i tail name same
        split
        · rfl
        · rename_i valid
          split
          · rename_i op left right after parsed
            exfalso
            exact boundary name op left right same after rfl (by simpa using valid) (value_syntax parsed)
          · rfl
      · rfl
  | @cons name op left right tokens afterValue rest others valid value tail ih =>
    intro fuel enough
    cases fuel with
    | zero => simp at enough
    | succ fuel =>
      have bound : others.length < fuel := by simp at enough; omega
      have parsed := value_syntax_complete value (by intro op more impossible; cases impossible)
      simp [parseComputedLets, valid, parsed, ih boundary fuel bound]

theorem lets_fit_tokens {tokens rest : List Token} {lets : List (String × Nat)} {vars : List String}
    (derivation : LetsSyntax tokens lets vars rest) : lets.length ≤ tokens.length := by
  induction derivation with
  | empty => simp
  | immutable tail ih => simp_all <;> omega
  | mutable tail ih => simp_all <;> omega

theorem computed_fit_tokens {tokens rest : List Token} {bindings : List (String × SourceValue)}
    (derivation : ComputedSyntax tokens bindings rest) : bindings.length ≤ tokens.length := by
  induction derivation with
  | empty => simp
  | cons valid value tail ih =>
    have consumed := value_consumes_tokens value
    simp_all <;> omega

theorem propagation_complete {tokens rest : List Token} {propagation : Option SourcePropagation}
    (derivation : PropagationSyntax tokens propagation rest)
    (boundary : propagation = none → ∀ target callee more,
      tokens ≠ .word "let" :: .word target :: .equals :: .word callee :: .leftParen :: more) :
    parsePropagation tokens = some (propagation, rest) := by
  cases derivation with
  | empty tokens =>
    unfold parsePropagation
    split
    · rename_i after target callee same
      exact False.elim (boundary rfl target callee same rfl)
    · rfl
  | @call target callee tokens rest arguments validTarget validCallee args =>
    have bound := arguments_fit_tokens args
    have parsed := arguments_complete args (tokens.length + 1) (by omega)
    simp [parsePropagation, validTarget, validCallee, parsed]

theorem assignment_complete {vars : List String} {tokens rest : List Token}
    {assignment : Option (String × SourceValue)}
    (derivation : AssignmentSyntax vars tokens assignment rest)
    (boundary : assignment = none → ∀ target more, tokens ≠ .word target :: .equals :: more) :
    parseAssignment vars tokens = some (assignment, rest) := by
  cases derivation with
  | empty tokens =>
    unfold parseAssignment
    split
    · rename_i more target same
      exact False.elim (boundary rfl target same rfl)
    · rfl
  | assign member value =>
    have parsed := value_syntax_complete value (by intro op more impossible; cases impossible)
    simp [parseAssignment, List.contains_iff_mem, member, parsed]

theorem record_body_complete {name parameter target : String} {fields : List (String × SourceValue)}
    {value : RecordValue} {tokens afterLet afterValue rest : List Token}
    (record : RecordLetSyntax tokens target fields (.word "return" :: afterLet))
    (returned : RecordValueSyntax afterLet value afterValue)
    (admitted : recordLocalAdmitted parameter target fields value = true)
    (closing : ClosingSyntax afterValue rest) :
    parseRecordBody .integer name [parameter] tokens =
      some (⟨name, [parameter], [], none, .recordLocal target fields value⟩, rest) := by
  have parsedRecord := record_let_complete record
  cases closing with
  | endOfInput =>
    have parsedValue := record_value_line_complete returned
    simp [parseRecordBody, parsedRecord, parsedValue, admitted]
  | more rest =>
    have parsedValue := record_value_line_complete returned
    simp [parseRecordBody, parsedRecord, parsedValue, admitted]

theorem assigned_result_complete {assignment : Option (String × SourceValue)}
    {returned result : SourceExpression} (derivation : AssignedResult assignment returned result) :
    attachAssignment assignment returned = some result := by
  cases derivation <;> rfl

theorem computed_result_complete {bindings : List (String × SourceValue)}
    {returned result : SourceExpression} (derivation : ComputedResult bindings returned result) :
    buildComputedResult bindings returned = some result := by
  cases derivation <;> rfl

theorem return_type_complete {tokens rest : List Token} {shape : ResultShape}
    (derivation : ReturnTypeSyntax tokens shape rest) :
    parseReturnType tokens = some (shape, rest) := by
  cases derivation <;> rfl

theorem ordinary_body_complete {shape : ResultShape} {name : String} {parameters : List String}
    {tokens afterLets afterComputed afterPropagation afterAssignment expressionTokens closing rest : List Token}
    {lets : List (String × Nat)} {vars : List String} {computed : List (String × SourceValue)}
    {propagation : Option SourcePropagation} {assignment : Option (String × SourceValue)}
    {returned assigned result : SourceExpression}
    (literalSyntax : LetsSyntax tokens lets vars afterLets)
    (computedSyntax : ComputedSyntax afterLets computed afterComputed)
    (propagationSyntax : PropagationSyntax afterComputed propagation afterPropagation)
    (assignmentSyntax : AssignmentSyntax vars afterPropagation assignment afterAssignment)
    (propagationAllowed : PropagationAllowed parameters lets assignment shape propagation)
    (computedAllowed : ComputedAllowed parameters lets assignment propagation shape computed)
    (returnLine : afterAssignment = .word "return" :: expressionTokens)
    (expressionSyntax : ExpressionSyntax expressionTokens returned closing)
    (assignedSyntax : AssignedResult assignment returned assigned)
    (computedResult : ComputedResult computed assigned result)
    (resultAllowed : admitsResult shape result.shape = true)
    (fresh : ∀ found, propagation = some found → found.target ∉ parameters)
    (closingSyntax : ClosingSyntax closing rest)
    (literalBoundary : beginsLiteralLet afterLets = false)
    (computedBoundary : ComputedBoundary afterComputed)
    (propagationBoundary : propagation = none → ∀ target callee more,
      afterComputed ≠ .word "let" :: .word target :: .equals :: .word callee :: .leftParen :: more)
    (assignmentBoundary : assignment = none → ∀ target more,
      afterPropagation ≠ .word target :: .equals :: more) :
    parseOrdinaryFunctionBody shape name parameters tokens =
      some (⟨name, parameters, lets, propagation, result⟩, rest) := by
  have parsedLets := lets_complete literalSyntax literalBoundary (tokens.length + 1)
    (by have := lets_fit_tokens literalSyntax; omega)
  have parsedComputed := computed_complete computedSyntax computedBoundary (afterLets.length + 1)
    (by have := computed_fit_tokens computedSyntax; omega)
  have parsedPropagation := propagation_complete propagationSyntax propagationBoundary
  have parsedAssignment := assignment_complete assignmentSyntax assignmentBoundary
  have parsedAssigned := assigned_result_complete assignedSyntax
  have parsedResult := computed_result_complete computedResult
  cases closingSyntax <;> have parsedExpression := expression_line_complete expressionSyntax <;>
    cases propagation <;> cases computed
  all_goals simp only [PropagationAllowed, ComputedAllowed, Option.some_ne_none, false_and, and_false,
      true_and, and_true] at propagationAllowed computedAllowed
  all_goals try contradiction
  all_goals try rcases propagationAllowed with ⟨rfl, rfl, rfl, pLen⟩
  all_goals try rcases computedAllowed with ⟨rfl, rfl, rfl, cLen, cBindings⟩
  all_goals simp [parseOrdinaryFunctionBody, parsedLets, parsedComputed, parsedPropagation,
      parsedAssignment, returnLine, parsedExpression, parsedAssigned, parsedResult,
      resultAllowed, fresh, List.contains_iff_mem]
  all_goals try assumption
  all_goals constructor <;> assumption

theorem assignment_boundary_from_return {vars : List String} {tokens afterAssignment expressionTokens : List Token}
    {assignment : Option (String × SourceValue)}
    (derivation : AssignmentSyntax vars tokens assignment afterAssignment)
    (returnLine : afterAssignment = .word "return" :: expressionTokens)
    {returned : SourceExpression} {closing : List Token}
    (expressionSyntax : ExpressionSyntax expressionTokens returned closing) :
    assignment = none → ∀ target more, tokens ≠ .word target :: .equals :: more := by
  cases derivation with
  | empty tokens =>
    intro _ target more same
    rw [returnLine] at same
    cases same
    cases expressionSyntax with
    | value value =>
      cases value with
      | atom atom => cases atom
      | binary left valid right => cases left
    | comparison left valid right => cases left
  | assign member value => simp

theorem propagation_boundary_from_return {vars : List String}
    {tokens afterPropagation afterAssignment expressionTokens : List Token}
    {propagation : Option SourcePropagation} {assignment : Option (String × SourceValue)}
    (propagationSyntax : PropagationSyntax tokens propagation afterPropagation)
    (assignmentSyntax : AssignmentSyntax vars afterPropagation assignment afterAssignment)
    (returnLine : afterAssignment = .word "return" :: expressionTokens) :
    propagation = none → ∀ target callee more,
      tokens ≠ .word "let" :: .word target :: .equals :: .word callee :: .leftParen :: more := by
  cases propagationSyntax with
  | empty tokens =>
    intro _ target callee more same
    cases assignmentSyntax with
    | empty => rw [returnLine] at same; simp at same
    | assign member value => cases same
  | call validTarget validCallee args => simp

theorem computed_boundary_from_return {vars : List String}
    {tokens afterPropagation afterAssignment expressionTokens : List Token}
    {propagation : Option SourcePropagation} {assignment : Option (String × SourceValue)}
    (propagationSyntax : PropagationSyntax tokens propagation afterPropagation)
    (assignmentSyntax : AssignmentSyntax vars afterPropagation assignment afterAssignment)
    (returnLine : afterAssignment = .word "return" :: expressionTokens) :
    ComputedBoundary tokens := by
  intro name op left right valueTokens after same valid value
  cases propagationSyntax with
  | empty tokens =>
    cases assignmentSyntax with
    | empty => rw [returnLine] at same; simp at same
    | assign member value => cases same
  | call validTarget validCallee args =>
    simp only [List.cons.injEq, Token.word.injEq, true_and] at same
    rcases same with ⟨_, _, _, _, rfl⟩
    cases value with
    | binary first valid second => cases first

theorem literal_boundary_from_return {vars : List String}
    {tokens afterComputed afterPropagation afterAssignment expressionTokens : List Token}
    {bindings : List (String × SourceValue)} {propagation : Option SourcePropagation}
    {assignment : Option (String × SourceValue)}
    (computedSyntax : ComputedSyntax tokens bindings afterComputed)
    (propagationSyntax : PropagationSyntax afterComputed propagation afterPropagation)
    (assignmentSyntax : AssignmentSyntax vars afterPropagation assignment afterAssignment)
    (returnLine : afterAssignment = .word "return" :: expressionTokens) :
    beginsLiteralLet tokens = false := by
  cases computedSyntax with
  | empty tokens =>
    cases propagationSyntax with
    | empty =>
      cases assignmentSyntax with
      | empty =>
        rw [returnLine]
        unfold beginsLiteralLet
        split
        · rename_i tail keyword label value same
          simp only [List.cons.injEq, Token.word.injEq, true_and] at same
          rcases same with ⟨rfl, _⟩
          decide
        · rfl
      | assign member value => simp [beginsLiteralLet]
    | call validTarget validCallee args => simp [beginsLiteralLet]
  | cons valid value tail =>
    cases value with
    | binary first valid right => cases first <;> simp [beginsLiteralLet]

theorem ordinary_body_not_record {vars : List String}
    {tokens afterLets afterComputed afterPropagation afterAssignment expressionTokens : List Token}
    {lets : List (String × Nat)} {bindings : List (String × SourceValue)}
    {propagation : Option SourcePropagation} {assignment : Option (String × SourceValue)}
    (literalSyntax : LetsSyntax tokens lets vars afterLets)
    (computedSyntax : ComputedSyntax afterLets bindings afterComputed)
    (propagationSyntax : PropagationSyntax afterComputed propagation afterPropagation)
    (assignmentSyntax : AssignmentSyntax vars afterPropagation assignment afterAssignment)
    (returnLine : afterAssignment = .word "return" :: expressionTokens) :
    ∀ target more, tokens ≠ .word "let" :: .word target :: .equals :: .leftBrace :: more := by
  intro target more same
  cases literalSyntax with
  | empty =>
    cases computedSyntax with
    | empty =>
      cases propagationSyntax with
      | empty =>
        cases assignmentSyntax with
        | empty => rw [returnLine] at same; simp at same
        | assign member value => cases same
      | call validTarget validCallee args => cases same
    | cons valid value tail =>
      simp only [List.cons.injEq, Token.word.injEq, true_and] at same
      rcases same with ⟨_, rfl⟩
      cases value with
      | binary left valid right => cases left
  | immutable tail => cases same
  | mutable tail => simp at same

theorem body_syntax_complete {shape : ResultShape} {name : String} {parameters : List String}
    {tokens rest : List Token} {function : SourceFunction}
    (derivation : BodySyntax shape name parameters tokens function rest) :
    parseFunctionBody shape name parameters tokens = some (function, rest) := by
  cases derivation with
  | body literal computed propagation assignment propAllowed compAllowed returnLine expression assigned result resultAllowed fresh closing =>
    have parsed := ordinary_body_complete (name := name) literal computed propagation assignment propAllowed compAllowed
      returnLine expression assigned result resultAllowed fresh closing
      (literal_boundary_from_return computed propagation assignment returnLine)
      (computed_boundary_from_return propagation assignment returnLine)
      (propagation_boundary_from_return propagation assignment returnLine)
      (assignment_boundary_from_return assignment returnLine expression)
    unfold parseFunctionBody
    split
    · rename_i more target same
      exact False.elim (ordinary_body_not_record literal computed propagation assignment returnLine target same rfl)
    · exact parsed
  | record shape parameters record returned admitted closing =>
    subst shape
    subst parameters
    have parsed := record_body_complete (name := name) record returned admitted closing
    cases record with
    | record valid fields => simpa [parseFunctionBody] using parsed

theorem quote_byte_outside_domain : ¬ Domain [34] [] := by
  intro domain
  obtain ⟨functions, scopes, admitted⟩ := domain
  rcases admitted with ⟨_, _, _, _, _, _, tokens, lexical, _⟩
  have parsed := tokenizer_complete lexical
  simp [tokenize, isDigitCode, isWordStartCode] at parsed

theorem constructor_is_not_an_ordinary_call :
    ¬ ExpressionSyntax [.word "Ok", .leftParen, .word "x", .rightParen]
        (.call "Ok" [.var "x"]) [] ∧
      parseExpression [.word "Ok", .leftParen, .word "x", .rightParen] =
        some (.constructed true (.atom (.var "x")), []) := by
  constructor
  · intro derivation
    have callName {tokens rest : List Token} {name : String} {arguments : List SourceAtom}
        (parsed : ExpressionSyntax tokens (.call name arguments) rest) : name ≠ "Ok" := by
      cases parsed with
      | call _ notOk _ _ => exact notOk
    exact callName derivation rfl
  · decide +kernel

theorem computed_scope_complete {parameter : String} {slot : Nat}
    {bindings : List (String × SourceValue)} {scope : List (String × Nat)}
    (projection : ComputedScope parameter slot bindings scope) :
    computedLetsScope? parameter slot bindings = some scope := by
  induction projection with
  | empty => rfl
  | cons distinct fresh tail ih =>
    simp [computedLetsScope?, distinct, fresh, ih]

theorem scope_projection_complete {function : SourceFunction} {scope : List (String × Nat)}
    (projection : ScopeProjection function scope) : function.scope? = some scope := by
  cases projection with
  | record name parameter target fields value distinct =>
    simp [SourceFunction.scope?, distinct]
  | propagated name parameter propagation result distinct =>
    simp [SourceFunction.scope?, distinct]
  | literal => rfl
  | parameterLiteral name parameter localName value result unassigned distinct =>
    simp [SourceFunction.scope?, unassigned, distinct]
  | computed name parameter localName value result distinct =>
    simp [SourceFunction.scope?, distinct]
  | computedSeveral name parameter bindings result scope projected =>
    simp [SourceFunction.scope?, computed_scope_complete projected]
  | single name parameter result ordinary =>
    cases result <;> simp_all [SourceFunction.scope?, OrdinaryResult]
  | several name first second rest result distinct =>
    simp [SourceFunction.scope?, distinct]

theorem scopes_projection_complete {functions : List SourceFunction}
    {scopes : List (List (String × Nat))} (projection : ScopesProjection functions scopes) :
    functions.mapM SourceFunction.scope? = some scopes := by
  induction projection with
  | empty => rfl
  | cons head tail ih =>
    simp [List.mapM_cons, scope_projection_complete head, ih]

theorem decimal_step_preserves_failure (state : Nat × Bool) (code : Nat)
    (successful : (decimalRunStep state code).2 = true) : state.2 = true := by
  unfold decimalRunStep at successful
  split at successful
  · simp only [Bool.and_eq_true] at successful
    exact successful.1
  · exact successful

theorem decimal_fold_preserves_failure (codes : List Nat) (state : Nat × Bool)
    (successful : (codes.foldl decimalRunStep state).2 = true) : state.2 = true := by
  induction codes generalizing state with
  | nil => exact successful
  | cons code rest ih =>
    apply decimal_step_preserves_failure state code
    exact ih (decimalRunStep state code) successful

theorem decimal_digits_fold_value (digits : List Nat) (value : Nat) (ok : Bool)
    (decimal : digits.all isDigitCode = true) :
    (digits.foldl decimalRunStep (value, ok)).1 =
      digits.foldl (fun current digit => current * 10 + (digit - 48)) value := by
  induction digits generalizing value ok with
  | nil => rfl
  | cons digit rest ih =>
    simp only [List.all_cons, Bool.and_eq_true] at decimal
    simp only [List.foldl_cons, decimalRunStep, decimal.1, ↓reduceIte]
    exact ih _ _ decimal.2

theorem decimal_digits_fold_monotone (digits : List Nat) (left right : Nat)
    (ordered : left ≤ right) :
    digits.foldl (fun current digit => current * 10 + (digit - 48)) left ≤
      digits.foldl (fun current digit => current * 10 + (digit - 48)) right := by
  induction digits generalizing left right with
  | nil => exact ordered
  | cons digit rest ih =>
    simp only [List.foldl_cons]
    apply ih
    omega

theorem nonempty_has_last {α : Type} (items : List α) (nonempty : items ≠ []) :
    ∃ front last, items = front ++ [last] := by
  induction items with
  | nil => exact False.elim (nonempty rfl)
  | cons head tail ih =>
    cases tail with
    | nil => exact ⟨[], head, rfl⟩
    | cons second rest =>
      obtain ⟨front, last, same⟩ := ih (by simp)
      exact ⟨head :: front, last, by simp [same]⟩

theorem decimal_digit_segment_bounded (before digits after : List Nat)
    (nonempty : digits ≠ []) (decimal : digits.all isDigitCode = true)
    (safe : decimalRunsInRange (before ++ digits ++ after) = true) :
    digitsValue digits ≤ maxIntLiteral := by
  unfold decimalRunsInRange at safe
  simp only [List.foldl_append] at safe
  let state := before.foldl decimalRunStep (0, true)
  have middle : (digits.foldl decimalRunStep state).2 = true := by
    exact decimal_fold_preserves_failure after _ safe
  obtain ⟨front, last, same⟩ := nonempty_has_last digits nonempty
  subst digits
  have lastDigit : isDigitCode last = true := by
    simpa using (List.all_eq_true.mp decimal last (by simp))
  have bound : ((front ++ [last]).foldl decimalRunStep state).1 ≤ maxIntLiteral := by
    simp only [List.foldl_append, List.foldl_cons, List.foldl_nil, decimalRunStep,
      lastDigit, ↓reduceIte, Bool.and_eq_true, decide_eq_true_eq] at middle ⊢
    exact middle.2
  have lower := decimal_digits_fold_monotone (front ++ [last]) 0 state.1 (Nat.zero_le _)
  rw [← decimal_digits_fold_value (front ++ [last]) state.1 state.2 decimal] at lower
  exact Nat.le_trans lower bound

theorem no_conflicting_adjacency_tail {head : Nat} {rest : List Nat}
    (safe : noConflictingAdjacency (head :: rest) = true) : noConflictingAdjacency rest = true := by
  cases rest with
  | nil => rfl
  | cons next tail =>
    simp only [noConflictingAdjacency, Bool.and_eq_true] at safe
    exact safe.2

theorem no_conflicting_adjacency_suffix (before rest : List Nat)
    (safe : noConflictingAdjacency (before ++ rest) = true) : noConflictingAdjacency rest = true := by
  induction before with
  | nil => exact safe
  | cons head tail ih =>
    apply ih
    exact no_conflicting_adjacency_tail safe

theorem no_word_hyphen_in_guarded_source {codes before after : List Nat} {head : Nat}
    (safe : noConflictingAdjacency codes = true)
    (location : codes = before ++ head :: 45 :: after) : isWordPartCode head = false := by
  subst codes
  have suffix := no_conflicting_adjacency_suffix before (head :: 45 :: after) safe
  simp only [noConflictingAdjacency, Bool.and_eq_true, Bool.not_eq_true, beq_self_eq_true,
    Bool.and_true] at suffix
  simpa using suffix.1.1.1.1.1.1.1

inductive PositionedLexed : Nat → List Nat → List (Token × Nat × Nat) → Prop where
  | empty (offset : Nat) : PositionedLexed offset [] []
  | space {offset : Nat} {codes : List Nat} {tokens : List (Token × Nat × Nat)} :
      PositionedLexed (offset + 1) codes tokens → PositionedLexed offset (32 :: codes) tokens
  | token {offset : Nat} {leading rest : List Nat} {token : Token}
      {tokens : List (Token × Nat × Nat)} :
      Lexeme rest leading token → PositionedLexed (offset + leading.length) rest tokens →
      PositionedLexed offset (leading ++ rest) ((token, offset, offset + leading.length) :: tokens)

theorem lexed_has_positions {codes : List Nat} {tokens : List Token}
    (lexed : Lexed codes tokens) (offset : Nat) :
    ∃ positioned, PositionedLexed offset codes positioned ∧
      positioned.map (fun entry => entry.1) = tokens := by
  induction lexed generalizing offset with
  | empty => exact ⟨[], .empty offset, rfl⟩
  | space lexical ih =>
    obtain ⟨positioned, positions, projection⟩ := ih (offset + 1)
    exact ⟨positioned, .space positions, projection⟩
  | token lexical tail ih =>
    obtain ⟨positioned, positions, projection⟩ := ih (offset + _)
    exact ⟨_ :: positioned, .token lexical positions, by simp [projection]⟩

theorem positioned_spans_bounded {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens) :
    ∀ token start stop, (token, start, stop) ∈ tokens →
      offset ≤ start ∧ start ≤ stop ∧ stop ≤ offset + codes.length := by
  induction lexed with
  | empty => simp
  | space lexed ih =>
    intro token start stop member
    have bounds := ih token start stop member
    simp only [List.length_cons]
    omega
  | @token offset leading rest head tokens lexical tail ih =>
    intro token start stop member
    simp only [List.mem_cons, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl, rfl⟩ | member
    · simp only [List.length_append]
      omega
    · have bounds := ih token start stop member
      simp only [List.length_append]
      omega

theorem positioned_span_lexeme {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens) :
    ∀ token start stop, (token, start, stop) ∈ tokens →
      ∃ before leading rest, codes = before ++ leading ++ rest ∧
        start = offset + before.length ∧ stop = start + leading.length ∧ Lexeme rest leading token := by
  induction lexed with
  | empty => simp
  | @space offset codes tokens lexical ih =>
    intro token start stop member
    obtain ⟨before, leading, rest, same, first, last, lexeme⟩ := ih token start stop member
    refine ⟨32 :: before, leading, rest, ?_, ?_, last, lexeme⟩
    · simp [same]
    · simp only [List.length_cons]
      omega
  | @token offset leading rest head tokens lexical tail ih =>
    intro token start stop member
    simp only [List.mem_cons, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl, rfl⟩ | member
    · exact ⟨[], leading, rest, by simp, by simp, rfl, lexical⟩
    · obtain ⟨before, tokenBytes, remaining, same, first, last, lexeme⟩ :=
        ih token start stop member
      refine ⟨leading ++ before, tokenBytes, remaining, ?_, ?_, last, lexeme⟩
      · simp [same, List.append_assoc]
      · simp only [List.length_append]
        omega

theorem positioned_decimal_interpretation {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens)
    {value start stop : Nat} (member : (Token.number value, start, stop) ∈ tokens) :
    ∃ before digits rest, codes = before ++ digits ++ rest ∧
      start = offset + before.length ∧ stop = start + digits.length ∧
      digits ≠ [] ∧ digits.all isDigitCode = true ∧ Boundary isDigitCode rest ∧
      value = digitsValue digits := by
  obtain ⟨before, digits, rest, same, first, last, lexeme⟩ :=
    positioned_span_lexeme lexed (.number value) start stop member
  cases lexeme with
  | punctuation punctuation maximal => simp [Punctuation] at punctuation
  | decimal nonempty digitCodes boundary =>
    exact ⟨before, digits, rest, same, first, last, nonempty, digitCodes, boundary, rfl⟩

theorem guarded_positioned_decimal_bounded {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed 0 codes tokens)
    (safe : decimalRunsInRange codes = true) {value start stop : Nat}
    (member : (Token.number value, start, stop) ∈ tokens) : value ≤ maxIntLiteral := by
  obtain ⟨before, digits, rest, same, _, _, nonempty, decimal, _, value⟩ :=
    positioned_decimal_interpretation lexed member
  rw [value]
  apply decimal_digit_segment_bounded before digits rest nonempty decimal
  simpa [same] using safe

theorem ascii_byte_valid_utf8 (byte : UInt8) (ascii : byte.toNat < 128) :
    [byte].toByteArray.IsValidUTF8 := by
  let c := Char.ofNat byte.toNat
  have valid : byte.toNat.isValidChar := by
    left
    omega
  have bound : byte.toNat < UInt32.size := by
    simp [UInt32.size]
    omega
  have value : c.val = UInt32.ofNatLT byte.toNat bound := by
    simp only [c, Char.ofNat, dif_pos valid, Char.ofNatAux, UInt32.ofNatLT]
  have size : c.utf8Size = 1 := by
    apply Char.utf8Size_eq_one_iff.mpr
    rw [value, UInt32.le_iff_toNat_le, UInt32.toNat_ofNatLT]
    simp
    omega
  have encoded := Char.isValidUTF8_toByteArray_utf8EncodeChar (c := c)
  rw [String.utf8EncodeChar_eq_singleton size, value, UInt32.toUInt8_ofNatLT,
    UInt8.ofNat_toNat] at encoded
  exact encoded

theorem ascii_bytes_valid_utf8 (bytes : List UInt8)
    (ascii : ∀ byte, byte ∈ bytes → byte.toNat < 128) : bytes.toByteArray.IsValidUTF8 := by
  induction bytes with
  | nil => simpa using ByteArray.isValidUTF8_empty
  | cons byte rest ih =>
    have first := ascii_byte_valid_utf8 byte (ascii byte (by simp))
    have tail := ih (by intro value member; exact ascii value (by simp [member]))
    simpa [← List.toByteArray_append] using first.append tail

theorem check_has_token_positions {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∃ tokens positioned, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
      positioned.map (fun entry => entry.1) = tokens ∧ FunctionsSyntax tokens functions ∧
      ScopesProjection functions scopes := by
  have sound := check_sound accepted
  cases sound.2.2.1 with
  | admitted tokens lexical grammar earlier recordCalls allRecordCalls =>
    obtain ⟨positioned, positions, projection⟩ := lexed_has_positions lexical 0
    exact ⟨tokens, positioned, positions, projection, grammar, sound.2.2.2⟩

theorem check_has_valid_utf8 {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    bytes.toByteArray.IsValidUTF8 := by
  apply ascii_bytes_valid_utf8
  have admitted := (check_sound accepted).2.2.1
  cases admitted with
  | admitted tokens lexical grammar earlier recordCalls allRecordCalls =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

theorem check_sound_with_positions {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    moduleInputs = [] ∧ bytes.toByteArray.IsValidUTF8 ∧
      (functions.map (·.name)).Nodup ∧
      AdmittedSource (bytes.map UInt8.toNat) functions ∧ ScopesProjection functions scopes ∧
      ∃ tokens positioned, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
        positioned.map (fun entry => entry.1) = tokens ∧ FunctionsSyntax tokens functions := by
  have sound := check_sound accepted
  obtain ⟨tokens, positioned, positions, projection, grammar, _⟩ :=
    check_has_token_positions accepted
  exact ⟨sound.1, check_has_valid_utf8 accepted, sound.2.1, sound.2.2.1, sound.2.2.2,
    tokens, positioned, positions, projection, grammar⟩

theorem check_in_declarative_domain {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    DeclarativeDomain bytes moduleInputs functions scopes := by
  have wordGuard : supportedWords (bytes.map UInt8.toNat) = true := by
    unfold check at accepted
    simp only [Bool.and_eq_true] at accepted
    exact accepted.1.1.1.1.2
  have namesGuard : functions.all supportedFunctionNames = true := by
    unfold check at accepted
    simp only [Bool.and_eq_true] at accepted
    exact accepted.1.1.1.2
  have parsed : parseSourceCodes (bytes.map UInt8.toNat) = some functions := by
    unfold check at accepted
    simp only [Bool.and_eq_true] at accepted
    cases found : parseSourceCodes (bytes.map UInt8.toNat) with
    | none => simp [found] at accepted
    | some tree =>
      simp only [found, Bool.and_eq_true, beq_iff_eq] at accepted
      exact congrArg some accepted.2.1
  have sound := check_sound accepted
  have safeAdjacency := accepted_has_no_conflicting_adjacency accepted
  have safeDecimals := accepted_decimal_runs_in_range accepted
  have trees := parse_source_admitted parsed
  unfold parseSourceCodes at parsed
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
  obtain ⟨tokens, tokenized, ⟨found, functionsParsed, parsed⟩⟩ := parsed
  split at parsed
  · cases parsed
    unfold supportedWords at wordGuard
    rw [tokenized] at wordGuard
    cases trees with
    | admitted _ _ _ ordered recordOrdered recordAnywhere =>
      exact ⟨sound.1, safeAdjacency, safeDecimals, sound.2.1, namesGuard, sound.2.2.2,
        tokens, tokenizer_syntax _ tokenized, functions_syntax _ functionsParsed,
        ordered, recordOrdered, recordAnywhere, wordGuard⟩
  · cases parsed

/-- The source admission boundary composes with the existing universal lowerer theorem. The
artifact's entire function meaning is the meaning of the tree derived from these exact bytes. -/
theorem admitted_lowering_means_source {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))} {artifact : RawArtifact}
    (accepted : check bytes moduleInputs functions scopes = true)
    (lowered : lowersTo artifact functions = true) :
    moduleInputs = [] ∧ AdmittedSource (bytes.map UInt8.toNat) functions ∧
      ScopesProjection functions scopes ∧ functionPrograms artifact = sourcePrograms functions := by
  have admission := check_sound accepted
  exact ⟨admission.1, admission.2.2.1, admission.2.2.2, lowers_to_means_the_source lowered⟩

/-- A certified source/IR pair starts at the original code bytes: its grammar derivation is
independent of the compiler's tree, and its IR meaning follows from the universal lowerer proof.
Source-document decoding and extraction from a Rust parser remain separate boundaries. -/
theorem certified_codes_admitted {sourceId : Xsh.Conformance.Transport.BridgeId} {codes : List Nat}
    {artifact : RawArtifact} {results : List ObservedResult}
    (certified : certifyCodes sourceId codes artifact = some results) :
    ∃ functions, AdmittedSource codes functions ∧
      functionPrograms artifact = sourcePrograms functions ∧
      artifact.sourceId = sourceId ∧ validateArtifact artifact = true ∧
      namedResults artifact = some results := by
  unfold certifyCodes at certified
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at certified
  rcases certified with ⟨functions, parsed, certified⟩
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, parse_source_admitted parsed, lowers_to_means_the_source checks.1.2,
      checks.1.1, checks.2, certified⟩
  · cases certified

theorem admitted_bytes_are_ascii {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∀ byte, byte ∈ bytes → byte.toNat < 128 := by
  have admission := (check_sound accepted).2.2.1
  cases admission with
  | admitted tokens lexical productions earlier recordCalls allRecordCalls =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

def arithmeticTree : List SourceFunction :=
  [⟨"f", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩]

def arithmeticBytes : List UInt8 := witnessBytes "pure f(x: Int) -> Int {\n  return x + 1\n}\n"

theorem arithmetic_bytes_admitted :
    check arithmeticBytes [] arithmeticTree [[("x", 0)]] = true := by
  decide +kernel

theorem assignment_bytes_admitted :
    check (witnessBytes "pure f() -> Int {\n  var x = 1\n  x = x + 2\n  return x\n}\n") []
      [⟨"f", [], [("x", 1)], none, .assigned "x" (.binary .add (.var "x") (.int 2))
        (.atom (.var "x"))⟩] [[("x", 1)]] = true := by
  decide +kernel

theorem computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let a = x + 1\n  let b = a * 2\n  return b\n}\n") []
      [⟨"f", ["x"], [], none,
        .computedLets [("a", .binary .add (.var "x") (.int 1)),
          ("b", .binary .mul (.var "a") (.int 2))] (.atom (.var "b"))⟩]
      [[("x", 0), ("a", 2), ("b", 3)]] = true := by
  decide +kernel

theorem boolean_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Bool {\n  return x > 1\n}\n") []
      [⟨"f", ["x"], [], none, .comparison .gt (.var "x") (.int 1)⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem result_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Result[Int, Int] {\n  return Err(x)\n}\n") []
      [⟨"f", ["x"], [], none, .constructed false (.atom (.var "x"))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem propagation_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Result[Int, Int] {\n  return Ok(x)\n}\n" ++
        "pure f(x: Int) -> Result[Int, Int] {\n  let y = g(x)?\n  return Ok(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .constructed true (.atom (.var "x"))⟩,
        ⟨"f", ["x"], [], some ⟨"y", "g", [.var "x"]⟩, .constructed true (.atom (.var "y"))⟩]
      [[("x", 0)], [("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem conditional_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return if x > 0 { x + 1 } else { 0 }\n}\n") []
      [⟨"f", ["x"], [], none, .conditional .gt (.var "x") (.int 0)
        (.binary .add (.var "x") (.int 1)) (.atom (.int 0))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem call_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Int {\n  return x + 1\n}\n" ++
        "pure f(y: Int) -> Int {\n  return g(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩,
        ⟨"f", ["y"], [], none, .call "g" [.var "y"]⟩] [[("x", 0)], [("y", 0)]] = true := by
  decide +kernel

theorem single_computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let y = x + 1\n  return y\n}\n") []
      [⟨"f", ["x"], [], none, .computedLet "y" (.binary .add (.var "x") (.int 1))
        (.atom (.var "y"))⟩] [[("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem admitted_byte_domain_nonempty :
    ∃ bytes functions scopes, bytes ≠ [] ∧ functions ≠ [] ∧ check bytes [] functions scopes = true := by
  exact ⟨arithmeticBytes, arithmeticTree, [[("x", 0)]], by decide +kernel, by decide +kernel,
    arithmetic_bytes_admitted⟩

theorem declarative_domain_nonempty :
    ∃ bytes functions scopes, bytes ≠ [] ∧ functions ≠ [] ∧
      DeclarativeDomain bytes [] functions scopes := by
  exact ⟨arithmeticBytes, arithmeticTree, [[("x", 0)]], by decide +kernel, by decide +kernel,
    check_in_declarative_domain arithmetic_bytes_admitted⟩

theorem substituted_source_and_projection_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n") []
      arithmeticTree [[("x", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("x", 1)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("y", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [] = false ∧
    check arithmeticBytes [arithmeticBytes] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

def substitutedSourceMutant (_bytes : List UInt8) (moduleInputs : List (List UInt8))
    (functions : List SourceFunction) (scopes : List (List (String × Nat))) : Bool :=
  check arithmeticBytes moduleInputs functions scopes

theorem substituted_source_mutant_exposed :
    let other := witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n"
    substitutedSourceMutant other [] arithmeticTree [[("x", 0)]] = true ∧
    check other [] arithmeticTree [[("x", 0)]] = false := by
  decide +kernel

theorem reserved_word_source_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return while\n}\n") []
      [⟨"f", ["x"], [], none, .value (.atom (.var "while"))⟩] [[("x", 0)]] = false := by
  decide +kernel

theorem reserved_literal_binding_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let if = 1\n  return x\n}\n") []
      [⟨"f", ["x"], [("if", 1)], none, .value (.atom (.var "x"))⟩]
      [[("x", 0), ("if", 2)]] = false := by
  decide +kernel

theorem adjacent_word_hyphen_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x-1\n}\n") []
      [⟨"f", ["x"], [], none, .value (.binary .sub (.var "x") (.int 1))⟩]
      [[("x", 0)]] = false := by
  decide +kernel

theorem adjacent_error_redirect_refused :
    check (witnessBytes "pure f(E: Int) -> Bool {\n  return E>1\n}\n") []
      [⟨"f", ["E"], [], none, .comparison .gt (.var "E") (.int 1)⟩]
      [[("E", 0)]] = false := by
  decide +kernel

theorem out_of_range_decimal_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return 9223372036854775808\n}\n") []
      [⟨"f", ["x"], [], none, .value (.atom (.int 9223372036854775808))⟩]
      [[("x", 0)]] = false := by
  decide +kernel

theorem unsupported_trailing_bytes_refused :
    check (arithmeticBytes ++ [0]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [64]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [195, 169]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ witnessBytes "pure") [] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end SourceAdmission
end Xsh.Conformance.Vertical
