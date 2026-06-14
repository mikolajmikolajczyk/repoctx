; Call sites for the static call graph (epic af42572 / ADR-0010).
; Captures the callee name node as @callee. Receiver type is not resolved
; (name-based): a.foo() and b.foo() both capture `foo`.

(call_expression
  function: (identifier) @callee)

(call_expression
  function: (scoped_identifier
    name: (identifier) @callee))

(call_expression
  function: (field_expression
    field: (field_identifier) @callee))

(macro_invocation
  macro: (identifier) @callee)
