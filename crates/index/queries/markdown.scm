; Markdown headings (custom, ADR-0002). ATX h1..h6 and setext h1/h2.
; Names are the entire heading node's source; the extractor trims marker
; characters (#, =, -) and surrounding whitespace.

(atx_heading) @definition.section
(setext_heading) @definition.section
