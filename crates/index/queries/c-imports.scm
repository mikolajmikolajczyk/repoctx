; Import sites for the import / dependency graph (epic #4 / ADR-0011).
; #include "foo.h" and #include <stdio.h> (quotes/brackets stripped downstream).

(preproc_include
  path: (string_literal) @module)

(preproc_include
  path: (system_lib_string) @module)
