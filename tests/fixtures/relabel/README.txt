ANONYMISED: ids, plants, names, users and free-text codes in these fixtures were replaced by consistent
substitutions (order and shape preserved). Mentions of "real" data below refer to the shape of the original exports.

REAL DATA (added 2026-11-11): real/ holds the owner's real exports of the live run (byte-exact, -text; do not re-encode):
  raw_zmdesnr_del.txt        ZMDESNR by deliveries, 6939 rows (short list titles: Sr No Verf, Scan Cpy, Doc. Date, ...)
  raw_zmdesnr_hist.txt       ZMDESNR by serial / History, 5 rows
  raw_zmdesnr_del_rerun.txt  a second export of the same run (the one that triggered the missing-column error)
expected_real_combined.tsv / expected_real_pivot.tsv come from the macro xlsx of the run (11-05-2026_RELABEL.xlsx): yellow =
the 5 rows with a yellow fill; blank Scan Cpy shown as 0 by Excel is blank here. Test: tests/relabel_real.rs. The synthetic
files below remain for edge cases the real run does not cover (B7 rows, 4-label variants).

relabel fixtures (run date 2026-11-05)

!! SYNTHETIC STAND-INS. NOT real relabel exports and NOT the macro output. !!
No raw ZMDESNR export of a relabel run (variant ZMDESNR_RELABEL_VARIANT, layout ZMDESNR_RELABEL_LAYOUT; by deliveries and by serial /
History) and no RELABEL macro xlsx were available when the report was written. Everything here was built by a one-off
script (kept out of the repo) from the column list of the ZMDESNR_RELABEL_LAYOUT layout (docs/private/abc_headless_migration_spec.md
section 8.2 / 9.2: 20 columns, the 20th with a blank title = customer) and the rules of spec 5.6 / 9.5. The tests pin the
rules of spec 9.5; they do not prove equality with the macro. Replace this directory with real exports (then under
real/, byte-exact, -text) when the owner supplies them.

The delivery list is a private copy of the former k2xx stand-in (k2xx_vl06o_stand_in.tsv, 107 deliveries): the runner test copies it to
the path of the k2xx block's vl06o output of the same run. The deliveries inside the stand-in ZMDESNR rows are taken from
that list round-robin and have no meaning for the tests (the fake SAP seam ignores the input lists).

synthetic/  (UTF-8, LF line ends; real SAP exports have CRLF. The loader accepts both)
  raw_zmdesnr_del.txt   44 rows x 20 columns, preamble like the real ZMDESNR lists ("Serial Number History", run
                        header, "No. of records"), then a header line with the CANONICAL layout titles (the real list
                        may use short titles, e.g. "Parent no", "Plnt": unknown for ZMDESNR_RELABEL_LAYOUT; canonical names
                        pass through raw.rs unchanged) and a blank line. Row order is shuffled on purpose.
                        Every rule of spec 9.5 has rows (the script tagged them; the tags are not in the file):
                        - plants PL33/PL50/PL58 (not PL55): spv_scan_copies 2 (also written `     2.000`), blank
                          verification, document_date 11-05 / 11-06 (window end) / 11-04 / 11-07 / blank, blank plant,
                          blank customer, blank serial (dropped), PAL rows with a shared parent (B3 keeps the first)
                        - PL55 with customers GM*, ALIAS_B*, ALIAS_C* and a lower-case `gmc trucks` (these become AL55
                          during the stage: they fall into B1/B2 instead of B4/B5/B6) and AGM* (NOT a match: stays PL55)
                        - PL55 pallet 1 (history candidates), pallet 2, pallet 3, pallet blank with and without a
                          parent, container PC100 / kc9 (no 4 labels) / TK77 / C0013043, kanban numbers K55, K77, 88,
                          K1 and `0` (B7: kanban starting 0 is excluded)
                        Serials and deliveries are zero padded like SAP prints them (the loader strips the zeros).
  raw_zmdesnr_hist.txt  7 rows x 20 columns, the History call for the 9 PL55 pallet-1 serials of the file above: three
                        PRI rows at storage location 0011 / 11 (yellow; one of them also has an older RCV row at 0003),
                        one SHP at 0011 and one PRI at 0003 (both do NOT mark), one PRI/0011 for a row that has a kanban
                        number (yellow, but never 4 labels anyway); two serials are not returned at all.

Expected outputs (UTF-8, tab separated, LF), computed by the one-off script from the spec text. The script parses the
raw text with its own parser and implements spec 9.5 from scratch, independently of the Rust code:
  expected_sn_del.tsv    44 x 20  the delivery export as loaded (no AL55 marks, no extra row)
  expected_combined.tsv  33 x 23  B1, B2, B3, B5, B6, B7 in F order (4 + 7 + 2 + 14 + 3 + 3 rows; B4 = 9 rows is not
                                  appended), then _of_labels (8 rows with 4, 25 with 2, sum 82), export_date
                                  (2026-11-05) and yellow (`1` for the 4 history hits)
  expected_pivot.tsv     9 x 6    cross-tab like the macro pivot: customer rows x plant columns + Grand Total column and
                                  row (ascending, (blank) last; sum 82); GM-type customers show their real plant PL55

Tests: tests/relabel_golden.rs, tests/relabel_runner.rs.
