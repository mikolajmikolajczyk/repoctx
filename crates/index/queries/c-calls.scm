; Call sites for the static call graph (epic af42572 / ADR-0010).

(call_expression
  function: (identifier) @callee)

(call_expression
  function: (field_expression
    field: (field_identifier) @callee))
