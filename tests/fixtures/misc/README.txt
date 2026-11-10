misc fixtures (run date 2026-11-05)
Origin: resources/misc_tsv.zip (exported 2026-11-10 by the owner): two REAL SAP text-with-tabs exports and the macro output
11-05-2026_MISC.xlsx (sheets MISC_VT11_11-05-2026, MISC_VL06O, pivot). Tiny: 3 shipments, 6 VL06O item rows = 3 deliveries,
shipping points FV55 / FV59 (no EPDC row, so the regex step is not exercised by real data). No ZMDETPC for misc.

real/ (byte-exact, -text in .gitattributes: do not re-encode or normalise line endings)
  raw_vt11.txt     3 rows x 13 columns (variant MISC_EPDC_, layout scorecard)
  raw_vl06o.txt    6 rows x 7 columns  (layout inv; `ShPt` = shipping point; no preamble; first row has no No.Pk cell)

Expected outputs converted from the xlsx (UTF-8, tab separated, LF; one-off converter kept out of the repo)
  expected_vt11.tsv    3 x 13  Excel dates written back as yyyy-mm-dd / hh:mm:ss / MM/DD/YYYY
  expected_vl06o.tsv   3 x 8   as in the sheet (number_of_packages 0 where the text export is blank)
  expected_pivot.tsv   3 x 2   pivot as rendered without its blank first column (Row Labels / Sum of number_of_packages,
                               last row Grand Total 2)
Differences to our output are asserted exactly in tests/misc_golden.rs.
