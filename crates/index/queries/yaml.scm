; Top-level keys of every YAML document in a stream (custom, ADR-0002).
; Each document's block_node may carry a block_mapping; flow root or non-
; mapping documents produce no symbols (per gain contract / ADR-0002).
(document
  (block_node
    (block_mapping
      (block_mapping_pair
        key: (_) @name) @definition.key)))
