mustgo fixtures (run date 2026-11-05)
Origin: resources/mustgo.zip, converted xlsx -> TSV (UTF-8, tab separated, LF, tabs/newlines in cells replaced by a space).
These are APPROXIMATE stand-ins: no real SAP text exports exist, so the raw TSVs only mimic the xlsx cell display.
Raw files keep their original (unsanitised) headers and identifiers as text exactly as in the xlsx (leading zeros kept,
e.g. 0372738060 in raw_zmdesnr_del). Numbers have no trailing .0; dates yyyy-mm-dd; times hh:mm:ss.

Raw per-step exports (data rows x columns)
  raw_vt11.tsv              91 x 18   (first column has a blank header; 54 rows TransportPlanningPt=TPDC)
  raw_vl06o.tsv            834 x 10   (107 distinct deliveries)
  raw_zmdesnr_del.tsv     5796 x 19   (blank header in column 14 = customer; SHP 5546, PAL 228, SPV 22)
  raw_lt23.tsv            1071 x 11   (87 distinct TOs)
  raw_zmdesnr_del_to.tsv   476 x 19
  (history export 20261109122844_zmdesnr.xlsx, 2108 rows, intentionally not converted)

Expected outputs from 11-05-2026_MUSTGO.xlsx (sheet -> file, rows x columns; headers already sanitised by the macro)
  MUSTGO_LT23  -> expected_lt23.tsv       15 x 13  (bdup, ..., datetime, ...)
  MUSTGO_SN_TO -> expected_sn_to.tsv     476 x 19
  SNR_COPY     -> expected_snr_copy.tsv   92 x 21  (week at position 19, export_date last)
  PIVOT        -> expected_pivot.tsv       9 x 4   (laid out as rendered: blank first row/column, "Count of ..." label rows)

Conversion notes / ambiguities
  - SNR_COPY column "time" is stored as an Excel fraction in the sheet (0.84376157407407); converted to hh:mm:ss (20:15:01).
  - Time cells elsewhere are real Excel times, rounded to the nearest second.
  - Date cells with midnight time are written as yyyy-mm-dd; the raw xlsx do not distinguish date-only from datetime formats.
  - PIVOT is a rendered pivot table, its cells are values; its header row is the blank first row.
  - raw_vt11 "Descrip.of Shipment" is text like "11/04/2026 1800" and is kept as is.
