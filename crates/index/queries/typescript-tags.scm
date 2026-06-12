; TypeScript / TSX symbol-extraction tags.
;
; Vendored from https://github.com/Aider-AI/aider
;   `aider/queries/tree-sitter-languages/typescript-tags.scm`
; License: Apache-2.0. Attribution: see crates/index/queries/NOTICE.
;
; Plus arrow-function patterns ported from Aider's javascript-tags.scm
; (same license, same source). TypeScript grammar reuses ECMAScript
; node names for `lexical_declaration` / `variable_declaration` /
; `assignment_expression` / `pair`, so the patterns apply as-is.
;
; This file overrides `tree_sitter_typescript::TAGS_QUERY` for both
; the TypeScript and TSX language entries.

;; -- declarations (signatures + concrete) ---------------------------

(function_signature
  name: (identifier) @name.definition.function) @definition.function

(method_signature
  name: (property_identifier) @name.definition.method) @definition.method

(abstract_method_signature
  name: (property_identifier) @name.definition.method) @definition.method

(function_declaration
  name: (identifier) @name.definition.function) @definition.function

(method_definition
  name: (property_identifier) @name.definition.method) @definition.method

;; -- arrow + function-expression bound to identifiers --------------

(lexical_declaration
  (variable_declarator
    name: (identifier) @name.definition.function
    value: [(arrow_function) (function_expression)]) @definition.function)

(variable_declaration
  (variable_declarator
    name: (identifier) @name.definition.function
    value: [(arrow_function) (function_expression)]) @definition.function)

(assignment_expression
  left: [
    (identifier) @name.definition.function
    (member_expression
      property: (property_identifier) @name.definition.function)
  ]
  right: [(arrow_function) (function_expression)]) @definition.function

(pair
  key: (property_identifier) @name.definition.function
  value: [(arrow_function) (function_expression)]) @definition.function

;; -- classes + interfaces ------------------------------------------

(class_declaration
  name: (type_identifier) @name.definition.class) @definition.class

(abstract_class_declaration
  name: (type_identifier) @name.definition.class) @definition.class

(interface_declaration
  name: (type_identifier) @name.definition.interface) @definition.interface

;; -- type aliases + enums + modules --------------------------------

(type_alias_declaration
  name: (type_identifier) @name.definition.type) @definition.type

(enum_declaration
  name: (identifier) @name.definition.enum) @definition.enum

(module
  name: (identifier) @name.definition.module) @definition.module

;; -- references (kept from upstream; not surfaced by repoctx yet) --

(type_annotation
  (type_identifier) @name.reference.type) @reference.type

(new_expression
  constructor: (identifier) @name.reference.class) @reference.class
