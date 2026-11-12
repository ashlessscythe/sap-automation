ANONYMISED: ids, plants, names, users and free-text codes in these fixtures were replaced by consistent
substitutions (row order and shape preserved). Mentions of "real" data below refer to the shape of the original exports.

repack fixtures (run date 2026-11-05)

REAL DATA (added 2026-11-11): real/ holds the owner's real exports of the live run (byte-exact, -text; do not re-encode):
  raw_zmdesnr_del.txt   ZMDESNR by deliveries excluding parents, 5839 rows, CRLF, ASCII (short list titles: LM Serial, Parent no,
                        Cust. Part, Plnt, SLoc, WhN, Created, Created On, Net wt, Container; a blank title = customer). Four rows
                        have no plant/customer/status (blank-plant rows) and a short field count (no container column).
  raw_zmdesnr_hist.txt  ZMDESNR by serial / History for the 36 serials of block R2, 380 rows (the header says `Created By`, the
                        by-deliveries list says `Created`: both are covered by raw::ZMDESNR_ALIASES)
  alias_customers.txt   the anonymised customer names that the alias rule counts as alias-plant customers (the names that started
                        with the three alias prefixes of the macro); the real-data tests pass them as exact patterns
expected_real_snr_repack.tsv (280 x 16) and expected_real_pivot.tsv (customer x plant, count of lm_serial_number, Grand Total
row and column) come from the macro xlsx of the run (11-05-2026_REPACK.xlsx, sheets SNR_REPACK and pivot), UTF-8, tab separated,
LF, with the same anonymisation applied. Excel date serials in the xlsx are written as ISO dates; the net weight as the loader
prints it (`29.45`, `0`). The rows are in the macro order R1, history rows, R3, R4 (see spec 9.5). Tests: tests/repack_real.rs.

How the anonymisation was done (one-off script, kept out of the repo): serials and parents map to 9 digit numbers (order
preserving; width kept, zero padded to 10 in the exports like SAP prints them); materials, deliveries and customer parts to numbers or
alphanumerics of the same length / character classes; containers keep their letters (so KC* and SC* stay KC* / SC*) and get new
digits; customers become `CUSTOMER nnn (ccccc)` in the order of the original names (so the pivot keeps its sort order) with the
parenthesised code only where the original had one; plants `Fxnn` become `PLnn` (the label plant is PL55, as in the relabel
fixtures); the warehouse number becomes WH01; users become GNAAAAAnnn / UUUUUU. Dates, statuses, pallet flags, storage locations
and net weights are unchanged. The expected files went through the same mapping.

Stand-ins (NOT real exports):
  k2xx_vl06o_stand_in.tsv       the k2xx `vl06o` TSV of the run: the 104 distinct deliveries of the real by-deliveries export (anonymised),
                                columns delivery, date, delivery_, pallet, child, total. The runner test copies it to the path of
                                the k2xx block's vl06o output; the fake SAP seam ignores the delivery list itself
  relabel_combined_stand_in.tsv the relabel `combined` TSV of the run: 3 ordinary rows and the 5 yellow (history) rows of the relabel
                                real fixture (tests/fixtures/relabel/expected_real_combined.tsv, serials 130007098..130007102).
                                The macro exclusion list = the serials of the yellow rows; no row of the repack export has one of them
                                as parent (spec 9.5), so the real run is the same whichever 5 serials are listed. The relabel fixture
                                and this one use a different anonymisation of the same run, so the serials are NOT the ones of the
                                real repack export (they cannot be linked, by design)

synthetic/  (UTF-8, LF line ends, short list titles like the real list; the loader accepts both). Built by a one-off script that
implements spec 9.5 independently of the Rust code (the expected files come from that script):
  raw_zmdesnr_del.txt   21 rows (+ one blank serial row that is dropped): every rule of spec 9.5 has rows
                        - net weight 49.999 (R1), 50.000 (neither R1 nor R2: the macro filters are `< 50` and `> 50`), 50.001 (R2),
                          blank net weight (neither), parent serial set (neither)
                        - KC0001 / sc0002 / KC0015 containers (deleted from R1/history/R3; the match is case-insensitive)
                        - label plant PL55: plain customer without pallet and with a container (R4), blank container (R4),
                          pallet 1 / 2 (nothing), container KC (not R4), alias customers ALIAS_A / ALIAS_B (alias plant for the
                          rule: R1 and R2 instead of R4, but the plant prints as PL55), AGM* (NOT an alias: R4)
                        - blank plant rows, light and heavy (nothing)
                        - PAL rows: R2 + R3, R3 only, R1 + R3 (a row can be in two blocks; both copies are kept)
  raw_zmdesnr_hist.txt  7 rows for the serials 900000003, 900000013, 900000020: UNP/WH01 rows (kept), PRI/WH01 and UNP/WH02 (not),
                        a KC container history row (kept by the history filter, then deleted)
  expected_snr_repack.tsv 13 x 16 and expected_pivot.tsv 6 x 5 (Grand Total 13)
  combined_one_yellow.tsv, combined_no_yellow.tsv  relabel `combined` stand-ins with 1 and with 0 yellow rows (the exclusion list is
                        skipped below 2); combined_no_yellow_column.tsv has no `yellow` column (an output of an older relabel)

Differences to the macro (documented, owner decisions 33-35): no temporary alias-plant marks and no extra last row in repack_sn; the macro's
Multi_Sort calls do not change the order and are not reproduced; yellow is the explicit `yellow` column of relabel's combined.

Tests: tests/repack_real.rs (golden on the real exports), tests/repack_golden.rs (synthetic edge cases), tests/repack_runner.rs.
