; Top-level keys of a JSON document (custom, ADR-0002).
; Captures only pairs whose object parent is the direct child of `document`.
(document
  (object
    (pair
      key: (string (string_content) @name)) @definition.key))
