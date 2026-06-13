; Minimal tags query for Bash — tree-sitter-bash ships no TAGS_QUERY.
; Captures function definitions (the only named, navigable construct).
; Variables / aliases are intentionally not surfaced (partial coverage).
(function_definition
  name: (word) @name) @definition.function
