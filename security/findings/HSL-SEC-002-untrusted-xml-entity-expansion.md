# HSL-SEC-002 — untrusted XML used the standard parser

Severity: Medium  
Status: Fixed and regression-tested  
Trust boundary: user/network document → Python parser

## Source to sink

User-supplied DOCX/XLSX parts and network-supplied RSS/arXiv documents reached `ElementTree.fromstring`. Standard XML parsing permits dangerous entity-expansion patterns that can consume excessive CPU or memory.

## Impact and reachability

DOCX/XLSX extraction is reachable from the core file tool. RSS and arXiv paths are user-invoked skills. A crafted document or response could deny service to the Hermes process. External-entity file disclosure is constrained by Python's parser behavior, but entity-expansion denial of service was sufficient to validate the finding.

## Fix

- Added `defusedxml==0.7.1` as a pinned core dependency.
- Replaced parsing imports in `tools/read_extract.py`, the RSS watcher, and the arXiv skill.
- Preserved the read-tool error contract by translating `DefusedXmlException` into `ExtractionError`.
- The WeCom crypto helper retains standard ElementTree only for XML construction; it does not parse input.

## Verification

Behavior tests feed DTD/entity payloads through DOCX, RSS, and arXiv paths. The read tool fails safely and network skills reject the payload. Focused tests report 84 passes; Ruff is clean.
