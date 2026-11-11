k2xx fixtures (run date 2026-11-05)
Origin: resources/k2xx_tsv.zip (VT11, VL06O, ZMDETPC raw exports + macro output 11-05-2026_K2XX.xlsx, exported
2026-11-11 by the owner) and resources/k2xx_zvt11.zip (raw ZVT11 Closed export of the same run). The former
synthetic stand-ins were retired; everything here is real.

real/ (byte-exact, -text in .gitattributes: do not re-encode or normalise line endings; ASCII, CRLF)
  raw_vt11.txt     53 rows x 13 columns  (VT11 variant TIER_VT11_K2XX, list layout scorecard; free text truncated to 14 chars)
  raw_vl06o.txt   840 rows x 8 columns   (layout pallets: Delivery, Ship-to, Name of the ship-to party, Deliv.date, GM, Plnt,
                                          External Delivery ID, No.Pk; 107 distinct deliveries)
  raw_zmdetpc.txt 106 rows x 3 columns   (summarised; zero padded; blank column between Pallet and Child; only 8012734818 missing)
  raw_zvt11.txt   409 rows x 8 titles    (ZVT11 Closed, variant k2xx_, layout add_del, window 2026-07-28..2027-02-13 =
                                          run date -/+ 100 days; titles ` Date`, blank, `Trailer Nu`, `Shipment`, `Delivery N`,
                                          `Plnt`, `Ship To/From Name`, `Shipment E`)

Expected outputs (UTF-8, tab separated, LF; Excel dates written back as yyyy-mm-dd / hh:mm:ss; one-off converter kept out of the repo)
  expected_real_zvt11.tsv  409 x 7   sheet K2XX_ZVT11 WITH SPEC 7.0 ITEM 30 APPLIED (owner decision 2026-11-11, deliberate deviation):
                                     date = shipment_end_date on every row (the macro sheet = raw export has 4 blank and 78 other dates)
  expected_real_vt11.tsv    53 x 13  sheet K2XX_VT11_11-05-2026 (free-text columns longer than the list shows; tests compare prefixes)
  expected_real_vl06o.tsv  107 x 13  sheet K2XX_VL06O (merged with ZMDETPC; delivery 8012734818 has blank delivery_/pallet/child/total)
  expected_real_pivot.tsv   12 x 2   plant / sum of total, COMPUTED from expected_real_vl06o.tsv (the macro workbook has no pivot;
                                     owner decision, spec 7.0 item 1), total 1148

Tests: k2xx_golden_real (loaders and sheets), k2xx_golden (stages), k2xx_runner (run_k2xx with a fake seam serving real/).
Known property of the real ZVT11 export: 4 rows (FW63 shipment 24625349) have a blank Date and 78 rows have Date 2026-11-06 while
shipment_end_date is 2026-11-05. The macro keeps this; k2xx overwrites date with shipment_end_date (item 30).
