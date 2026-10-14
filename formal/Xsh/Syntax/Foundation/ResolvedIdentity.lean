import Xsh.Syntax.Core

/-!
Identity types for a resolved name occurrence. Raw names belong to the syntax
and name table; declaration identities identify lexical binders; occurrence
identities distinguish separate uses even when their syntax is equal. A
binding identity combines one declaration with one activation.
-/

namespace Xsh.Syntax.Foundation

open Xsh.Foundation

/-- A declaration identity reuses the base machine's distinct binder domain. -/
structure DeclarationId where
  binder : BinderId
  deriving DecidableEq

/-- A source-site label that can distinguish equal name syntax at separate sites. -/
structure OccurrenceId where
  word : Word32
  deriving DecidableEq

/-- A dynamic binding is one declaration instantiated in one activation. -/
structure BindingId where
  activation : Nat
  declaration : DeclarationId
  deriving DecidableEq

/-- A name use carries its source-site label; lookup uses its name. -/
structure NameOccurrence where
  occurrence : OccurrenceId
  name : RawName
  deriving DecidableEq

end Xsh.Syntax.Foundation
