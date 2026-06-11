; Top-level TOML keys (custom, ADR-0002).
; Two cases: root-level pair keys (before any [table]) and table / array-of-
; table headers (the names you would reference: [foo], [[bar]], [a.b]).

(document
  (pair
    . (_) @name) @definition.key)

(table
  . (_) @name) @definition.key

(table_array_element
  . (_) @name) @definition.key
