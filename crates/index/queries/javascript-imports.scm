; Import sites for the import / dependency graph (epic #4 / ADR-0011).
; Shared by JavaScript, TypeScript, and TSX. Captures the ESM specifier
; string content as @module: `import x from "mod"`, `export … from "mod"`.

(import_statement
  source: (string (string_fragment) @module))

(export_statement
  source: (string (string_fragment) @module))
