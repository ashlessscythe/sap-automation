xdock fixtures (run date 2026-11-05)
Origin: resources/xdock_tsv.zip (exported 2026-11-10 by the owner): three REAL SAP text-with-tabs exports and the macro output
11-05-2026_XDOCK.xlsx (sheets XDOCK_VL06O, XDOCK_VT11_11-05-2026, pivot).

real/ (byte-exact, -text in .gitattributes: do not re-encode or normalise line endings)
  raw_vt11.txt     91 rows x 13 columns  (list layout scorecard; free text truncated to 14 characters)
  raw_vl06o.txt  1068 rows x 7 columns   (layout xdock; 148 distinct deliveries)
  raw_zmdetpc.txt 140 rows x 3 columns   (summarised; zero padded deliveries, blank column between Pallet and Child)

Expected outputs converted from the xlsx (UTF-8, tab separated, LF; one-off converter kept out of the repo)
  expected_vt11.tsv    91 x 13   Excel dates written back as yyyy-mm-dd / hh:mm:ss, date-only descripof_shipment as MM/DD/YYYY
  expected_vl06o.tsv  148 x 15   as in the sheet, INCLUDING the 3 stray trailing columns M:O (delivery_, pallet, child again)
                                 and total = 0 for the 8 deliveries without a ZMDETPC row (see docs/abc_headless_plan.md)
  expected_pivot.tsv   15 x 2    as rendered (13 groups) (header Row Labels / Sum of total, last row Grand Total 1328)
The known differences to our output are asserted exactly in tests/xdock_golden.rs.
