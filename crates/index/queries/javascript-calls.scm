; Call sites for the static call graph (epic af42572 / ADR-0010).
; Shared by JavaScript, TypeScript, and TSX (same call/member node names).

(call_expression
  function: (identifier) @callee)

(call_expression
  function: (member_expression
    property: (property_identifier) @callee))
