; Minimal tags query for Kotlin — tree-sitter-kotlin-ng ships no TAGS_QUERY.
; Captures the named, navigable declarations (class / object / function).
(class_declaration
  name: (_) @name) @definition.class

(object_declaration
  name: (_) @name) @definition.class

(function_declaration
  name: (_) @name) @definition.function
