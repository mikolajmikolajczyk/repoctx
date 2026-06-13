; Deep variant (opt-in `index.nested_keys`): all pair keys at any depth +
; table / array-of-table headers.
(pair
  . (_) @name) @definition.key
(table
  . (_) @name) @definition.key
(table_array_element
  . (_) @name) @definition.key
