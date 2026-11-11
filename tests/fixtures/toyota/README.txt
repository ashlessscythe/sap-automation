ANONYMISED: ids, plants, names, users and free-text codes in these fixtures were replaced by consistent
substitutions (order and shape preserved). Mentions of "real" data below refer to the shape of the original exports.

toyota fixtures (run date 2026-11-05)
Origin: resources/toyota_tsv.zip (exported 2026-11-10 by the owner): three REAL SAP text-with-tabs exports and the macro output
11-05-2026_TOYOTA.xlsx (sheets VT11_11-05-2026, TOYOTA_VL06O, pivot). The data is tiny: 3 shipments, 12 VL06O item rows = 3
deliveries, one plant (PL55), every delivery has a ZMDETPC row.

real/ (byte-exact, -text in .gitattributes: do not re-encode or normalise line endings)
  raw_vt11.txt      3 rows x 13 columns (variant VT11_TOYOTA_VARIANT, layout scorecard)
  raw_vl06o.txt    12 rows x 7 columns  (layout xdock; no preamble, starts with a blank line)
  raw_zmdetpc.txt   3 rows x 3 columns  (summarised)

Expected outputs converted from the xlsx (UTF-8, tab separated, LF; one-off converter kept out of the repo)
  expected_vt11.tsv    3 x 13  Excel dates written back as yyyy-mm-dd / hh:mm:ss
  expected_vl06o.tsv   3 x 15  as in the sheet, including the trailing columns M:O (delivery_, pallet, child again)
  expected_pivot.tsv   2 x 5   pivot sheet as rendered without its blank first column: B:C pivot (Row Labels / Sum of total,
                               Grand Total row) and E:F the macro trailer cells (E1/F1 header artefact, E3 Grand Total, F3 blank)
Differences to our output are asserted exactly in tests/toyota_golden.rs.
