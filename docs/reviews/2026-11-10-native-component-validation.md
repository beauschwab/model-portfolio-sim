# Native component validation and exact platform proofs

The previous native product and ledger jobs build the decision runtime and run
engine parity, but do not run standalone core or decision crate tests. The new
`native-components.yml` gate runs all six native crates on Linux and Windows:
format, all-target tests, all-feature strict Clippy and documentation tests.
It uploads command/exit/log/source-hash receipts and preserves failures.
No API release artifact is installed or replaced by this job.

The first Linux run (42773560598, source 1ed04ea) exposed ten exact Card2 frozen
proof comparisons that had only been exercised on Windows. Original-EIR values
derived from platform `log1p`/`expm1` differ in their final bits. Production
formulas, output tolerances and the original Windows fixture are unchanged.

Run 42774011504 generated a separate proof from the existing independent Python
raw-source verifier, without native callbacks. Every one of the ten failing
native expanded results exactly equals that independent Linux proof. Across the
frozen fixture, 1,102 numeric leaves differ, by at most
2.7755575615628914e-17. No source, identity, date, ordering, shape or nonnumeric
field differs. The Linux reference proof SHA256 is
`51f3af69946b1601faf885d1eff46b31319e43a36f6a633b74a4fd1a4b41dd68`.
The original fixture SHA256 is
`4b35610e48e9d1a6a31f51e38670f5bfed00102d1abb8c715112f60116c871d7`.

The checked-in GNU/Linux x86_64 patch stores only these independently derived
numeric expectations. The test binds the original fixture digest and every old
leaf, prohibits duplicate or non-expected-field patches, and still compares the
complete expanded result exactly. Other platforms retain the original fixture.
The assertion reports the first differing leaf instead of dumping megabytes of
state. There is no epsilon, normalization, native-generated expected value or
financial implementation change.

`generate_card2_platform_proof.py` independently reconstructs all accepted cases,
requires all refused cases to remain refused, applies the existing independent
verification gate against the original proof, and emits full proof plus sparse
numeric differences. Windows reconstruction is exactly equal to all original
cases. These are synthetic implementation checks, not observed model accuracy.
The expanded run also found two historical mixed-journal frozen outputs with
the same platform dependency. Run 42777848624 independently reconstructed both
mixed results, including their nested original-EIR income receipts. Exactly 140
numeric leaves change in each result, and applying the reference patch matches
each complete native Linux output exactly. The patch SHA256 is
`dc5eedd3c775426820d92104c30f679106d697bf957471b31d24ce5a25416827`.
Original fixture digests are bound after canonicalizing Git CRLF/LF endings;
numeric values are never normalized. The existing card1 reference has 1,168
separate reference-versus-native rounding differences: these are frozen,
explicitly checked to be identical on both platforms, and excluded from the
platform patch. Any platform change in such a field rejects. Windows independent
reconstruction produces no patch for either mixed fixture.

CI regenerates both reference proofs and checks their numeric changes against
the checked-in Linux expectations before running the Rust tests. The Rust tests
also bind every original leaf and compare the complete final output exactly.

The cut-admission fixture contains 28 fewer serialized receipt bytes on Linux
(585,839 completed and 1,126,328 revised). Its exact work expectation now charges
the actual receipt bytes at the unchanged two preparation and six final scans
per byte. Structural work, retained-byte reserves, and the pre-publication
refusal remain unchanged. This does not authorize the remaining cut adoption.

Full final-source native execution remains pending until both hosted jobs pass.
