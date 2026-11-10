k2xx fixtures (run date 2026-11-05)

!! SYNTHETIC / DERIVED STAND-INS. NOT real k2xx exports and NOT the macro output. !!
No raw k2xx export (VT11 variant TIER_VT11_K2XX, VL06O layout pallets, ZVT11 Closed, ZMDETPC) and no K2XX macro xlsx
were available when the report was written. Everything here was built by a one-off script (kept out of the repo) from
the REAL exports of other reports of the same business day, plus the column lists and rules of the spec
(docs/abc_headless_migration_spec.md 3.2, 3.3, 5.5, 9.5). The tests pin the rules of spec 9.5; they do not prove
equality with the macro. Replace this directory with real exports (then under real/, byte-exact, -text) when the
owner supplies them.

synthetic/  (UTF-8, LF line ends; real SAP exports have CRLF. The loader accepts both)
  raw_vt11.txt     54 rows x 13 columns. DERIVED: the rows with TPPt = TPDC of ../xdock/real/raw_vt11.txt (list layout
                   scorecard), order kept, preamble rewritten (Entries: 54). The spec 9.5 run had 107 deliveries, the
                   same number these 54 TPDC shipments give, which is why they stand in for variant TIER_VT11_K2XX.
                   Whether that variant really selects them is NOT known.
  raw_vl06o.txt   834 rows x 8 columns, 107 distinct deliveries. DERIVED: the rows of ../xdock/real/raw_vl06o.txt
                   whose delivery occurs in ../mustgo/real/raw_vl06o.txt (the deliveries of the TPDC shipments), with
                   the columns re-ordered to the `pallets` layout of spec 3.3 (Delivery, Ship-to, Name of the ship-to
                   party, Deliv.date, GM, Plnt, External Delivery ID, No.Pk) and Deliv.date taken from the mustgo
                   export. The short titles are those of the real xdock/mustgo lists; the real `pallets` list may title
                   or order its columns differently. (Spec 9.5 reports 840 item rows for the real run.)
  raw_zmdetpc.txt 104 rows x 3 columns. DERIVED: the rows of ../xdock/real/raw_zmdetpc.txt for those deliveries,
                   preamble kept. 3 deliveries have no row: 85650310, 85655488, 8012734818 (spec 9.5: 106 rows, only
                   8012734818 missing).
  raw_zvt11.txt   108 rows x 7 columns. INVENTED: one row per distinct (delivery, plant) of raw_vl06o.txt in the
                   `add_del` layout of spec 3.2 / 8.2 ( Date, Trailer Number, Shipment Number, Delivery Number, Plant,
                   Ship To/From Name, Shipment End date; the header ` Date` has the leading blank of spec 9.5).
                   The shipment of each delivery is assigned ROUND-ROBIN over the 54 shipments (the real pairing is in
                   no fixture), trailer and end date come from that shipment's VT11 row, Date is the run date,
                   deliveries are zero padded. The format of a real ZVT11 text export (preamble, titles, widths) has
                   never been seen. (Spec 9.5 reports 409 rows for the real run.)

Expected outputs (UTF-8, tab separated, LF), computed by the same one-off script from the spec text, independently of
the Rust code:
  expected_vl06o.tsv  107 x 13  first row per delivery, ascending by delivery, date, delivery_, pallet, child, total
                                (total = pallet + child; blank delivery_/pallet/child/total without a ZMDETPC row);
                                sum of total = 1144
  expected_zvt11.tsv  108 x 7   the ZVT11 stand-in as loaded (written as exported)
  expected_pivot.tsv   12 x 2   pivot of the vl06o table: plant, total (sum of total per plant). Long form, plants
                                ascending, no grand-total row; sum = 1144. Computed by a separate one-off Python
                                script straight from synthetic/raw_vl06o.txt and raw_zmdetpc.txt (first row per
                                delivery, pallet + child of its ZMDETPC row, nothing for a delivery without one),
                                independently of the Rust code. The pivot is an owner decision of 2026-11-10 that
                                overrides spec 7.0 item 1 ("k2xx produces no pivot"); the macro workbook has no pivot
                                sheet, so there will never be a macro pivot to compare with.
                                The stand-in VL06O export already had the Plnt column (canonical `plant`), so the
                                synthetic exports were NOT changed for the pivot. No stand-in row has a blank plant:
                                the `(blank)` row is covered by hand-made rows in the tests (k2xx_golden
                                pivot_edge_cases, k2xx_runner pivot_puts_a_blank_plant_last).
The VT11 output is checked against the real xdock export in tests/k2xx_golden.rs.

Tests: tests/k2xx_golden.rs, tests/k2xx_runner.rs.
