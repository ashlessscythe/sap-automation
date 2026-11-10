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

====================================================================================================
real/  REAL SAP "text with tabs" exports (resources/mustgo_tsv.zip, exported 2026-11-10 by the owner)
====================================================================================================
Bytes are unmodified (.gitattributes: `tests/fixtures/mustgo/real/** -text`; checked: git hash-object with and
without filters gives the same blob). Same business data as the xlsx run of 2026-11-05.

  real/raw_vt11.txt            from 20261110101550-vt11.txt        91 data rows, 22 tab fields (17 after loading)
  real/raw_vl06o.txt           from 20261110101726-vl06o.txt      834 data rows, 11 fields (10)
  real/raw_zmdesnr_del.txt     from 20261110101837-zmdesnr_1.txt 5796 data rows, 20 fields (19)  = ZMDESNR by deliveries
  real/raw_lt23.txt            from 20261110102002-lt23.txt      1071 data rows, 13 fields (11)
  real/raw_zmdesnr_del_to.txt  from 20261110102450-zmdesnr_2a.txt 476 data rows, 20 fields (19)  = by deliveries + TOs
  (20261110102547-zmdesnr_2b.txt, 2108 rows, header "Created By", statuses TSP/ASH...: the serial HISTORY call
   that mustgo discards; omitted on purpose. 2a is the deliveries+TOs query: 476 rows, same 15 TOs.)

Observed format (all five files)
  - Encoding: pure ASCII (valid UTF-8), no BOM, no NUL. Line ends CRLF everywhere, final line terminated.
  - Layout: SAP classic list download. Preamble, header line, ONE blank line, data rows. No summary/separator/total rows
    at the end, no page breaks.
      vt11   : line 1 "<TAB>List level:<TAB><TAB>1<TAB><TAB>Entries:...91...View:<TAB><TAB>1", blank, header, blank, data
      vl06o  : blank line, header, blank, data
      zmdesnr: 5 text lines (Serial Number History / User name / Run date / Time / "No. of records: n"), blank, header, blank, data
      lt23   : "Warehouse No.<TAB><TAB><TAB>E01<TAB>El Paso Dist. Center", 2 blank lines, header, blank, data
  - Every line starts with a tab (list border), so field 0 has a blank header and no data in every file (vt11 has
    two such leading fields). Other blank-header columns with no data (vt11: 3 more, lt23: 2, spacers) are dropped by
    the loader; the one blank-header column with data (zmdesnr: customer name after Ship-to) becomes `customer`.
  - Header and data lines have the same number of fields, except lt23 where the header has a trailing `CS` field and
    data rows end after `Source target qty` (13 fields vs 14: short rows are padded; `CS` = confirmation status is blank).
  - Numbers are right aligned, space padded, `.` decimal with 3 decimals and `,` thousands: `         600.000`,
    `        34,800.000`, `       122,460.768`, `    10` (item), `    6` (packages). No negative numbers, no trailing
    minus signs occur in these files (not handled).
  - Dates are already ISO `2026-11-05` (SAP user date format yyyy-mm-dd), times `hh:mm:ss` (`10:44:27`). There are no
    combined date-time cells, except the VT11 free text column `Description` with `11/04/2026 1800` / `11/05/2026`
    (mm/dd/yyyy hh24mm, kept as raw text per decision 26).
  - Leading zeros are kept as text: LM Serial/Parent `0372738060`, SLoc `0004`, TO Number `0209193416` (lt23; zmdesnr
    has `209193416` without the zero), VT11 route `000000`, SC `01`. The loader strips them (decision 25): route
    `000000` becomes `0`, as in the xlsx fixtures.
  - Free text is truncated to the list column width (14 characters) in the real VT11 export: Seal Number 2 / Description
    `CTII GMCCA 700` (xlsx: `CTII GMCCA 700141`), `11/05/2026 080` (xlsx: `11/05/2026 0800 Bank`). Not used by mustgo.
  - VT11 quirk (same in the xlsx): the values under `Seal Number 2` / `Description` / `S` are shifted by one against
    the header (Seal Number 2 holds the shipment description, `Description` holds the date text).
  - "No. of records" in the zmdesnr preamble does NOT equal the data rows: 5,839 (file 1; 5796 data rows), 476 (2a; 476),
    6,994 (2b; 2108). vt11 "Entries: 91" and lt23/vl06o row counts do match the spec. Row counts per spec 9.3 are the
    data rows (5796 and 476).

Differences vs the xlsx-derived fixtures
  - Column NAMES are the short list-screen titles, not the xlsx titles: vt11 Shipment/ShTy/TPPt/ServcAgent, zmdesnr
    LM Serial/Parent no/Plnt/Cust. Part/WhN/SLoc/Typ/Bin/Ship-to/TO Number/Created, lt23 TO Number/Created On/Time/Plnt/
    Typ/Source Bin/CS, vl06o Ship-to/No.Pk/Deliv.date/SU/WUn. The loader keeps them (sanitised) and
    `abc::raw::*_ALIASES` map them to the canonical (xlsx / macro) names; the mapping is tested.
  - VT11 layout differs: 17 columns after loading (xlsx: 18 incl. a blank first column); real has C,S,ST,PL,SL,L,SC,S
    where the xlsx has Shpmt compl. type, Service Level, ... Only shipment_number and transportplanningpt are used.
    Header `S` occurs twice in VT11.
  - After alias + loading, vl06o, both zmdesnr files and lt23 equal the xlsx-derived tables cell for cell (tested).
