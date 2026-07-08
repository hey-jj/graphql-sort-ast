# Changelog

## [0.2.0] - 2026-07-07

### Changed
- Variable default values now reject nested variables instead of accepting them as defaults. (#14)
- A fragment named `on` now returns a parse error. (#15)
- Block string dedent now treats only ASCII space and tab as indentation, so nonbreaking spaces stay in the parsed string. (#18)

### Fixed
- Byte order marks are now ignored anywhere GraphQL allows ignored input. (#16)
- Braced Unicode escapes in strings now decode to the intended character. (#17)

## [0.2.0] - 2026-07-07

### Changed
- Variable default values now reject nested variables instead of accepting them as defaults. (#14)
- A fragment named `on` now returns a parse error. (#15)
- Block string dedent now treats only ASCII space and tab as indentation, so nonbreaking spaces stay in the parsed string. (#18)

### Fixed
- Byte order marks are now ignored anywhere GraphQL allows ignored input. (#16)
- Braced Unicode escapes in strings now decode to the intended character. (#17)
