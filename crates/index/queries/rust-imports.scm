; Import sites for the import / dependency graph (epic #4 / ADR-0011).
; Captures the `use` path as @module (e.g. crate::foo::Bar, std::collections).
; extern crate names are captured too.

(use_declaration
  argument: (_) @module)

(extern_crate_declaration
  name: (identifier) @module)
