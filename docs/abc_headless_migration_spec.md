# abc macro → headless Rust runner: behaviour specification

Source of truth: the VBA in this folder plus the `ACTIVE`, `FILTERS`, `LAYOUTS` and `LOOKUPS` sheets of
`xlsx/260803-abc_reports_v21.6.xlsm`. SAP automation reference: `C:\temp\repo\sap-automation` (Rust).

**Purpose.** Describe what the macro does, per report, in a way that a human or an LLM can re-implement as a headless
executable that writes **TSV files instead of xlsx**. SAP work stays almost the same. Excel work (filter, copy,
formatting, pivot, formulas) is described as **method-agnostic table operations**.

**How to read.** Section 1 explains the model. Section 2 lists the reusable SAP procedures. Section 3 lists the SAP
tcodes with every control ID the VBA uses. Section 4 is the table-operation glossary. Section 5 has one specification
per report (`rName`). Section 6 is the Rust coverage matrix. Section 7 lists problems and open questions found while
reading the code. **Read section 7 before implementing.**

Notation: `A!n` = `ACTIVE` sheet row *n*. `T[x]` = a logical table (a former worksheet). Control IDs are shown
exactly as the VBA uses them (`wnd[0]/usr/...`).

---

## 1. Model

### 1.1 Run configuration (`ACTIVE` rows 3-6)

| Cell | Meaning | Current value |
|---|---|---|
| `E4` `rDefaultDate` | run date: a plain date **or** a JSON date object | `{"type":"range","values":{"week":"31","year":"2026"}}` |
| `J3` | popups on/off | `False` |
| `J6` `rDateFormat` | SAP-side date format used when typing dates into SAP fields | `yyyy-mm-dd` |
| `M3` | SAP user id | `vjvdmj` |
| `M4` | `SavePath` (not read by code; export folder is `%UserProfile%\Documents\SAP\SAP GUI`) | |
| `M5` | SAP login flag | `Ok` |
| `M6` | SAP connection name | `01. PN1 - ERP Production` |

**Dates (`get_json_date`).** Every step that has `"date":{"type":"default"}` resolves to `E4`.

| `E4` / date object | `date1` | `date2` | Notes |
|---|---|---|---|
| real date | that date | none | single-date mode |
| `{"type":"range","values":{"week":W,"year":Y}}` | Sunday of week W (`WeekStart(W,Y)-1`) | `date1 + 6` (Saturday) | week 31/2026 = **2026-07-26 → 2026-08-01** (matches sheet `VT11_07-26-2026`) |
| `{"type":"range","values":{"start_date":..,"end_date":..}}` | start | end | inclusive |
| `{"type":"diff","value":"-N"}` | default date − N | default date | `value:"+N"`: default → default + N; `use_date` overrides base |

Orchestration keys that can also live in `E4`: `runs` (array of `{checkmark|layout, iterations}`), `iterations`
(`single`|`loop`), `checkmarks` (name of a saved `x` layout), `loop`. With `loop`, `E4` is overwritten with each day in
turn and restored afterwards. `Call_Delete_Exports` runs after every pass.
The headless runner needs an equivalent: a list of report names, a date or range, and an optional per-day loop.

### 1.2 Step model (`ACTIVE` rows 7+)

Columns: A `step`, B `report_name` (rName), C `tcode`, D `variant`, E `arg`, F `export_action`, H notes (ignored).

* A blank = skipped. `x` = run on SAP session 0. A number = SAP session N. `override`/`ovr`/`only` = run only that row.
  **As checked in, only `A!135` (`add_del` / zvt11) and `A!136` (`add_del` / cleanup) have `x`.**
* D is JSON if it contains `{` and `}`, else a plain string. E is JSON if it contains `{`, else plain.
  The JSON parser is lenient: commas between members are optional (`A!17` omits them), single-quoted strings are
  accepted (`A!21`, `A!32`, `A!140`), and keys like `"formula"` hold Excel formulas.
* **F is free text matched with `Contains(...)`.** Comments in F can accidentally trigger behaviour (for example the
  word `filter`, `save`, `visible`, `pivot`, `subdir`). Treat F as a list of flags, not a comment field.
* Rows of one `rName` form a block. A row whose rName is `del_exp` ends the block and **deletes every table except the
  four config sheets** (`ACTIVE`, `FILTERS`, `LAYOUTS`, `LOOKUPS`).
* Error propagation: a step that fails sets an error. Remaining rows with the same rName are skipped. When the next
  row's rName is blank, a popup is shown and the error resets.
* A step with `use <name> <column>` as its variant, such as `use vt11 shipment_number`, means "take column
  `<column>` from the most recent table whose name **contains** `<name>`" (case-insensitive substring) and feed it
  to SAP as a multi-select list. Hidden (filtered-out) rows are **not** included, and the header cell **is**
  included in the pasted list (section 7, item 4).

### 1.3 Tables

Every SAP export becomes a table. On import the macro always does the following:

1. Convert text that looks numeric to numbers (`ConValue`).
2. Clear any autofilter.
3. **Sanitise headers**: keep only `A-Za-z0-9`, space and `_`. Replace space with `_`. Lowercase. Examples:
   `Name of the ship-to party` → `name_of_the_shipto_party`, `Serial #` → `serial_`,
   `Deliv. date(From/to)` → `deliv_datefromto`, `Ship-to party` → `shipto_party`.
4. Format date/time columns.
5. If a header cell is blank, fill it with `customer` (`DEFAULT_BLANK_FILL_HEADER`).

A table with fewer than 2 columns after import counts as empty. A step that needs rows from an empty table aborts the
block (`no_data`, `empty_sheet`).

Logical table names used below are the substrings that later steps match on (`vt11`, `vl06o`, `sn_del`, ...).
A sheet's real name carries a prefix or suffix (`VT11_07-26-2026`, `MUSTGO_SN_DEL`). Names are normally upper-cased and
the `sheet_out:<name>` flag renames the export.

### 1.4 Output files (current → target)

| | Current macro | Target |
|---|---|---|
| Format | `.xlsx` workbook, several sheets; `.txt` for Y_DN3 / VT22 / ZMDETPC detail | one `.tsv` per table |
| Folder | `<workbook dir>\reports\<override>\<today yyyy-mm-dd>\` and, with option `subdir`, `...\<rName>\` | same layout |
| `<override>` | `LOOKUPS!T:U` `rname_overrides`: see table below; unmatched rNames use their own name | same |
| File name | `UPPER(<date>_<name>).xlsx`. `<date>` = `MM-DD-YYYY` for single dates, `WK_<n>` for week ranges, or `date_format` if the save step sets it. `<name>` = `name` arg or rName. A numeric suffix is added if the file exists. | `<date>_<name>_<table>.tsv` (proposal, section 7 item 11) |
| Pivot | pivot table sheet | group-by/aggregate table (section 4.14) |

`rname_overrides`: pallets, dispatches, 999, util, pend, arrived, unload → `scorecard`. rma, redtag, busbar, add_del,
nc_tsp, tesla, nc_masterpack, inb_tsp → `ABC`. mustgo, k2xx, relabel, repack → `K2XX`. xdock, toyota, misc → `INV`.
`out_shp`, `out_rel`, `out_pal`, `inb_rcv` are not listed, so each uses its own name. Matching is substring-based
(`Contains(override_name, rName)`).

### 1.5 Retained behaviours worth keeping

* **Retry on failed paste**: up to 3 attempts (2 in some routines). Each retry re-opens the tcode. The last attempt
  uses a second copy method (`CopyColumn`).
* **Layouts are created on demand**: if a named ALV layout is missing in SAP, the macro builds it from the column list
  in `LAYOUTS` (`<layout>_<tcode>_layoutrows`) and saves it. Rust already has `setup_layout*` for this.
* Empty-result popups (`No shipments were found...`, `No data exists in the database for this selection`) end the step
  with an error, not a crash.

---

## 2. Reusable SAP procedures

IDs come from `Control_id_.bas` and `Ops_.bas`. Rust equivalents are in section 6.

**P1 start tcode** (`assert_tcode`). `StartTransaction(tcode)`. Read `wnd[0]/sbar` text. The tcode is unavailable if
the status bar contains `exist` or `autho`.

**P2 choose variant** (`choose_variant`). `wnd[0]` `sendVKey 17`. If the popup title contains `abap`, the popup is an ALV
list: `wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell`. Loop rows *i* and read `GetCellValue(i,"VARIANT")`. When it
contains the name set `SelectedRows=i`, `CurrentCellRow=i`, `DoubleClickCurrentCell`. Otherwise set
`wnd[1]/usr/txtV-LOW`=name and `wnd[1]/usr/txtENAME-LOW`="" (creator blank), then `sendVKey 0`, then `sendVKey 8`.
If `wnd[1]` is still open, the variant failed. Several routines instead press `wnd[0]/tbar[1]/btn[17]`, fill
`txtV-LOW` and `txtENAME-LOW`, and press `wnd[1]/tbar[0]/btn[8]`.

**P3 multi-value paste.** Put the values on the clipboard (one per line). Press the field's multi-select button
(`btn%_<FIELD>_%_APP_%-VALU_PUSH`). In `wnd[1]` do `sendVKey 16` (clear existing), then `sendVKey 24` (paste).
Verify that row 0 of
`wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/txtRSCSEL_255-SLOW_I[1,0]`
(or `ctxt...`) is non-empty (exclude list: `tabpNOSV/ssubSCREEN_HEADER:SAPLALDB:3030/tblSAPLALDBSINGLE_E/txtRSCSEL_255-SLOW_E[1,0]`).
If `wnd[2]` appears (non-numeric warning), press `wnd[2]/usr/btnBUTTON_1` (Yes) and re-check. ZMDETPC additionally
deletes row 0 with `wnd[1]/tbar[0]/btn[14]` when it is non-numeric. Then close with `sendVKey 8`. On failure re-run
P1 and repeat. *The headless runner should paste only data values (no header, no trailing blank).*

**P4 select or create layout** (`check_select_layout` / `SelectLayout` / `SetupLayout*`). Per tcode:

| tcode | open chooser | chooser list | create when missing |
|---|---|---|---|
| ZMDESNR, ZVT11, ZMDETPC, MB52 | `wnd[0]/tbar[1]/btn[33]` | grid `wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell` | `btn[32]` then `SetupLayout` on `/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620` |
| VT11 | menu `wnd[0]/mbar/menu[3]/menu[0]/menu[1]` | `wnd[1]/usr/lbl[1,i]`, i=1..60, match text, `SetFocus`, `sendVKey 2`; numeric layout = row index, decremented until found | menu `menu[3]/menu[0]/menu[0]` then `SetupLayout_li` on `/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810` |
| VL06O | menu `menu[3]/menu[2]/menu[1]` | `lbl[1,i]` | menu `menu[3]/menu[2]/menu[0]` then `SetupLayout_li` |
| LX02, LX03, LT23, VT22 | `btn[33]` | LT23: `wnd[1]/usr/cntlGRID/shellcont/shell`; others `lbl[1,i]` | `btn[32]` then `SetupLayout_li` |

Layout-creation dialog IDs: `wnd[1]/tbar[0]/btn[5]`; `wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/`
`chkG51_USPEC`, `chkLTVARIANT-DEFAULTVAR`, `txtLTDX-VARIANT`, `txtLTDXT-TEXT`; `wnd[2]/tbar[0]/btn[0]`;
`wnd[3]/usr/btnSPOP-OPTION1`. For LI-style: `wnd[1]/usr/btn%#AUTOTEXT002`, `wnd[0]/tbar[1]/btn[34]`, `btn[36]`,
`wnd[1]/usr/ctxtLTDX-VARIANT`, `wnd[1]/usr/txtLTDXT-TEXT`, `wnd[1]/usr/chkG_FOR_USER`,
`wnd[0]/mbar/menu[3]/menu[2]/menu[3]`, `menu[3]/menu[0]/menu[3]`. Column search popup: `wnd[1]/tbar[0]/btn[71]`,
`wnd[2]/usr/chkSCAN_STRING-START`, `wnd[2]/usr/txtRSYSF-STRING`, `wnd[3]/usr/lbl[1,2]`. Source code:
`Control_id_.bas` `SelectLayout` (:1581), `SetupLayout` (:1660), `SetupLayout_li` (:1773).
Arg flags: `layout` = layout name. `nosave:"true"` = never pick an existing layout; build the column set on the fly
and do not save it as a variant. `pre_export_back:true` = press Back once (`wnd[0]` `sendVKey 3`) before touching the
layout. Section 7 item 8 has the caveat on `pre_export_back`.

**P5 export** (`Check_Export_Window`, `save_sap_file`).

| tcode | export menu | expected popup title |
|---|---|---|
| ZMDESNR, ZVT11 | `wnd[0]/mbar/menu[0]/menu[3]/menu[1]` | `ZMDEMAIN SERIAL NUMBER HISTORY CONTENTS` |
| VT11 | `menu[0]/menu[10]/menu[0]` | `SHIPMENT LIST: PLANNING` |
| VL06O | `menu[0]/menu[5]/menu[1]` | `LIST OF OUTBOUND DELIVERIES` |
| LX02, LX03 | `menu[0]/menu[1]/menu[1]` | `BIN STATUS REPORT: OVERVIEW` |
| LT23 | `menu[0]/menu[1]/menu[1]` | `Transfer Orders: List of Resident Documents` |
| ZMDETPC summarised | `menu[0]/menu[3]/menu[1]` | `SAVE LIST IN FILE...` |
| ZMDETPC detail | `menu[0]/menu[1]/menu[2]` | `SAVE LIST IN FILE...` (saved as `.txt`) |
| VT22 | `menu[0]/menu[1]/menu[2]` then radio `[1,0]` (text with tabs) | saved as `.txt` |
| Y_DN3_47000149 | `menu[0]/menu[3]/menu[2]` | saved as `.txt` |
| ZLEQTY | `menu[0]/menu[3]/menu[1]` | |
| MB51 | `menu[0]/menu[1]/menu[1]` (or `menu[3]/menu[2]/menu[1]`) | |
| MB52 | `menu[0]/menu[1]/menu[1]`, then `wnd[1]/tbar[0]/btn[0]` | |

Popup handling: title contains `SELECT SPREADSHEET` → `wnd[1]/tbar[0]/btn[0]`. Title contains `SAVE LIST IN FILE...` →
`wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[1,0]` select, then
`wnd[1]/tbar[0]/btn[0]`. If no popup: right-click fallback on the grid. VT11 uses `wnd[0]` `sendVKey 44`; ZMDESNR uses
`/usr/cntlGRID1/shellcont/shell/shellcont[1]/shell`; MB51 and ZWM_MDE_COMPARE use `/usr/cntlGRID1/shellcont/shell`.
The sequence is `selectedRows="0"`, `currentcellrow=-1`, `contextmenu`, `selectContextMenuItem "&XXL"`, then
`wnd[1]/usr/chkCB_ALWAYS` = false. Save dialog: `wnd[1]/usr/ctxtDY_PATH`, `wnd[1]/usr/ctxtDY_FILENAME`,
`wnd[1]/tbar[0]/btn[0]`. Temp file name: `<yyyymmddhhmmss>_<tcode>.xlsx` (`.txt` as noted). **For the headless runner
choose "Text with tabs" (radio `[1,0]`), which Rust `export_local_file` already supports (`export_type` 1).** Header
and column differences between the xlsx spreadsheet export and the text-with-tabs export must be checked
(section 7 item 12).

**P6 outcome checks.** VT11: status bar text containing `E:` is an error. A `wnd[1]` information popup with text
containing `no shipments` means "no results". Status bar `no display authorization` is an error unless
`"option":["ignore_display_error"]`. A label at `wnd[0]/usr/lbl[2,4]` containing `no data` aborts. After layout
selection the status bar `No layouts found` is an error.

---

## 3. SAP tcode reference (inputs, IDs, outputs)

`Z..`-tcodes are customer transactions. `ZMDESNR` is the serial-number history. `ZVT11` is the shipment report.
`ZMDETPC` is the pallet count.

### 3.1 VT11 (shipment list) · Rust: yes (`vt11.rs`)

| Input | Control | Source |
|---|---|---|
| variant | P2 / `wnd[0]/tbar[1]/btn[17]` | D |
| shipment status (only if no variant) | `ctxtK_STTRG-LOW` = "1", `ctxtK_STTRG-HIGH` = "7" | |
| actual shipment end date from/to | `ctxtK_DATEN-LOW`, `ctxtK_DATEN-HIGH` | `date1`/`date2`, SAP date format. Single date: only `-LOW`. If `date1 == date2`: `-HIGH` is cleared (limiter path). |
| creation date from/to | `ctxtK_ERDAT-LOW`, `ctxtK_ERDAT-HIGH` | used when the date string contains `created` (section 7 item 6) |
| description of shipment | `txtK_TPBEZ-LOW/-HIGH`; multi `btn%_K_TPBEZ_%_APP_%-VALU_PUSH` | limiter paths |
| delivery multi | `btn%_S_VBELN_%_APP_%-VALU_PUSH` | limiter `delivery` |
| shipment multi (display screen) | `btn%_K_TKNUM_%_APP_%-VALU_PUSH` | `vt11_display_screen_info` |

Execute `wnd[0]/tbar[1]/btn[8]`. Then P6, P4 (VT11 column), P5. `arg:monday_include_sunday` (F) = if `date1` is a Monday,
replace the date with "previous day".
Output → `T[vt11]`. Columns come from layout, e.g. `scorecard_vt11_layoutrows`: Shipment Number, Ov.transport status,
Shipment type, TransportPlanningPt, Seal Number 2, Shipping type, Service agent, Trailer Number, Seal Number 1,
ActualShipmentEnd, Act.shipmnt end time, Descrip.of Shipment, Forwarding agent tracking ID.

### 3.2 ZVT11 (shipment report) · Rust: yes (`zvt11.rs`, partly)

Inputs: a delivery list from a previous table (`sheet`, `column`) and a date.
Type is `Closed` (default) or `Open` (rName contains `mg_`).

*Closed:* select tab `wnd[0]/usr/tabsTABSTRIP_BLK1/tabpTAB2`. Sub-screen prefix
`.../tabpTAB2/ssub%_SUBSCREEN_BLK1:ZSDR_SHIPMENT_REPORT:0102/`. Optional ship point `ctxtS_VSTEL2-LOW`. Variant via
`btn[17]` (if D is non-empty). Dates `ctxtS_DATE_2-LOW` = **date1 − 100 days**, `ctxtS_DATE_2-HIGH` = **date1 + 100 days**
(dates are deliberately wide because the delivery list is the real filter). Deliveries: multi `btn%_S_VBELN2_%_APP_%-VALU_PUSH`.
Radio `radR1` (display all deliveries). `chkP_ALV_2` = true.

*Open:* tab `tabpTAB1`, sub-screen `...:0101/`. Variant required. Deliveries `btn%_S_VBELN1_%_APP_%-VALU_PUSH`.
`chkP_ALV_1` = true.

Execute `wnd[0]/tbar[1]/btn[8]`. A popup `No data exists in the database for this selection` = error. Layout via
`btn[33]` + P4, export P5. Output → `T[zvt11]`, e.g. `add_del_zvt11_layoutrows`: Date, Trailer Number,
Shipment Number, Delivery Number, Plant, Ship To/From Name, Shipment End date.

### 3.3 VL06O (outbound delivery list) · Rust: yes (`vl06o.rs`)

Input: list of deliveries or shipments (column name contains `delivery` → `btn%_IT_VBELN_%_APP_%-VALU_PUSH`;
contains `shipment` → `btn%_IT_TKNUM_%_APP_%-VALU_PUSH`).
Steps: press `wnd[0]/usr/btnBUTTON6` ("List outbound deliveries"). Clear `ctxtIT_WADAT-LOW` and `-HIGH`
(no date limit). P3 paste. `wnd[0]` `sendVKey 8` (execute). Press `wnd[0]/tbar[1]/btn[18]` (item view).
Layout via menu `menu[3]/menu[2]/menu[1]` + P4. Export `menu[0]/menu[5]/menu[1]`. Output → `T[vl06o]`.
Layout examples (`LAYOUTS`):
`xdock`: Delivery, Ship-to party, Name of the ship-to party, External Delivery ID, Total gds mvt stat., Plant, Number of packages.
`pallets`: Delivery, Ship-to party, Name of the ship-to party, Deliv. date(From/to), Total gds mvt stat., Plant, External Delivery ID, Number of packages.
`inv`: as `xdock` but `Shipping Point/Receiving Pt` instead of Plant.
`rma_1`: Delivery, Ship-to party, Name of the ship-to party, Sold-to party, Total gds mvt stat., Plant, Number of packages.
`mg_view`: Delivery, Plant, Name of the ship-to party, Material, Delivery quantity, Picking quantity, Storage Bin, Description.

### 3.4 ZMDESNR (serial number history) · Rust: partly (`zmdesnr.rs`: delivery paste, serial, bin filter, tab number, pre-export back)

Selection screen tabs: `wnd[0]/usr/tabsTABSTRIP_TABB1/tabpUCOMM1` (by serial number), `tabpUCOMM2` (general selection),
`tabpUCOMM7` (user activity). Sub-screen prefixes are `.../tabpUCOMM1/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9001/`,
`.../tabpUCOMM2/...:9002/`, `.../tabpUCOMM7/...:9007/`. Below, `[1]`=9001, `[2]`=9002, `[7]`=9007.

| Mode (VBA routine / ACTIVE tcode) | Tab and controls |
|---|---|
| **user activity** (`zmdesnr` with rName containing `nc_`, `redtag`, `knock-offs`, `del_`, `inb`, `out_`; `tab` default 7) | `tabpUCOMM<tab>` select. `radial:"information"` → `[7]/radP_USER` + `[7]/ctxtS_DATE-LOW`/`-HIGH`. `radial:"report"` (default) → `[7]/radP_PENSKE`, `[7]/chkP_ALV` = true, `[7]/ctxtS_ERDAT1-LOW`/`-HIGH`. Dates = `date1`/`date2`. Execute `btn[8]`. |
| **by date** (`zmdesnr`, rName contains `busbar`) | `tabpUCOMM2`, `[2]/ctxtS_ERDAT-LOW`/`-HIGH` = `date1`/`date2`, execute `btn[8]` |
| **by deliveries** (`zmdesnr_del`, `sn_from_del_run`) | P2 variant (arg `var`), `tabpUCOMM2`, `[2]/btn%_S_VBELN_%_APP_%-VALU_PUSH` paste, `wnd[0]` `sendVKey 8`. **No date field is set** — the variant carries the date. |
| **by deliveries + TOs** (`zmdesnr_to`, `sn_from_to_run`) | variant via `btn[17]`; `tabpUCOMM2`; clear `[2]/ctxtS_PALLTD-LOW` (palletized "X" default), `[2]/txtS_VBELN-LOW`, `-HIGH`; paste deliveries `[2]/btn%_S_VBELN_%_APP_%-VALU_PUSH`; paste TO numbers `[2]/btn%_S_TANUM_%_APP_%-VALU_PUSH` (answer `wnd[2]/usr/btnBUTTON_1` while present); execute `btn[8]`. |
| **by serial** (`ZMDESNR_Run_Export`) | `tabpUCOMM1`; radio `[1]/radP_CURRNT` ("Current"), `[1]/radP_HISTRY` ("History"), `[1]/radP_OLDNEW` ("DB"/archive); `[1]/chkP_PALLET` = pallet children; paste serials `[1]/btn%_S_SERIAL_%_APP_%-VALU_PUSH`; execute `btn[8]` |
| **by deliveries excluding parents** (`zmdesnr_exclude`) | as "by deliveries", plus `[2]/btn%_S_PARENT_%_APP_%-VALU_PUSH` → `wnd[1]/usr/tabsTAB_STRIP/tabpNOSV` (exclude tab) → paste exclusion list |
| by bin / part / serial-from-PN (`zmdesnr_bin`, `_sn`, `_pn`) | `[2]/ctxtS_LGPLA-LOW`, `[2]/btn%_S_MATNR_%_APP_%-VALU_PUSH`, `[2]/ctxtS_MATNR-LOW`; `[1]/btn%_S_SERIAL_%_APP_%-VALU_PUSH` |
| by shipment / by material doc | `[2]/btn%_S_TKNUM_%_APP_%-VALU_PUSH`, `[2]/btn%_S_MBLNR_%_APP_%-VALU_PUSH` |

After execute: optional Back (`pre_export_back`), P4 (`btn[33]` / `btn[32]`), P5 (`menu[0]/menu[3]/menu[1]`).
Layout columns are in `LAYOUTS`, e.g. `mg_view_zmdesnr_layoutrows`: LM Serial Number, Status, Pallet, Parent Serial Number,
Container, Plant, Material, Customer Part #, Warehouse Number, Storage Location, Storage Type, Storage Bin, Ship-to party,
(blank), Delivery, Transfer Order Number, Created by, Time.
**`LAYOUTS` has no column list for `u_activity` or `busbar`.** Those layouts must already exist in SAP.

### 3.5 LT23 (transfer orders) · Rust: no

`ctxtT1_LGNUM` = warehouse (`E01`). Multi `btn%_T1_TANUM_%_APP_%-VALU_PUSH` (answer `wnd[2]/usr/btnBUTTON_1` while
present). Radio `radT1_QUITA` (confirmed only). `radT1_OFFTA` (open) and `radT1_ALLTA` (all) exist but are unused.
Date mode (not used by `ACTIVE`): `ctxtBDATU-LOW`/`-HIGH`. Execute `btn[8]`. Layout: `btn[33]`, grid
`wnd[1]/usr/cntlGRID/shellcont/shell`, else `btn[32]` + `SetupLayout_li`. Export `menu[0]/menu[1]/menu[1]`.
Layout `to_mg_lt23_layoutrows`: Delivery, Transfer Order Number, Creation Date, Creation time, User, Plant, Material,
Source Storage Type, Source Storage Bin, Source target qty, Confirmation status.

### 3.6 ZMDETPC (pallet count) · Rust: no

Select a variant (P2). Selection screen controls: `ctxtS_DATE-LOW`/`-HIGH` (= `date1`/`date2`),
`ctxtS_ADATE-LOW`/`-HIGH` (pallet pick date; blank unless `pallet_pick_date` arg), `btn%_S_VBELN_%_APP_%-VALU_PUSH`
(deliveries), radios `radR_SUMM` (summarised), `radR_COMPL`, `radR_DETA` (detail).
Summarised flow: set dates, paste deliveries (P3, delete a non-numeric first row with `wnd[1]/tbar[0]/btn[14]`; answer
`wnd[2]` messages with `sendVKey 0`), `radR_SUMM`, ensure `S_DATE-HIGH` filled, click `radR_COMPL` then `radR_SUMM`
(refresh), execute `btn[8]`, read `wnd[1]/usr/txtMESSTXT1` if a popup appears, export `menu[0]/menu[3]/menu[1]`
(local file, xlsx). Detail flow (`export_type:"detail"`): `radR_COMPL`, dates, paste, `radR_DETA`, execute,
layout via `btn[33]`, export `menu[0]/menu[1]/menu[2]` as `.txt`.
`pallet_pick_date` arg: `{"type":"default"}` = same as the date; `{"type":"dynamic","values":{"match"|"inner":n|"outer":n}}` =
widen (`outer`) or narrow (`inner`) the base range by *n* days; a collapsed inner range leaves `S_ADATE` blank.
**The `layout` and `date_range` args in `ACTIVE` have no effect in the summarised flow** (section 7 item 7).
The summarised export has three columns: `delivery`, `pallet`, `child` (confirmed from `resources/07-02-2026_XDOCK.xlsx`, section 9).

### 3.7 VT22 (change documents for shipments) · Rust: no

Variant via P2 (arg `variant`, e.g. `button`). Multi `btn%_OBJECT_%_APP_%-VALU_PUSH` = shipment numbers.
Radio `radVERSION1` (changes in shipment). Execute `btn[8]`. Layout via `btn[33]` (arg `layout`, e.g. `date_change`).
Export `menu[0]/menu[1]/menu[2]`, radio `radSPOPLI-SELFLAG[1,0]` (text with tabs), `btn[0]`.
Unused-in-flow selection fields: `txtG_USER-LOW`, `ctxtG_UDATE-LOW`/`-HIGH`, `ctxtOBJECT-LOW`, `ctxtSEL_TAB-LOW`, `ctxtSEL_FELD-LOW`.
Output `.txt` → `T[vt22]`, then post-processing (section 5.18).

### 3.8 VT03N (shipment display, read one value per shipment) · Rust: no

Loop over shipment numbers (skip blanks): P1, `wnd[0]/usr/ctxtVTTK-TKNUM` = shipment, `wnd[0]` `sendVKey 0`,
select tab `wnd[0]/usr/tabsHEADER_TABSTRIP2/tabpTABS_OV_TX`. Let `TXT` =
`wnd[0]/usr/tabsHEADER_TABSTRIP2/tabpTABS_OV_TX/ssubG_HEADER_SUBSCREEN2:SAPMV56A:1034/subTEXTEDIT:SAPLV70T:2101/cntlSPLITTER_CONTAINER/shellcont/shellcont/shell`.
Then `TXT/shellcont[0]/shell`.`SelectItem "CM20","Column1"`; `.doubleClickItem "CM20","Column1"` (text type
**CM20** = number of pallets). Read `TXT/shellcont[1]/shell`: `setSelectionIndexes 0,1`, then `.text`. Write that text
into column `pallets` of the row that held the shipment number.
Print variant (`vt03n_print`, not in `ACTIVE`): `ctxtVTTK-TKNUM`; menu `menu[0]/menu[4]/menu[0]`;
`wnd[1]/usr/ctxtDNAST-KSCHL` = output type (default `ZMBL`); `btn[0]`; `wnd[1]/usr/btnSPOP-OPTION1` (yes);
`wnd[1]/usr/ctxtNAST-LDEST` = printer (default `DFLT`); `btn[0]`; `wnd[1]/tbar[0]/btn[86]`.

### 3.9 Other tcodes (not used by `ACTIVE` rows, but handled by the macro)

| tcode | Controls |
|---|---|
| **LX02 / LX03** | variant (P2), execute `btn[8]`, layout P4, export `menu[0]/menu[1]/menu[1]`. Rust: LX03 yes. |
| **LX04** | variant (P2), execute `btn[8]`, read screen labels `wnd[0]/usr/lbl[row,col]` (`get_info_lbl`) into a table. No export. |
| **Y_DN3_47000149** (149 report; Rust `y_149*.rs`) | variant (`btn[17]`, `txtV-LOW`, `txtENAME-LOW`, `btn[8]`). `radR_ALL`, `radR_IN_TRA` (in transit), `ctxtS_D_DATE-LOW`/`-HIGH`, `ctxtS_WERKS-LOW`, `txtS_DELIV-LOW`, multi `btn%_S_MATNR_%…`, `btn%_S_WERKS_%…`, `btn%_S_SIGNI_%…`, `btn%_S_DELIV_%…`. Execute `btn[8]`; layout `btn[33]`/`btn[32]`; export `menu[0]/menu[3]/menu[2]` → `.txt`. |
| **MB51** | `ctxtMATNR-LOW` + multi `btn%_MATNR_%_APP_%-VALU_PUSH`, `ctxtBUDAT-LOW`/`-HIGH`, `chkDATABASE`, `chkSHORTDOC`, `chkARCHIVE`, `txtMBLNR-LOW`, `radRFLAT_L`/`radRHIER_L`, `ctxtALV_DEF`, `btn[8]`, `btn[48]`; export `menu[0]/menu[1]/menu[1]` |
| **MB52** | `ctxtP_VARI` (variant), `btn[8]`, `lbl[2,3]` as no-data probe, export `menu[0]/menu[1]/menu[1]` + `wnd[1]/tbar[0]/btn[0]` |
| **ZLEQTY** | per plant: `ctxtP_WERKS`, `radP_RD1`, `ctxtS_LGNUM-LOW` (= `E01`), `btn[8]`, export `menu[0]/menu[3]/menu[1]`. Plant list = `LOOKUPS!W` (`zleqty_plant`). |
| **VL03N** | `ctxtLIKP-VBELN`, `btn[18]`, tab `wnd[0]/usr/tabsTS_HU_VERP/tabpUE6INH`, table `ssubTAB:SAPLV51G:6040/tblSAPLV51GTC_HU_005/txtHUMV4-IDENT[1,j]` (HU identifiers, row j) |
| **VL02N** (`vl06o_date_update`, `delivery_qty_update`) | `ctxtLIKP-VBELN`, tab `wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01`, `.../ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT` (goods issue date), `wnd[0]/usr/subSUBSCREEN_HEADER:SAPMV50A:1502/ctxtLIKP-VBELN`, `wnd[1]/usr/ctxtRV50A-PO_MATNR`, `.../tblSAPMV50ATC_LIPS_OVER/txtLIPSD-G_LFIMG[2,0]`, save `wnd[0]/tbar[0]/btn[15]`, confirm `wnd[1]/usr/btnSPOP-OPTION1` |
| **XD03** | `wnd[1]/usr/ctxtRF02D-KUNNR`, `-BUKRS`, `-VKORG`, `-VTWEG`, `-SPART`, `wnd[1]/tbar[0]/btn[7]`, `btn[14]`, `wnd[2]/tbar[0]/btn[0]`, read `wnd[1]/usr/txtINT_KNA1-NAME1`, `.../tabpTAB01/ssubSUBSC:SAPLATAB:0201/subAREA1:SAPMF02D:7111/subADDRESS:SAPLSZA1:0300/subCOUNTRY_SCREEN:SAPLSZA1:0301/txtADDR1_DATA-POST_CODE1` and `...-CITY1` |
| **ZMDEETSP** (`run_tsp`) | `wnd[0]/usr/txtZMDEMAIN-LGORT`, `ctxtZMDEMAIN-WERKS`, `txtV_SERIAL_NO`, `wnd[0]` key presses |
| **ZMDEPD** (`run_zmdepd`, `post_mde_deliv`, `get_push_label`) | `txtST_SCREEN-WERKS`, `-DELIVERY`, `-DELV_TYPE`, `-VSTEL`, `-SHIP_TO`, `-VKORG`, `-VTWEG`, `-SPART`, `-LGORT`, `-SERIAL`, `-LABEL_QTY`; option fields `txtV_OPTION_0100`, `_0112`, `_0150`; `btnCONTINUE`, `btnBACK`, `btnSELECT`, `btn%#AUTOTEXT008`; printer `txtST_PRINTER_SCREEN-V_PRNTR_CODE`; errors `sub:SAPMZMDEPD:2400/txtTBL_ERROR_DATA[row,0]`; `wnd[0]/tbar[0]/okcd`; screen format `wnd[0]/usr/txtLOGON_DATA-DEVTY` |
| **Login** | `wnd[1]/usr/radMULTI_LOGON_OPT1` (multi-logon popup, continue without ending other sessions). Rust has its own login. |

---

## 4. Table-operation glossary (method-agnostic)

`Sel(T,pred)` = **non-destructive row selection**: rows that fail `pred` are *hidden*, not removed, and later steps
that read "visible rows" (list pasted into SAP, `dedup visible`, `add_col_value`, `save`) see only the selected rows
until the selection is cleared. Option `"delete"` makes the selection destructive (non-matching rows are removed).
In the TSV runner, represent this as a selection mask plus an explicit `reset` and `materialize`. The cell value of a
**criterion** follows Excel: `x` or `=x` equal (case-insensitive); `<>x` not equal; `=` blank; `<>` non-blank;
`>n`, `<n`, `>=n`, `<=n` numeric/date compare; `*` and `?` wildcards (`<>KC*` = does not start with KC).
A named filter (section 4.2) with several criteria on one row = AND.

### 4.1 `filter` (single column)
`{"sheet":S,"filter_col":C,"filter_val":V}`. Defaults: column `plant`, value `=`. `Sel(S, C matches V)`.
Option `keep_prev_filter` = AND with the existing selection (otherwise the previous selection is cleared first).
Option `delete` = remove non-matching rows. Option `reset_filter` = clear the selection afterwards.
`filter_val` may be `{"cell_ref":"B2"}` (read the value from `ACTIVE`). With `dynamic`/`dynamic_filter_val` = list of dates.

### 4.2 `filter` with `"filter_type":"adv_filter"`
`filter_str` names a block on `FILTERS`: header names across a row, criteria below. Cells in one row = AND, rows = OR.
Rows are materialised in section 8. Without `delete`, the effect is a selection. With `"option":"delete"`, non-matching
rows are removed. (`F` containing `visible` applies it only to already-selected rows.)

### 4.3 `dedup`
`{"sheet":S,"column":C}` or `"column":[C1,C2]` or `"dedup":{"columns":[...],"type":"visible"}`. Remove rows whose
key (one or several columns) was already seen; the **first** occurrence is kept. `column:"all"` = whole-row key.
`type:"visible"` = only among selected rows. **`"option":"visible"` (as on `A!15`) is not read by the code**
(section 7 item 5).

### 4.4 `lookup`
`D={"sheet":SRC,"col_key":K,"col_val":V}`, `E={"sheet":DST,"col_lookup":L,"col_insert":N,"col_ins_pos":P,"nf_value":X,"found_value":F}`.
Insert a new column `N` into `DST` at position `P` (a number, `first`, `last`, `before:<col>`; default end). For each row
of `DST`, find `L` among `SRC.K` (case-insensitive, first match wins). If found, write `SRC.V` (or the constant `F` if
`found_value` is set; then `V` is ignored). If not found, write `X` (default `-`). `F` flag `skip_hidden` = look only at
selected `SRC` rows.

### 4.5 `add_col_value`
`D` may be `reset_filter` (clear selection first) or a filter spec. `E={"sheet":S,"col":C,"value":V|"value_dyn":"date"}`.
Create column `C` at the end if missing. Set it to `V` on all rows (`F` flag `visible` = selected rows only;
`hidden` is unsupported). `value_dyn:"date"` writes the raw `E4` cell value (section 7 item 2).

### 4.6 `insert_formula` / `insert` / `insert_column`
`insert_formula`: `E={"sheet":S,"formula_col_name":N,"formula_col_pos":P,"formula":"=..."}` or `direct_cell` for a single
cell (`F` flag `copy_down` = fill the whole column below). Formulas are **R1C1 relative** (`RC[-1]` = same row, previous
column). Adds column `N` at position `P` (1-based) and fills each data row. `F` flag `format:general` = number format.
`F` flag `save` = save the workbook now. `insert`: `{"sheet":S,"column_name":N,"column_position":"before:<col>"|"after:<col>"|"last"|n,"column_formula":"=..."}`;
the formula is A1-style relative to the first data row. `insert_column`: add empty column `col` at the end.
In the TSV runner each formula becomes a computed column with the semantics given in the report section.

### 4.7 `compare`
`{"sheet":S,"col1":A,"col2":B,"outcol":N}` (default `N` = `bruh`). New last column = `A − B` per row (General format).

### 4.8 `regex`
`D={"sheet":S,"in_col":I,"out_col":O}` (or `"action":"delete"`), `E={"pattern":P,"replace":R}`. For each row take the first
match of `P` in `I` (case-insensitive; whole match including parentheses). If there is a match and `R` is set: if the
current `O` value **contains** `R`, set `O` to the match. If `R` is not set: set `O` only when `O` is empty. With `action:"delete"`: delete rows
that match.

### 4.9 `remove_one`
`{"sheet":S,"col":C,"action":"delete"|"delete_row"|"filter"|"highlight"}`. Sort by `C` ascending, then for each distinct
`C` value remove/hide/highlight **one** occurrence (the first in sorted order). Section 7 item 9: the implementation of
`delete_row` appears to be off by one; the intent is "one row fewer per distinct value".

### 4.10 `copy_sheet`
`{"sheet":S,"name":N}`: duplicate table `S` (current rows including hidden ones) as `N`.

### 4.11 `sort` / `multi_sort` / `multi_filter` / `highlight` / `dbfilter` / `morph_datetime` / `calc_time_diff`
Not used by `ACTIVE` as checked in; they exist in `run_misc_op` (`sequence_.bas:1608`). `morph_datetime(date_col,time_col,datetime_col)` =
datetime column = date + time (`MM/DD/YYYY hh:mm:ss AM/PM`).

### 4.12 `merge ZMDETPC into <table>` (F=`merge <table>` on a ZMDETPC step)
1. In the ZMDETPC table rename column `delivery` → `delivery_`.
2. In `<table>`: drop rows with duplicate `delivery` (first kept). Sort `<table>` by `delivery` and the ZMDETPC table by `delivery_`.
3. Append the ZMDETPC columns to the right of `<table>` row-by-row (positional). Then insert blank rows where the
   two `delivery` columns disagree (`insert_spacers(delivery, delivery_, 4)`) so that rows line up. Net effect: **outer
   join on delivery, ordered by delivery**.
4. Add column `total` = `pallet + child` (`=SUM(RC[-2]:RC[-1])`, the two columns immediately left of it). Resulting column order: `<table columns>, delivery_, pallet, child, total`.
5. If rName contains `xdock` or `toyota`: where `total` is blank copy `number_of_packages` into it; where `delivery_` is
   blank copy `delivery` into it; replace `0` in `total` with `1`. If rName contains `add_del`: in column `pallets`, any
   non-numeric or empty cell becomes `1` (cell highlighted).
   (Section 7 item 3 on the `0` → `1` replacement.)

### 4.13 `save`
`D={"data_sheet":S,"sheets":[...],"name":N,"date":{...},"date_format":F}` and `E={"pivot_sheet":P,"row1":..,"row2":..,"col1":..,"sum":..,"count":..,"filteritem":..,"filtervalue":..,"tabular":..,"distinct_count":..,"option":[...]}`.
Write the listed tables (`sheets`, substring-matched, first = `data_sheet`) into one output (xlsx workbook today;
a set of TSVs in the target). Options: `subdir`, `close_file`, `pivot`. **A pivot is created only when `option`
contains `pivot` or `F` contains `pivot`.** `date_format` overrides the date part of the file name.

### 4.14 `pivot` (as a data operation)
Group `data_sheet` by `row1` (then `row2`) and optional `col1` (cross-tab); aggregate `sum` = SUM of that column, or
`count` = COUNT of non-empty values (or distinct count if `distinct_count:true`). Optional `filteritem`/`filtervalue` =
page filter (exact). Pivot totals (row and grand totals) are included by default. For the TSV runner emit either the
cross-tab or a long-form group-by table; the choice affects downstream consumers (section 7 item 11).

### 4.15 `cleanup`
`{"option":"rename_sheets"}` = upper-case sheet names (cosmetic; drop in the TSV runner).
`{"option":"format","sheet":"zvt11"}` = see 5.18 row 136 (`zvt11` format) and `{"option":"format","sheet":"vl06o"}`
(a report variant using LX02; not used by `ACTIVE`).
`{"option":"filter"}` (not used) = advanced filter `abc_vt11_filter_1` on `service_agent`, then delete rows where
`shipment_number` is empty.

### 4.16 `executable`
`D={"path":EXE,"subdir":bool,"date_format":F,"append_filename":bool}`, `E={"args":{...},"argpath":"from_rname"|<path>}`.
Run `EXE` with the non-empty `args` values (quoted) plus one last argument: the output folder for the rName
(`from_rname`, with `subdir` honoured), with the saved file name appended when `append_filename` is true.

### 4.17 `combine` (F on a VT11 step, `A!131`)
Returns the VT11 table without further action. (`combine` as tcode merges named workbooks and is not used.)

---

## 5. Report specifications

For every block: SAP steps (what to do), table steps, output. Step numbers are `ACTIVE` rows.
All SAP steps use P1 → P2 (variant) → inputs → execute → P4 → P5, as in section 2, unless said otherwise.
"Date" = the run date from `E4` (section 1.1). "Layout" = a layout name resolved by P4.

### 5.1 `mustgo` (`A!9`-`A!23`) → folder `K2XX`
| Row | Step |
|---|---|
| 9 | **VT11** variant `XD_ALL_EPDC_`, date, option `ignore_display_error`, no layout (export as-is) → `T[vt11]` |
| 10 | `Sel(vt11, transportplanningpt = TPDC)` (`mg_vt11_filter_1`), non-destructive |
| 11 | **VL06O** with `vt11.shipment_number` (selected rows), no layout → `T[vl06o]` |
| 12 | **ZMDESNR by deliveries** with `vl06o.delivery`, variant `mg_k2xx`, layout `mg_view` → `T[sn_del]`; a copy is kept as `snr_orig` |
| 13-14 | `Sel(sn_del, parent_serial_number non-blank)` then AND `status <> PAL` |
| 15 | `dedup(sn_del, parent_serial_number)` among selected rows (intent "dedup parent visible") |
| 16 | **LT23** with `sn_del.transfer_order_number` (selected rows), warehouse `E01`, confirmed TOs only, layout `to_mg` → `T[lt23]`. Then **post-process `mustgo`** (F=`mustgo`): (a) dedup `lt23` by `transfer_order_number`; (b) add `datetime` after `creation_time` = `creation_date + creation_time`; (c) sort by `delivery` desc, `datetime` desc, `source_storage_bin` desc; (d) add `bDup` = count of rows with the same `delivery` if > 1 else `"unique"`; (e) delete rows with `bDup = unique` (deliveries with one TO); (f) for each remaining `delivery`, delete one row — the **last** row in sorted order, i.e. the earliest TO |
| 17 | `lookup`: key `lt23.transfer_order_number` → into `sn_del` new column `count`: `1` if the row's `transfer_order_number` exists in `lt23`, else `x` |
| 18 | delete rows of `sn_del` where `count = x` |
| 19 | **ZMDESNR by deliveries + TOs** (`del_col delivery`, `to_col transfer_order_number`, variant `mg_k2xx`, layout `mg_view`) → `T[sn_to]`; copy kept as `to_orig` (`action:"dupsheet"`). Then the internal **`Filter_Copy_MG`**: (a) run **ZMDESNR by serial / History** for `sn_to.lm_serial_number` → `mustgo_sn_hist`, filter it (`storage_location` in {3,10}, `status = RCV`, `created_on` between `date1` and `date2`) and keep as `mustgo_filtered`, sorted `created_on`, `time`; (b) copy `sn_to` as `snr_copy`, `Sel(parent_serial_number non-blank)`, dedup visible by `parent_serial_number`, clear selection. **`mustgo_filtered` and `mustgo_sn_hist` never reach the output** (section 7 item 10) |
| 20 | add column `week` to `snr_copy` at position 19, formula `IFNA(WEEKNUM(<column 18>),"-")`, general format. Column 18 is `created_on` in the real SAP layout (section 9), so `week` = week number of `created_on` |
| 21 | `reset_filter`, then add column `export_date` = run date value to `snr_copy` |
| 22 | **save** `data_sheet snr_copy`, sheets `lt23`, `snr_copy`, `pivot`; pivot `pivot`: rows `plant`, cols `export_date`, count `lm_serial_number`; options `close_file`, `pivot`, `subdir` |
| 23 | `del_exp` |

Output: `reports\K2XX\<today>\mustgo\<date>_MUSTGO.xlsx` with sheets `LT23`, `SNR_COPY`, `PIVOT`.

### 5.2 `toyota` (`A!25`-`A!33`) → folder `INV`
| Row | Step |
|---|---|
| 25 | **VT11** variant `FG_TOYO_XD`, layout `scorecard`, date, `ignore_display_error` → `T[vt11]` |
| 26 | **VL06O** with `vt11.shipment_number`, layout `xdock` → `T[vl06o]` |
| 27 | clear selection, add `vl06o.date` = run date value |
| 28 | **ZMDETPC** summarised with `vl06o.delivery`, variant `EPDC_TONY`, date → `T[zmdetpc]`; **merge ZMDETPC into vl06o** (4.12, `xdock/toyota` rules) |
| 29 | rename sheets (drop) |
| 30 | **save** `data_sheet vl06o`, sheets `vt11`, `vl06o`, `pivot`; pivot rows `plant`, sum `total`; options `pivot`, `subdir` |
| 31 | on the saved `pivot` table, cell `E2` = unique values of column B (the plant column of the pivot) |
| 32 | on `pivot`, cell `F2` copied down: `ROUND(SUMIF(B:B, E<row>, C:C) / 14.5, 0)`, blank on error; file saved and closed |
| 33 | `del_exp` |

Rows 31-32 add a **trailers estimate per plant = round(total ÷ 14.5)** next to the pivot (plants in `E`, estimate in `F`).
A pivot grand-total row may appear in `E` if present in column B.

### 5.3 `xdock` (`A!35`-`A!42`) → folder `INV`
Same as toyota rows 25-30 with VT11 variant `XD_ALL_EPDC_` (layout `scorecard`), VL06O layout `xdock`, plus
`A!38` **ZVT11 Closed** (variant `E01_601_FV_FW`, layout `add_del`, `vl06o.delivery`, date ± 100 days) → `T[zvt11]`.
**`zvt11` is not in the save list and no later step uses it** (section 7 item 10). Save sheets `vt11`, `vl06o`, `pivot`;
pivot rows `plant`, sum `total`; options `subdir`, `pivot`, `close_file`. No trailer estimate rows.

### 5.4 `misc` (`A!44`-`A!50`) → folder `INV`
| Row | Step |
|---|---|
| 44 | **VT11** variant `MISC_EPDC_`, layout `scorecard`, date, `ignore_display_error` |
| 45 | **VL06O** layout `inv`; F=`remove_dups:delivery`: dedup `vl06o` by `delivery` |
| 46 | add `vl06o.date` |
| 47 | `regex`: for each `vl06o` row take the first `\((.*?)\)` match (with parentheses) of `name_of_the_shipto_party`; if `shipping_pointreceiving_pt` contains `EPDC`, replace that cell with the match |
| 48 | rename sheets (drop) |
| 49 | **save** sheets `vt11`, `vl06o`, `pivot`; pivot rows `shipping_pointreceiving_pt`, sum `number_of_packages`; options `close_file`, `pivot`, `subdir` |

### 5.5 `k2xx` (`A!52`-`A!59`) → folder `K2XX`
| Row | Step |
|---|---|
| 52 | **VT11** variant `TIER_VT11_K2XX`, layout `scorecard`, date, F=`arg:monday_include_sunday` |
| 53 | **VL06O** layout `pallets` |
| 54 | add `vl06o.date` |
| 55 | **ZVT11 Closed**, variant `k2xx_`, layout `add_del`, `vl06o.delivery`, date ± 100 days → `T[zvt11]` |
| 56 | **ZMDETPC** summarised, variant `EPDC_TONY`, date (`date_range:7` has no effect); merge into `vl06o` |
| 57 | rename sheets (drop) |
| 58 | **save** `data_sheet vl06o`, sheets `zvt11`, `vt11`, `vl06o`, `pivot`; pivot spec rows `name_of_the_shipto_party`, cols `plant`, sum `total`; options `close_file`, `subdir` — **no `pivot` option, so as written no pivot is produced** (section 7 item 1) |

### 5.6 `relabel` (`A!61`-`A!65`) → folder `K2XX`
Needs the `vl06o` table of the `k2xx` block of the same run (section 9.5: `del_exp` rows without `x` are skipped, so tables persist). **Section 9.5 holds the verified algorithm; where it differs from the list below, 9.5 wins.**
| Row | Step |
|---|---|
| 61 | **ZMDESNR by deliveries** with `vl06o.delivery`, variant `relabel_k2xx`, layout `epdc_relabel` → `T[sn_del]` (+ copy `snr_orig`) |
| 62 | **save** name `full_label`, sheets `sn_del`, `snr_orig`, option `subdir` |
| 63 | **`filter_copy_snr`** `sn_del` → `combined` (below) |
| 64 | add `combined.export_date` = run date value |
| 65 | **save** `data_sheet combined`, sheets `pivot`, `combined`, `sn_del`, `snr_orig`; pivot rows `customer`, cols `plant`, sum `_of_labels`; options `close_file`, `pivot`, `subdir` |

**`filter_copy_snr` (`Relabel_Filters_.Filter_Copy_SNR_E01`).** Let `D` = the run date (must be a single date, not
JSON; section 7 item 16) and `W` = [`D`, `D+1`] on `document_date`. Operate on `sn_del` (call it `F`):
1. Fill blank header with `customer`; format `document_date` as a date.
2. Any row with `plant = FV55` and `customer` matching `LSI*`, `GM*` or `GENERAL*` (`rework_relabel_gm55`) has `plant`
   set to `GM55` for the duration of this routine (restored to `FV55` at the end).
3. Build `combined` (`C`) by appending, in this order, always as full rows with one header row at the top:
   * **A**: `plant <> FV55` and non-blank, `spv_scan_copies = 2`, `document_date ∈ W`. If non-empty, underline the block.
   * **B**: `plant <> FV55` and non-blank, `serial_number_2nd_party_verification` blank, `document_date ∈ W`.
     Then delete empty rows (by `lm_serial_number`), sort `C` by `customer`, `plant`, `parent_serial_number`, `lm_serial_number`.
   * **PAL**: `plant <> FV55`, `status = PAL`, sorted likewise. Copy, dedup by `parent_serial_number`, append with a blank separator.
   * **FV55 history**: `plant = FV55`, `pallet = 1`. If non-empty, run **ZMDESNR by serial / History** on those serials
     (variant `relabel_k2xx`, layout `epdc_relabel`) → `serial_history`; keep rows with `storage_location = 11` and `status = PRI`;
     if any remain, highlight the matching serials in `F`.
   * **P**: `pallet ∈ {1,2}` and `plant = FV55`. Blank separator.
   * **Q**: `plant = FV55`, `pallet` blank, `parent_serial_number` blank.
   * **R**: `parent_serial_number` non-blank, `kanban_number` non-blank and not starting with `0`. Sorted as above.
4. Delete empty rows by `lm_serial_number`, resort as above, add column `of_labels` (General).
5. Labels: apply `k4xx_relabel_filter_2` (`plant=FV55`, `kanban_number` blank, `container <> KC*`, `pallet = 1` **or** `2`);
   rows highlighted yellow are excluded. If at least 2 rows remain set `of_labels = 4` on those rows where it is empty.
   Clear the filter, then set `of_labels = 2` on every other row where it is still empty (any non-empty `customer`).
6. Replace `GM55` with `FV55` (whole-cell) in `C`. Output `C`.
The exact set of rows that receive 4 labels must be validated against a real run (section 7 item 17).

### 5.7 `repack` (`A!68`-`A!72`) → folder `K2XX`
Needs `vl06o` (k2xx) and `combined` (relabel) from the same run (section 9.5). **Section 9.5 holds the verified algorithm; where it differs, 9.5 wins.**
| Row | Step |
|---|---|
| 68 | **ZMDESNR by deliveries excluding parents**: deliveries `vl06o.delivery`, variant `repack_k2xx`, layout `epdc_repack`; exclusion list = `combined.lm_serial_number` restricted to cells with **yellow fill in column 2** (selection logic of `Repack_Filter_Exclude`); skipped if fewer than 2 such rows → `T[repack_sn]` |
| 69 | **`filter_copy_repack`** `repack_sn` → `snr_repack` (below) |
| 70 | add `snr_repack.export_date` |
| 71 | **save** `data_sheet snr_repack`, sheets `pivot`, `snr_repack`, `repack_sn`; pivot rows `customer`, cols `plant`, count `lm_serial_number`; options `close_file`, `pivot`, `subdir` |

**`filter_copy_repack`** (`Repack_Filters_.Filter_Copy_SNR_Repack`), on `F` = `repack_sn`, output `C` = `snr_repack`:
1. Fill blank header with `customer`; apply the same `GM55` rule as relabel step 2.
2. `C` := rows with `plant <> FV55` (non-blank), `parent_serial_number` blank, `net_weight < 50` (with header).
3. `TEMP` := rows with `plant <> FV55`, `parent_serial_number` blank, `net_weight > 50`. If `TEMP` is non-empty, run
   **ZMDESNR by serial / History** on `TEMP` serials (variant `repack_k2xx`, layout `epdc_repack`) → `serial_history_repack`;
   keep `warehouse_number = E01` and `status = UNP`; append to `C` (no header).
4. Append rows with `plant <> FV55`, `status = PAL` (no header). Sort `C` by `customer`, `plant`, `parent_serial_number`, `lm_serial_number`.
5. If `C` is non-empty, delete rows whose `container` matches `KC*` or `SC*` (`repack_filter_kc`).
6. Append rows with `plant = FV55`, `pallet` blank, `container <> KC*` (blank row first). Sort as above. Delete empty rows.
   Replace `GM55` with `FV55`. Output `C`.

### 5.8 `nc_tsp` (`A!74`-`A!81`) → folder `ABC`
| Row | Step |
|---|---|
| 74 | **ZMDESNR user-activity report** (tab 7, radio `radP_PENSKE`, `chkP_ALV`, `S_ERDAT1` = `date1`/`date2`), variant `NC_TSP_T`, layout `nc_mp` built on the fly (`nosave`), Back once (`pre_export_back`) → `T[tsp]` |
| 75 | `tsp` := rows matching `tsp_zmdesnr_filter_a` (delete others): `status=TSP`, `warehouse_number=E01`, `container` not `KC*`, `C0028934*`, `C0027948*`, `C0029854*`, `C0014595*` |
| 76 | copy `tsp` → `orig` |
| 77 | `tsp` := rows matching `tsp_zmdesnr_filter_b` (delete others): `parent_serial_number` blank and `pallet <> 1` ("orphans") |
| 78 | copy `tsp` → `main` |
| 79 | `remove_one(main, storage_bin, delete_row)`: sort `main` by `storage_bin` ascending, then remove the first row of every distinct `storage_bin` (bins with a single serial disappear). Decision 9 (the macro's off-by-one is fixed) |
| 80 | **save** `data_sheet main`, sheets `orig`, `tsp`, `main`, `pivot`; pivot rows `plant`, count `storage_bin`; options `close_file`, `subdir`, `pivot` |

### 5.9 `inb_tsp` (`A!83`-`A!89`) → folder `ABC`
| Row | Step |
|---|---|
| 83 | **ZMDESNR user-activity information** (tab 7, radio `radP_USER`, `S_DATE-LOW/HIGH` = `date1/date2`), variant `EPDC_TSP_USR`, layout `u_activity`, no Back → `T[in]` |
| 84 | keep rows with `parent_serial_number` blank (`inb_tsp_filter_1`, delete others) |
| 85 | `dedup(in, [storage_bin, created_on])` |
| 86 | delete rows with `plant = FV30` (`filter_val "<>FV30"` + `delete`). **CLAUDE.md says "filtered to plant FV30"; the code removes FV30.** (section 7 item 18) |
| 87 | **save** sheets `in`, options `subdir`, `close_file`, `date_format yyyy-mm-dd` |
| 88 | run `C:\dist\inb_rs\inb_rs.exe` with argument = full path of the file saved in row 87 (`argpath from_rname`, `append_filename true`, `YYYY-MM-DD`, subdir). **inb_rs reads the xlsx** (section 7 item 19) |

### 5.10-5.12 `out_shp`, `out_rel`, `out_pal` (`A!91`-`A!107`) → folder = own name
Each: **ZMDESNR user-activity information** (as 5.9) with variant `EPDC_SHP_USR` / `EPDC_REL_USR` / `EPDC_PAL_USR`, layout
`u_activity`, no Back → `T[out]` / `T[rel]` / `T[pal]`; keep rows with `parent_serial_number` blank; delete rows with
`plant = FV30`; **save** the single table (`close_file`).

### 5.13 `inb_rcv` (`A!109`-`A!112`) → folder = own name
**ZMDESNR user-activity information**, variant `EPDC_IN_USR`, layout `u_activity` → `T[in]`; keep rows with
`parent_serial_number` blank; **save** (`close_file`). No plant filter.

### 5.14 `rma` (`A!114`-`A!121`) → folder `ABC`
| Row | Step |
|---|---|
| 114 | **VT11** variant `RMA_K2XX`, date (`filterby created` — see section 7 item 6), `ignore_display_error`, no layout |
| 115 | **VL06O** layout `rma_1` |
| 116 | **ZVT11 Closed** variant `RMA_ZVT`, layout `rma_1`, `vl06o.delivery`, date ± 100 days → `T[zvt11]` |
| 117 | format `zvt11` (5.18 row 136) |
| 118 | `lookup` key `zvt11.delivery` value `zvt11.date` → into `vl06o` new first column `date`, missing → `x` |
| 119 | **ZMDETPC** summarised, variant `epdc_tony`, date (`date_range:3` no effect); merge into `vl06o` |
| 120 | **save** `data_sheet vl06o`, date, sheets `zvt11`, `vl06o`, `vt11`; pivot spec rows `date`, `name_of_the_shipto_party`, col `plant`, sum `total`, tabular — **no `pivot` option, so no pivot as written** |

### 5.15 `redtag` (`A!123`-`A!125`) → folder `ABC`
| Row | Step |
|---|---|
| 123 | **ZMDESNR user-activity report** (tab 7, `radP_PENSKE`, `S_ERDAT1`), variant `PENSKE_RTG`, layout `rtg` (`nosave`), Back once → `T[zmdesnr]`. F=`filter`: dedup by `serial_`; keep rows matching `redtag_zmdesnr_filter` (`status=ASH`, `palletized` blank, `created_by` in `KJPP8Z`, `SZNCSS`, `TJ7B7L`, `NZ4NJH`); sort by `created_on` asc; **save immediately** as `<date>_REDTAG...xlsx` |
| 124 | **save** again (`data_sheet zmdesnr`), sheets `zmdesnr`, `pivot`; pivot rows `created_on`, cols `plant`, count `serial_` (pivot is built because F contains `pivot`). The second save gets a numeric suffix. |

### 5.16 `busbar` (`A!127`-`A!128`) → folder `ABC`
| Row | Step |
|---|---|
| 127 | **ZMDESNR by date** (tab `tabpUCOMM2`, `S_ERDAT-LOW/HIGH` = `date1/date2`), variant `BUSBAR_FV33`, layout `busbar` (must exist in SAP), no Back → `T[zmdesnr]`. F=`sort`: sort `pallet` desc, then `parent_serial_number` asc; dedup by `lm_serial_number`; **delete** rows matching `busbar_zmdesnr_filter_1` (`status = SHP` and `parent_serial_number` non-blank); then among rows matching `busbar_zmdesnr_filter_2` (`parent_serial_number` non-blank) dedup by `parent_serial_number`; clear selection; **save** as new book |
| 128 | **save** `data_sheet zmdesnr`, sheets `pivot`, `zmdesnr`; pivot rows `created_on`, count `pallet`; option `pivot` |

### 5.17 `nc_masterpack` (`A!144`-`A!153`) → folder `ABC`
| Row | Step |
|---|---|
| 144 | **ZMDESNR user-activity report**, variant `NC_MPK_TSP`, layout `nc_mp` (`nosave`), Back once → `T[tsp]` |
| 145 | `tsp` := `nc_mp_zmdesnr_filter_tsp` rows: `pallet<>1`, `plant<>FV30`, `parent_serial_number` blank, `status=TSP`, `gross_weight>60`, `warehouse_number=E01`, `container<>KC*` |
| 146-147 | same with variant `NC_MPK_ASH` → `T[ash]`, filter `nc_mp_zmdesnr_filter_alt` (status `ASH` instead of `TSP`) |
| 148-149 | same with variant `NC_MPK_PAL` → `T[pal]`, filter `nc_mp_zmdesnr_filter_2`: `status=PAL`, `warehouse_number=E01` |
| 150 | `lookup` key `pal.serial_` value `pal.status` → into `ash` new column `stat_2` at position 6, missing `-` |
| 151 | `lookup` key `tsp.serial_` value `tsp.status` → into `ash` new column `stat_1` at position 5, missing `-` |
| 152 | **save** `data_sheet ash`, sheets `tsp`, `pal`, `ash`, `pivot`; pivot rows `created_on`, `plant`, count `serial_`, page filter `stat_2 = PAL`, tabular — **no `pivot` option, so no pivot as written** |

### 5.18 `add_del` (`A!131`-`A!142`) → folder `ABC` (only `A!135`-`A!136` carry `x` today)
| Row | Step |
|---|---|
| 131 | **VT11** variant `XD_ALL_EPDC_`, layout `btn_5_7`, date, `ignore_display_error`, F=`combine` → `T[vt11]` |
| 132 | `vt11` := rows with `transportplanningpt = TV38` (delete others) |
| 133 | `compare`: new last column `bruh` = `actualshipmentend − planning_end`. **`planning_end` is not in the `btn_5_7` layout** (section 7 item 20) |
| 134 | **VL06O** with `vt11.shipment_number`, layout `pallets` |
| 135 | **ZVT11 Closed** variant `ADD_DELETES`, layout `add_del`, `vl06o.delivery`, date ± 100 days → `T[zvt11]` |
| 136 | **format `zvt11` (step Z)**: sort by `delivery_number`; dedup by `delivery_number`; copy `shipment_end_date` into `date` (replace column values) then delete `shipment_end_date`; insert empty column `pallets` after `ship_tofrom_name`; rename `delivery_number` → `delivery`; clear formats |
| 137 | **VT22** with `zvt11.shipment_number`, variant `button`, layout `date_change`, compare `24hr` → `T[vt22]` (text with tabs). Post-process (`format_vt22`, rName contains `add_del`): the macro deletes columns by position; the headless runner should instead **parse the raw text directly** (section 9.2: one row per change record with `shipment_number, date, time, old_value, new_value`); sort by `shipment_number`; build `datetime` = `date + time` (inserted at column 4); sort by `shipment_number`, `datetime`; scan per shipment: when `old_value = 7` and `new_value < 7` start tracking at that `datetime`; when tracking and `old_value < 7` and `new_value = 7`, set `changes = YES` if the gap is over 24 hours, else `NO`; stop tracking. Raw layout is now documented (section 9.2) |
| 138 | **VT03N** with `zvt11.shipment_number` → write the "No. of pallets" text into column `pallets` (3.8) |
| 139 | **ZMDETPC** summarised with `zvt11.delivery`, variant `epdc_tony`, date; **merge into `zvt11`** (4.12, `add_del` rule: non-numeric/empty `pallets` → `1`) |
| 140 | `insert` column `split` before `pallets`, formula (A1, row 2 first data row): `IFERROR(IF(C2<>"", IF(COUNTIF($C$2:$C2, C2)=1, H2 − COUNTIF(C:C, C2) + 1, 1), ""), 1)`. With `C` = `shipment_number` and `H` = `pallets` after insertion: the first row of each shipment gets `pallets − (rows in shipment) + 1`, other rows get `1`, blank shipment → blank |
| 141 | **save** `data_sheet zvt11`, sheets `zvt11`, `vl06o`, `vt22`; pivot rows `date`, sum `pallets` — **no `pivot` option** |

### 5.19 Blocks present as tcodes but not as rows
`ZLEQTY` (item master per plant) and `mg_snapshot`/`Y_DN3_47000149`, `lx02/03/04`, `mb51/52`, `zmdeetsp`, `zmdepd`, `xd03`
are implemented in code but no row on `ACTIVE` uses them. Their IDs are in section 3.

---

## 6. Rust coverage (`C:\temp\repo\sap-automation`)

| tcode | Module | Status | Gap to close |
|---|---|---|---|
| VT11 | `vt11.rs`, `vt11_module.rs` | covers `K_DATEN`, `K_TPBEZ`, `S_VBELN`, `tabpTAB1`, menu `menu[3]/menu[0]/menu[1]`, `menu[0]/menu[10]/menu[0]`, local-file export | `K_STTRG`, `K_ERDAT`, shipment-list paste (`use vt11`), `monday_include_sunday`, `ignore_display_error` |
| ZVT11 | `zvt11.rs` | `ZSDR_SHIPMENT_REPORT`, `tabpTAB1` (Open), `S_VBELN1`, `K_DATEN` | **Closed** type: `tabpTAB2`, `ctxtS_DATE_2-LOW/HIGH` (±100 d), `btn%_S_VBELN2…`, `radR1`, `chkP_ALV_2`, ship point `ctxtS_VSTEL2-LOW` |
| VL06O | `vl06o.rs`, `vl06o_module.rs`, `vl06o_delivery_module.rs` | `BUTTON6`, `IT_VBELN`, `IT_TKNUM`, `IT_WADAT`, `LIKP-VBELN` | check item view (`btn[18]`) and layout menu path |
| ZMDESNR | `zmdesnr.rs` | tab number, delivery paste (`tabpUCOMM2/…:9002`), `S_PARENT`, bin `S_LGPLA`, `S_PALLTD`, pre-export back, layout | **user activity** (`9007`: `radP_USER`, `radP_PENSKE`, `chkP_ALV`, `S_DATE`, `S_ERDAT1`), by-date (`S_ERDAT`), TO paste (`S_TANUM`), serial tab (`9001`, `S_SERIAL`, `radP_HISTRY`/`CURRNT`/`OLDNEW`, `chkP_PALLET`), exclude tab `tabpNOSV`, `S_TKNUM`, `S_MBLNR` |
| LX03 | `lx03.rs` | yes | |
| Y_DN3_47000149 (149) | `y_149*.rs` | `S_WERKS`, `S_D_DATE`, `R_ALL`, `S_DELIV` | `R_IN_TRA`, `S_SIGNI` |
| **LT23** | none | **missing** | section 3.5 |
| **ZMDETPC** | none | **missing** | section 3.6 |
| **VT22** | none | **missing** | section 3.7 |
| **VT03N** | none | **missing** | section 3.8 |
| ZLEQTY, MB51, MB52, LX02, LX04, XD03, ZMDEETSP, ZMDEPD, VL02N, VL03N | none | missing (not used by `ACTIVE`) | section 3.9 |
| Layout helpers | `select_layout_utils.rs`, `setup_layout_utils.rs`, `setup_layout_li_utils.rs` | present | confirm the VT11/VL06O/LT23 variants |
| Export | `sap_export_utils.rs` | local file, `export_type` 0-4 (1 = text with tabs) | this is the natural TSV source |
| Variant / paste / popups | `sap_tcode_utils.rs`, `sap_wnd_utils.rs`, `sap_ctrl_utils.rs` | `variant_select`, `paste_values_with_scroll`, `check_export_window`, `exist_ctrl`, `hit_ctrl` | ALV-variant popup path (`abap` title) |

---

## 7. Problems found and open questions

### 7.0 Decisions made by the owner (2026-11-09)

| Item | Decision |
|---|---|
| 1 | **Reproduce as-is.** k2xx, rma, add_del and nc_masterpack produce **no pivot**. |
| 2 | Resolve dates with `get_json_date` semantics (single date, `range` by week or start/end, `diff`; section 1.1). The column value must come from the resolved date object, not the raw `E4` text. **Still open:** the exact value written for a multi-day range (proposal: per-day `date1` in loop mode, otherwise the `WK_<n>` label). |
| 11 | File name `<date>_<name>_<table>.tsv`, same folder layout as today. Pivots are emitted as **long-form group-by** tables (one row per group: key columns plus the sum/count). |
| 12, 14 | Owner will supply sample exports (text with tabs and Excel) for ZMDETPC, VT22 and ZMDESNR/VT11. Until then the ZMDETPC `total` columns and VT22 positional deletes stay unspecified. |
| 15 | relabel and repack read the delivery list from the **k2xx block's `vl06o` TSV of the same run**. |
| 18 | **Remove FV30 rows**, as coded. `CLAUDE.md` wording ("filtered to plant FV30") is wrong. |
| 19 | **Update `inb_rs.exe` to read TSV**; the runner passes the TSV path. |
| 25 | Strip leading zeros from numeric-looking identifiers in **every** table (as today); apply the same normalisation to values pasted back into SAP. |
| 26 | Dates `yyyy-mm-dd`, times `hh:mm:ss`, datetimes `yyyy-mm-dd hh:mm:ss`, `descripof_shipment` as raw text. Numbers without a trailing `.0`. |
| 27 | Owner supplied raw exports and per-rName zips (`mustgo`, `add_del`); no TSV exists yet (the sap-scripting project handles TSV). |
| 8 | **Resolved**: the Rust runner already implements `pre_export_back` (press Back once before the layout step). |
| 9 | **Fix, implement the intent.** `remove_one` with `delete_row` removes the **first** row (in sorted order) of **every** distinct `storage_bin`, including the last bin. Sort `main` by `storage_bin` ascending (case-insensitive, stable); rows within a bin keep their SAP order. For the 2026 week-45 sample this gives 1218 rows instead of the macro's 1219. Used only by `nc_tsp` `A!79`. |
| 15 | `del_exp` rows without `x` are skipped, so tables persist across blocks in one run (relabel reads k2xx's `vl06o`, repack reads `vl06o` and relabel's `combined`). The runner keeps tables for the whole run unless `del_exp` is selected. |
| 33 | **Keep headless tables clean**: no temporary `GM55` marks in `sn_del` / `repack_sn` outputs and no extra last row. `GM55` is an internal flag only; the TSV tables always show the real plant. |
| 34 | **Keep the observed order**: `combined` / `snr_repack` stay in block order, each block in SAP row order; no sorting. |
| 35 | **Explicit data instead of colour**: the history hit is carried as an explicit boolean column or list (not a fill colour); it drives the 4-label rule and the repack exclusion list. |
| 28 | **Fix**: do not drop the last VT22 record. All records are kept before the `changes` logic runs. *Fixed in the VBA (commit `948e6e4`).* Root cause: after the positional column deletes the VT22 sheet still holds the detail lines as junk rows that sort below the records, and `delete_extra_rows(ws,"C")` deleted from the last record row to the end instead of from the row below it. |
| 29 | *Fixed in the VBA (commit `948e6e4`): sort by `delivery_number`, `plant` before the dedup. The old `sort_multi_fields(ws,"delivery_number")` call sorted nothing.* **Accurate plants required.** ZVT11 dedup by `delivery_number` must not keep a row with a blank `plant` when another row for the same delivery has one: prefer the first row with a non-blank `plant` (else the first row). |
| 30 | **Fix**: `date` always comes from `shipment_end_date` of the kept row; no row may lose its date. *Fixed in the VBA (commit `948e6e4`) with a position-based copy; the exact cause of the one lost date was not found.* |
| 31 | **As is**: exports without a layout use SAP's default layout; read columns by sanitised header name. |
| 32 | **OK**: send each delivery/TO/shipment to SAP once (distinct values, no header, no blanks). |

Items not listed above are still open.


Numbered so you can answer by number. Each item names the file/line it came from.

1. **Pivot is not produced in 5 saves.** `save` builds a pivot only if `option` or `F` contains `pivot`
   (`sequence_.bas:2497`). `A!58` (k2xx), `A!120` (rma), `A!141` (add_del), `A!152` (nc_masterpack) define pivot
   arguments but no `pivot` option. `A!62` (relabel `full_label`) is intentionally pivot-less. *Q: should these emit a
   pivot/aggregate table?*
2. **`value_dyn:"date"` writes the raw `E4` cell**. With `E4` = week-range JSON, the new column (`date`,
   `export_date`) receives the JSON text. *Q: for a range run, what should `date` / `export_date` contain (the week
   start, `WK_31`, the per-day date in loop mode)?* Relabel/repack also assume a plain date in `E4` (item 16).
3. **`Columns.Replace "0","1"`** (xdock/toyota total, `sequence_.bas` merge) is a *substring* replace in Excel
   (xlPart default) so `10` could become `11`. *Q: intended "cells equal to 0 → 1"?*
4. **Header row is pasted into SAP with the list.** `set_clipboard` copies the visible cells of the column including
   row 1 and a trailing blank. SAP shows warnings, which the code clicks through. The headless runner should send data
   values only. Confirm that no step relies on the header being sent.
5. **`dedup` `"option":"visible"` (`A!15`) is ignored**; the dedup runs as a plain whole-column dedup. Excel's
   RemoveDuplicates on a filtered range may only consider visible rows. *Q: is "first parent per visible row only" the
   intent (H says "dedup parent visible")?*
6. **`filterby:"created"` (`A!114`) has no effect with the current `E4`**: `get_json_date` only keeps `created` in
   `date_str` for `type:"diff"`. With `E4` as week range, VT11 filters by `K_DATEN` (actual end). *Q: should RMA select
   by creation date (`K_ERDAT`)?*
7. **`date_range` (`A!56`=7, `A!119`=3) and `layout:"base"` on ZMDETPC are never read** by the summarised flow. *Q: drop them?*
8. **`pre_export_back`**: Back (`sendVKey 3`) is sent before the layout step for `nc_*`, `redtag`; the reason (the user
   activity "report" mode opens a second screen) is not documented in code. Rust has `--pre-export-back`.
   *Q: confirm it is still needed.*
9. **CONFIRMED by `nc_tsp` run (section 9.4). Used only by `nc_tsp` `A!79`.** Original text: **`remove_one delete_row` looks off by one** (`Functions_.bas:831`). The routine collects array indexes of the first
   occurrence of each value, skips index 1, and deletes `ws.Rows(index)` although the array starts on sheet row 2. *Q: spec
   as intended ("one fewer row per distinct `storage_bin`") or reproduce current behaviour?*
10. **Steps whose output is never used:** `mustgo` `Filter_Copy_MG` history (`mustgo_sn_hist`, `mustgo_filtered`);
    `xdock` ZVT11 (`A!38`). *Q: drop them in the headless version?*
11. **TSV shape decisions needed:** file naming (`<date>_<name>_<table>.tsv`?), pivot form (cross-tab as Excel shows it
    or long form), whether to keep helper tables (`orig`, `snr_orig`, `to_orig`), and whether workbook "sheet order" matters.
12. **Export format.** The macro uses SAP's Spreadsheet export (xlsx). Rust's `export_local_file` writes text-with-tabs.
    Header row, date/time formats (`MM/DD/YYYY`, `hh:mm:ss`), thousands separators, trailing blank column and the
    `ZMDESNR`/`VT11` header text may differ. *Q: can you supply one sample of each export for comparison?*
13. ~~**`mustgo` `A!20`**~~ **Resolved by samples (section 9).** Original text: **`mustgo` `A!20`**: the formula `WEEKNUM(RC[-1])` at column 19 refers to column 18 of `snr_copy`. In the documented
    `mg_view` layout that is `Time`, not a date. *Q: which column is the date there?*
14. **ZMDETPC schema and VT22 raw text resolved (section 9).** Original text: **No schema for ZMDETPC summarised export or VT22 raw text.** `total = SUM(RC[-2]:RC[-1])` and the positional
    column deletes in `format_vt22` depend on them. *Q: please provide a sample of each.*
15. **RESOLVED (section 9.5): `del_exp` rows without `x` are skipped, so tables persist across blocks.** Original text: **Cross-block data dependencies.** `del_exp` clears all tables between blocks, but `relabel` (`A!61`) uses `vl06o`
    and `repack` (`A!68`) uses `vl06o` and `combined`. In the workbook this only works if those tables survive
    (the stale `ADD_DEL_VL06O` sheet also matches "vl06o" by substring). *Q: where should relabel/repack get their
    deliveries from — the k2xx block output, a file, or an explicit input list?*
16. **CONFIRMED (section 9.5): run relabel per single date `D` (loop mode).** Original text: **Relabel needs a plain date.** `Relabel_PreFilter` reads `rDefaultDate.value` with `Format(.., "dddd")` and filters
    `document_date` between `D` and `D+1`. That requires `E4` to be a single date (loop mode); week-range JSON breaks it.
17. **RESOLVED (section 9.5).** Original text: **Relabel label counts.** The label-quantity step (`set_item_qty`) is Find-based and may also write to hidden rows.
    *Q: confirm the expected number of labels per row (4 for FV55 non-kanban pallets, 2 otherwise).*
18. **`FV30` filters.** `A!86`, `A!93`, `A!99`, `A!105` use `filter_val "<>FV30"` with `delete`, which *removes* FV30 rows.
    `CLAUDE.md` says "filtered to plant FV30". *Q: which is right?*
19. **`inb_rs.exe` consumes the xlsx** saved at `A!87`. If TSV replaces xlsx, either keep an xlsx for that step or update
    `inb_rs`. *Q: which?*
20. **`A!133` compare** references column `planning_end`, which the `btn_5_7` layout does not contain; the output column
    name defaults to `bruh`.
21. **`u_activity` and `busbar` layouts have no column list in `LAYOUTS`**, so they cannot be auto-created and must exist in SAP.
22. **Hidden-row semantics.** Several steps rely on rows that are hidden but not deleted (`A!10`, `A!13`, `A!14`).
    The selection mask in section 4 reproduces that; materialising early would change which rows `A!11` and `A!16` send to SAP.
23. **Layout `to_mg` and the `mustgo` post-process (5.1 row 16)** assume column names `creation_date`, `creation_time`,
    `delivery`, `source_storage_bin`, `transfer_order_number`.
24. **Free-text `F`.** Words in comments (`A!34` `// orig (xd_all_epdc)`, `A!75`, `A!79`) are matched by `Contains` in `F`. They
    were checked to have no effect today, but a rewrite should treat `F` as structured flags.

---

## 8. Reference data

### 8.1 Named filters used by `ACTIVE`
(row = AND, `OR` between rows; `(blank)` = empty cell)

| Name | Criteria |
|---|---|
| `mg_vt11_filter_1` | `transportplanningpt` = `TPDC` |
| `inb_tsp_filter_1` | `parent_serial_number` = (blank) |
| `tsp_zmdesnr_filter_a` | `status` = TSP AND `warehouse_number` = E01 AND `container` <> KC* AND <> C0028934* AND <> C0027948* AND <> C0029854* AND <> C0014595* |
| `tsp_zmdesnr_filter_b` | `parent_serial_number` = (blank) AND `pallet` <> 1 |
| `nc_mp_zmdesnr_filter_tsp` | `pallet` <> 1 AND `plant` <> FV30 AND `parent_serial_number` = (blank) AND `status` = TSP AND `gross_weight` > 60 AND `warehouse_number` = E01 AND `container` <> KC* |
| `nc_mp_zmdesnr_filter_alt` | same as above with `status` = ASH |
| `nc_mp_zmdesnr_filter_2` | `status` = PAL AND `warehouse_number` = E01 |
| `redtag_zmdesnr_filter` | (`status`=ASH AND `palletized`=(blank) AND `created_by` = X) OR … for X in `KJPP8Z`, `SZNCSS`, `TJ7B7L`, `NZ4NJH` |
| `busbar_zmdesnr_filter_1` | `status` = SHP AND `parent_serial_number` <> (non-blank) |
| `busbar_zmdesnr_filter_2` | `parent_serial_number` <> (non-blank) |
| `rework_relabel_gm55` | `plant` = FV55 AND `customer` = LSI* OR GM* OR GENERAL* (three rows) |
| `k4xx_relabel_filter_2` | `plant`=FV55 AND `kanban_number`=(blank) AND `container`<>KC* AND `pallet`=1, OR the same with `pallet` = 2 |
| `repack_filter_kc` | `container` <> KC* AND <> SC* |

Other names on `FILTERS` (not referenced by `ACTIVE` rows): `relabel_filter_reset`, `dispatch_filter_1/2`,
`addon_filter_scac`, `same_day_filter_1`, `knock-offs_zmdesnr_filter`, `mustgo_lt23_filter`, `abc_vt11_filter_1/_pre`,
`mixed_snr_filter_2`, `mg_snr_filter_1`, `ob_*`, `pending_trl_filter`, `k2xx_*`, `k3xx_*`, `k4xx_*`, `k5xx_*`, `disc_lx03_filter`.
The full list is in the workbook.

### 8.2 Layouts referenced by `ACTIVE` (column order)
`LAYOUTS` row 2+ under each `<name>_<tcode>_layoutrows` header.

* `scorecard_vt11`: Shipment Number, Ov.transport status, Shipment type, TransportPlanningPt, Seal Number 2, Shipping type, Service agent, Trailer Number, Seal Number 1, ActualShipmentEnd, Act.shipmnt end time, Descrip.of Shipment, Forwarding agent tracking ID.
* `btn_5_7_vt11`: Shipment Number, TransportPlanningPt, Ov.transport status, Act.load.start time, Service agent, Seal Number 2, Trailer Number, ActShpmtCmpletDate, ActTranspProcTime, ActualShipmentEnd, Act.shipmnt end time.
* `xdock_vl06o`, `inv_vl06o`, `pallets_vl06o`, `rma_1_vl06o`, `mg_view_vl06o`: section 3.3.
* `mg_view_zmdesnr`: section 3.4.
* `epdc_relabel_zmdesnr`: LM Serial Number, Status, Serial number 2'nd party verification, SPV Scan copies, Pallet, Parent Serial Number, Material, Customer Part #, Delivery, Plant, Storage Location, Warehouse Number, Document Date, Created on, Created by, Plant Doc line, DLOC, Kanban Number, Container.
* `epdc_repack_zmdesnr`: LM Serial Number, Status, Pallet, Parent Serial Number, Material, Customer Part #, Delivery, Plant, Storage Location, Warehouse Number, Created by, Created on, Net Weight, Container.
* `nc_mp_zmdesnr`: Serial #, Pallet, Parent serial number, Storage Location, Status, Customer Name, Storage Bin, Material Number, Qty, Part Description, Created on, Delivery, Plant, Warehouse Number, Gross Weight, Container.
* `rtg_zmdesnr`: Serial #, Pallet, Parent serial number, Storage Location, Status, Customer Name, Storage Bin, Material Number, Qty, Part Description, Created on, Time, Delivery, Plant, Warehouse Number, Created by, Palletized, Shipment Number.
* `to_mg_lt23`: section 3.5.
* `add_del_zvt11`, `rma_1_zvt11`: Date, Trailer Number, Shipment Number, Delivery Number, Plant, Ship To/From Name, Shipment End date.
* `u_activity_zmdesnr`, `busbar_zmdesnr`, `base_zmdetpc`, `date_change_vt22`, `button` (VT22 variant): **not defined in `LAYOUTS`** (SAP-side saved layouts).

### 8.3 Source map
`start_loop` `sequence_.bas:4272`; `run_tcode_w_variant` `:3274`; `run_tcode_n_variant` `:2755`; `run_misc_op` `:1608`;
`save` `:2403`; ZMDETPC `Ops_.bas:4840`; ZVT11 `:5020`; VT11 `:2912`; VL06O `:2619`; LT23 `:641`; ZMDESNR `:4515`;
`sn_from_to_run` `:4008`; `sn_from_del_run` `:1900`; VT22 `:3106`; VT03N `:2770`; layout/export/variant helpers
`Control_id_.bas:134-1773`; dates `Functions_.bas:3417`; names `:3837`; filters `Relabel_Filters_.bas`, `Repack_Filters_.bas`, `MG_Filters_.bas`;
`vt22_format_.bas`.

---

## 9. Verification against sample files (`abc/resources/`)

Files supplied 2026-11-09: `06-11-2026_ADD_DEL.xlsx`, `06-12-2026_K2XX.xlsx`, `06-12-2026_MUSTGO.xlsx`,
`06-12-2026_RELABEL.xlsx`, `06-12-2026_REPACK.xlsx`, `07-02-2026_XDOCK.xlsx`. They are **post-processed xlsx outputs**, not raw
SAP exports, so item 12 (text-with-tabs vs spreadsheet differences) is still open.

**Confirmed (the spec matches the samples)**
* **ZMDETPC summarised schema**: `delivery`, `pallet`, `child`. After the merge the table ends `..., delivery_, pallet, child, total`
  with `total = pallet + child` (verified on every row of xdock and k2xx). The merge is a sorted outer join: both samples are
  sorted ascending by `delivery`, have no duplicate deliveries, and every `delivery` equals `delivery_` (no spacer rows were needed).
* **mustgo `lt23` post-process**: columns `bdup, delivery, transfer_order_number, creation_date, creation_time, datetime, user, plant,
  material, source_storage_type, source_storage_bin, source_target_qty, confirmation_status`. `bdup` keeps the original TO count.
  Rows are sorted `delivery` desc, `datetime` desc. A delivery with 2 TOs keeps 1 row and one with 3 TOs keeps 2, so the **earliest
  TO is the row removed** (matches 5.1 row 16(f)). `bdup` is not deleted.
* **mustgo `snr_copy`**: 21 columns ending `..., created_on, week, time, export_date`. `week` = week number of `created_on`
  (June 12 → 24). Parent serials are unique.
* **VT22 post-processed `add_del`** (`shipment_number, date, time, datetime, old_value, new_value, changes`): the 24 h rule
  reproduces exactly. Example: `24126693` reopened `7→4` on 06-09 15:10 and closed `6→7` on 06-11 20:25 (53 h) = `YES`;
  `24130734` reopened 06-10 13:12 and closed 06-11 08:48 (19.6 h) = `NO`. `changes` is set only on the closing row.
* **k2xx** has sheets `VL06O`, `VT11`, `ZVT11` and **no pivot** (matches item 1 decision). Its `ZVT11` is the unformatted export
  (`shipment_end_date`, no `pallets` column).
* **Pivots**: relabel = `customer` × `plant`, sum `_of_labels`; repack = `customer` × `plant`, count; mustgo = `plant` × `export_date`,
  count; xdock = `plant`, sum `total`. Rows/columns sorted ascending, blank labels omitted, grand totals present. Long-form output
  (item 11 decision) must keep that sort and decide whether to include totals rows.
* `export_date` and `date` hold a **date value** (per-day, 2026-06-12 / 2026-07-02), i.e. these samples came from single-date or per-day loop runs.
* Header blank fill: `customer` appears in mustgo `sn_to` (column 14), relabel and repack, as documented.

**New facts**
* The **real SAP layouts differ from `LAYOUTS`**. `mg_view_zmdesnr` in SAP has `customer` (14th) and `created_on` (18th) and `time`
  (19th), but `LAYOUTS` lists no `Created on`. Treat `LAYOUTS` as a fallback list, not the truth. Layout selection by name in SAP is what matters.
* `repack` writes its ZMDESNR export as `REPACK_SN_EXCL` (name `<rName>_sn_excl`); `ACTIVE`'s `repack_sn` is a substring match on it.
  relabel's `RELABEL_SN_DEL` has 1 row more than `snr_orig`.
* VT11 `descripof_shipment` is text like `06/12/2026 1000` but Excel turned some values into dates (xdock). A TSV runner should keep it as text.
* In `add_del`, `pallets` is `1` on every row and `split` is empty: the sample predates VT03N and the `split` formula, consistent with
  the "non-numeric/empty → 1" rule (4.12).
* The `0 → 1` replace (item 3) could not be tested: no sample row has `total = 0`, and no formula was corrupted (`=SUM(J10:K10)` is intact).

**Still needed from you**: raw (unprocessed) text-with-tabs exports for ZMDETPC, VT22 and ZMDESNR/VT11 so header names, number/date
formats and extra columns can be compared with these xlsx outputs.

### 9.2 Raw SAP exports (`abc/resources/2026110910*`, `20261109090447_zmdesnr.xlsx`)

The four raw files were exported on 2026-11-09 and are **not correlated** with each other or with the six processed files.
They are the files SAP saves into `%UserProfile%\Documents\SAP\SAP GUI\<yyyymmddhhmmss>_<tcode>.<ext>` before the macro imports them,
and they stay on disk after a run. The ZMDESNR file has the 20 `epdc_relabel` columns, so it came from a deliveries-based ZMDESNR run.

**VT22 raw text (`..._vt22.txt`, text with tabs, CRLF, SAP date format ISO)**
* Lines 1-5 are a header: `Changes in<TAB><TAB><TAB>Shipment Header`, `Field:<TAB><TAB>Overall transportation status`, a blank line,
  then two column-title lines. Skip them.
* After that, each change is **three lines**: a *record* line, a *detail* line, and a blank line.
* Record line, split on tab (0-based): `1` shipment number (10 digits, zero padded, e.g. `0024515316`), `4` document number, `5` user,
  `7` date (`yyyy-mm-dd`), `9` time (`hh:mm:ss`), `10` transaction (`VT02N`), `12` language, `14` `000`.
* Detail line: `1` table (`VTTK`), `4` table description, `6` field (`STTRG`), `8` field description, `11` change indicator (`U`),
  `13` **old value**, `15` **new value**.
* The sample has 641 records for 157 shipments, all `STTRG`/`U`. Old/new values are single digits (`3→7`, `7→4`, `6→7`, ...).
* Output of the VT22 step: one row per record with `shipment_number` (zero padding removed), `date`, `time`, `old_value`, `new_value`;
  then the `24hr` `changes` logic (5.18 row 137) with `datetime = date + time`.

**ZMDESNR raw (`..._zmdesnr.xlsx`, sheet `Sheet1`, 3270 data rows, 20 columns)**
* Headers are the raw SAP titles: `LM Serial Number`, `Status`, `Serial number 2'nd party verification`, `SPV Scan copies`, `Pallet`,
  `Parent Serial Number`, `Material`, `Customer Part #`, `Delivery`, *(blank)*, `Plant`, `Storage Location`, `Warehouse Number`,
  `Document Date`, `Created On`, `Created By`, `Plant Doc line`, `DLOC`, `Kanban Number`, `Container`.
  The **blank-header column 10 holds the customer name** (→ `customer`). `Kanban Number` is entirely empty but has a header and is kept.
* Every identifier is **text with leading zeros**: serial `0372781038`, storage location `0011`/`0004`, parent `0371103016`.
  Empty cells are empty strings. `Document Date` and `Created On` are real dates. `Status` is empty on 4 rows.

**VT11 raw (`..._vt11.xlsx`, 58 data rows)**
* Column A is an empty column with an empty header (dropped on import). Then the 13 `scorecard` headers.
* `Shipment Number`, `Ov.transport status` and seals are text; `ActualShipmentEnd` is a date; `Act.shipmnt end time` is a time of day.
* `Descrip.of Shipment` is text and **not uniformly formatted**: `10/19/2026 1500`, and one row `10/29/20261800` (no space). Keep as text.

**ZMDETPC raw (`..._zmdetpc.xlsx`, 257 data rows, 3 columns)**: `Delivery` (text, 10 digits, zero padded, e.g. `0085560436`), `Pallet`, `Child`
(numbers). No totals row. Deliveries are unique. Matches the merged `delivery_, pallet, child` block in section 9.1.

**Type rule the macro applies to every imported table (`ConValue`: `Cells.Value = Cells.Value`)**
Text that Excel would parse as a number becomes a number. **Leading zeros are lost**: `0372781038` → `372781038`, `0004` → `4`, `0085560436`
→ `85560436`. Text that does not parse stays text (`C0031714`, `32753T6Z AA00`). The processed samples confirm this
(`storage_location` = `4`, deliveries as `80021856`). Cross-table joins work only because every table is normalised the same way
(for example ZMDETPC `0085620728` matches VL06O `85620728`). The headless runner must apply **one** normalisation to all tables
(strip leading zeros from numeric-looking identifiers) or none, never a mixture; and it must normalise values it pastes back into SAP lists.
Dates and times: sheet formats are `yyyy-mm-dd`-style for `*date*`/`created on`/`actualshipmentend`/`planning_end`, `h:mm:ss AM/PM` for
`*time*`, `MM/DD/YYYY h:mm:ss AM/PM` for `datetime`, `MM/DD/YYYY hhmm` for `descripof_shipment`/`scheduled_date`.

**New open questions**
25. **Leading zeros in TSV.** Keep today's behaviour (strip zeros everywhere) or preserve SAP's padded text? Stripping matches the current xlsx
    outputs and `inb_rs.exe`; preserving is lossless but must be applied to every table and every join.
26. **Date/time text in TSV.** Proposal: dates `yyyy-mm-dd`, times `hh:mm:ss`, datetime `yyyy-mm-dd hh:mm:ss`, shipment description as raw text.
27. **Samples for the remaining gaps.** Text-with-tabs versions (the VT22 export is the only one so far) of ZMDESNR, VT11, ZMDETPC, VL06O,
    ZVT11 and LT23 from the same query as an xlsx export, to diff headers and formats.

### 9.3 Full-run verification: `mustgo` (1 day) and `add_del` (1 week), from `resources/mustgo.zip` and `resources/add_del.zip`

Each zip holds the raw SAP exports (`<yyyymmddhhmmss>_<tcode>.xlsx|txt`) and the final macro output. I re-implemented the documented steps on the raw
files and compared the results with the macro output. **Numbers below are exact matches unless stated.** TSV does not exist yet; the future
runner reads and writes TSV, so only the xlsx/txt above are available as reference.

**`mustgo`, run date 2026-11-05 (output `11-05-2026_MUSTGO.xlsx`)**

| Step | Raw file → result | Check |
|---|---|---|
| 9 VT11 | `..122601_vt11.xlsx`, 91 rows, **18 columns** (no layout, so SAP's default: Shipment Number, Shipment type, TransportPlanningPt, Shpmt compl. type, Service Level, Shipping type, PrelLegShipType, SubsequntLegShipType, Leg indicator, Shipping Conditions, Shipment route, Trailer Number, Seal Number 1/2, Descrip.of Shipment, Ov.transport status, Service agent) | `TPDC` = 54 shipments |
| 11 VL06O | `..122612_vl06o.xlsx`, 834 rows, **10 columns** incl. `Item` (item view, default layout): Delivery, Item, Ship-to party, Material, Number of packages, Delivery quantity, Deliv. date(From/to), Sales Unit, Total Weight, Unit of Weight | 107 distinct deliveries |
| 12 ZMDESNR by deliveries | `..122648_zmdesnr.xlsx`, 5796 rows, 19 columns (`mg_view`: blank-header column 14 = customer) | statuses SHP 5546, PAL 228, SPV 22; 64 blank parents |
| 13-15 | parent non-blank AND status ≠ PAL → 5504 rows; dedup by parent (first kept) → **1011 rows, 87 distinct TOs** | the 87 TOs equal the TOs in the LT23 export exactly |
| 16 LT23 | `..122744_lt23.xlsx`, 1071 rows, 11 columns (`to_mg`) → dedup TO (87) → delete deliveries with a single TO (25 rows left) → drop the earliest TO per delivery | **15 rows, same values and same order** as sheet `MUSTGO_LT23` |
| 17-18 | keep `sn_del` rows whose TO is among the 15 TOs → 476 rows, 10 deliveries | |
| 19 ZMDESNR by deliveries + TOs | `..122816_zmdesnr.xlsx`, 476 rows | identical to sheet `MUSTGO_SN_TO`; every row of `sn_del` with delivery and TO in the lists is returned (476 = 476) |
| 19 history (`Filter_Copy_MG`) | `..122844_zmdesnr.xlsx`, 2108 rows | **not in the output** (confirms item 10: this SAP call is wasted) |
| 20-21 `snr_copy` | `sn_to` rows, parent non-blank deduped by parent (first kept) **with blank-parent rows kept in place** → 92 rows (89 distinct parents + 3 blank) | same 92 rows, same order as the source (not re-sorted) |
| `week` | `WEEKNUM(created_on)` with Excel's default (week 1 contains Jan 1, weeks start on **Sunday**): 2026-11-04 and 11-05 → **45** | |
| `export_date` | `2026-11-05` (the per-day run date) | |
| 22 pivot | `plant` × `export_date`, count `lm_serial_number`, with totals | |

Number text: `Source target qty` is `144.0` in SAP and `144` after import; `time` is a time of day (`0.84376…` in the xlsx = `20:15:01`).
Write numbers without a trailing `.0`.

**`add_del`, week 45 of 2026 (Sunday 2026-11-01 → Saturday 2026-11-07; output `WK_45_ADD_DEL.xlsx`)**
This run did **not** do `A!132` (no TV38 filter: the VT11 export has 11 planning points) and **did not run VT03N or the `split` formula**.
Treat it as the flow `A!131, 134, 135, 136, 137, 139, 141`.

| Step | Raw file → result |
|---|---|
| 131 VT11 | `..122028_vt11.xlsx`, 513 shipments, all status 7, 11 columns (`btn_5_7`, blank column A) |
| 134 VL06O | `..122047_vl06o.xlsx`, 6381 **item rows**, 8 columns (`pallets`), **889 distinct deliveries**; output sheet `ADD_DEL_VL06O_` keeps all 6381 rows (no dedup) |
| 135 ZVT11 Closed | `..122332_zvt11.xlsx`, 3161 raw rows for the 889 deliveries and 513 shipments; all 889 deliveries are in the VL06O list, all shipments are in the VT11 list |
| 136 format `zvt11` | sort by `delivery_number` ascending; dedup by `delivery_number`, **first row kept** (889 rows); `date` := `shipment_end_date`; blank header column 1 renamed `date`; delete `shipment_end_date`; add `pallets`; rename `delivery`. Output sorted by delivery as numbers. |
| 137 VT22 | `..122403_vt22.txt`, 2325 records → output sheet `ADD_DEL_VT22_11-01-2026` (sheet name = `ADD_DEL_VT22_` + start date `MM-DD-YYYY`), 2324 rows, 512 shipments; `changes` = YES 100, NO 140, blank 2084. **My `changes` algorithm (5.18 row 137) reproduces all 2324 values.** Output is sorted by `shipment_number`, `datetime`. |
| 139 ZMDETPC | `..122455_zmdetpc.xlsx`, 869 rows for 889 deliveries. 20 deliveries have no ZMDETPC row: in the merged table `delivery_`, `pallet`, `child` **and `total` are all blank** for those rows (not 0). Includes 10-digit deliveries (`8012734818`). |
| `pallets` | `1` on all 889 rows (empty → 1 rule); `split` column present but empty |

**New findings (open questions 28-32)**
28. **The last VT22 record is lost.** The raw file has 2325 records, the output 2324. The missing record is the one with the largest shipment number
    (`24631451`, 2026-11-07 12:37:22, `6→7`, which can be a closing row). Cause: `format_vt22` sorts by `shipment_number` and then calls
    `delete_extra_rows(ws,"C")`, which deletes `lastRow..lastRow`. *Q: reproduce (drop the last record) or fix?*
29. **ZVT11 dedup keeps the first row, even if it has blank fields.** Deliveries `85614255` and `85614256` have four raw rows, three with
    blank `plant` and one with `FV58`; the first (blank) row is kept, so 7 output rows have no plant. *Q: keep the first row, or prefer the row with a plant?*
30. **One output row lost its date.** Delivery `85666620` (shipment `24631370`, raw end date 2026-11-07) has an empty `date`. Cause not found.
    The row is the last 8-digit delivery before the 10-digit ones. *Q: reproduce or fix (always take the date from `shipment_end_date`)?*
31. **Exports without a layout (`mustgo` steps 9 and 11) depend on the SAP default layout**, so the VT11 and VL06O column sets vary
    (18 and 10 columns above). Downstream steps only use `transportplanningpt`, `shipment_number`, `delivery` (VL06O: `delivery`). *Q: pin a layout
    or keep as-is and read columns by header name?* The runner must match columns by sanitised header name, never by position.
32. **Item-view VL06O has many rows per delivery.** `mustgo` and `add_del` pass the full column to SAP and SAP removes duplicates; the headless
    runner should send each delivery once.

### 9.4 Full-run verification: `nc_tsp` (week 45, `resources/nc_tsp.zip`)

Files: raw `..124913_zmdesnr.xlsx` (63183 data rows, 16 columns, header `Serial #, Pallet, Parent serial number, Storage Location, Status,
Customer Name, Storage Bin, Material Number, Qty, Part Description, Created on, Delivery, Plant, Warehouse Number, Gross Weight, Container`)
and output `WK_45_NC_TSP.xlsx` (sheets `NC_TSP_tsp`, `pivot`, `orig`, `main`). The `ACTIVE` rows used are `A!74`-`A!80` exactly as in section 5.8.

* **Raw first data row is a totals row**: every field empty except `Qty` = `1125019`. It is removed by the first filter (`status` is blank). Any
  runner that reads the raw file must tolerate it.
* `tsp_zmdesnr_filter_a` → **45855 rows = sheet `orig`** (exact). `tsp_zmdesnr_filter_b` on that → **1508 rows = sheet `NC_TSP_tsp`**, same order as the source (exact).
* **`remove_one` result (sheet `main`) is 1219 rows**, not 1218. I implemented the VBA literally (sort by `storage_bin` ascending, stable; take the
  array index of the first row of each bin; skip index 1; delete sheet row = index) and it reproduces `main` **exactly (same rows, same order)**.
  The intended rule ("remove the first row of each bin") gives 1218 rows.
* **Behaviour of the bug (confirmed):** of 290 distinct bins, 289 lose **their last row** and the alphabetically last bin (`BLDG-D-MG`, 4 rows) loses nothing.
  The intended rule removes the first row of every bin including the last. 89 bins have a single row (they disappear either way).
  201 bins have several rows; in each, the macro keeps the first rows and removes the last row, the intended rule the opposite, so 201 rows differ plus the extra
  row from the last bin.
* **Effect on the pivot** (`plant` count of `main`): macro `FV33 1, FV34 241, FV50 172, FV52 32, FV55 669, FV56 71, FV58 7, FV59 26 = 1219`;
  intended `FV50 169, FV55 668, FV58 8, FV59 28`, other plants unchanged `= 1218`. The differences exist because bins can hold serials from different plants.
* `main` is **sorted by `storage_bin`** as a side effect of `remove_one` (case-insensitive ascending); `orig` and `tsp` keep the SAP order.
* Numbers: `Material Number` `000000000036034422` → `36034422`; `Qty` `16.0` → `16`; `Storage Location` `0004` → `4`; `Serial #` zero-stripped.

Open question 9 is **decided: implement the intent** (section 7.0). Expected output for this sample: 1218 rows, pivot `FV33 1, FV34 241, FV50 169, FV52 32, FV55 668, FV56 71, FV58 8, FV59 28`.

**Note on item 29:** 5 of the 7 blank plants in the week-45 sample cannot be fixed from ZVT11 (every ZVT11 row for those deliveries has a blank plant, and
VL06O has none either). Only deliveries `85614255` and `85614256` are fixed by preferring the row with a plant. Deliveries `85603116`, `85617365`,
`85628836`, `85640568` and `85660537` stay blank.

### 9.5 Full-run verification: `k2xx` → `relabel` → `repack` (2026-11-05, `resources/k2_relabel_repack.zip`)

The three blocks ran in sequence with the `ACTIVE` grid pasted by the owner. **`del_exp` rows had no `x`, so they were skipped and every table stayed in
the workbook**: `relabel` read `vl06o` from `k2xx`, `repack` read `vl06o` and `combined`. `A!62` (relabel `save full_label`) was also not ticked.
Raw files: `..132255_vt11`, `..132316_vl06o`, `..132522_zvt11`, `..132542_zmdetpc` (k2xx); `..132627_zmdesnr` (relabel deliveries, 6939 rows),
`..132659-_ZMDESNR` (relabel history, 5 rows); `..132815_zmdesnr` (repack, 5839 rows), `..132844-_ZMDESNR` (repack history, 380 rows). Note the history
exports are named `<time>-_ZMDESNR.xlsx` (different from `<time>_zmdesnr.xlsx`).
**I re-implemented the steps on the raw files. Rows, columns, order and totals below match the macro output exactly.**

**`k2xx`** (`11-05-2026_K2XX.xlsx`: `K2XX_VL06O`, `K2XX_VT11_11-05-2026`, `K2XX_ZVT11`; no pivot)
* VL06O raw 840 item rows, 107 distinct deliveries. Output: dedup by `delivery` (**first row kept**), sorted by `delivery` ascending, then `date` (= run date),
  `delivery_`, `pallet`, `child`, `total`. ZMDETPC has 106 rows; the one delivery without a row (`8012734818`, 10 digits) has blank `delivery_`, `pallet`, `child`, `total`.
* `K2XX_ZVT11` is the **unformatted** ZVT11 export (409 rows, 7 columns; header ` Date` becomes `date`, so **headers are trimmed before sanitising**).

**`relabel` → `combined`** (D = 2026-11-05, window `document_date` from D to D+1 inclusive). Work on the delivery export `F` (6939 rows, original order):
1. Rows with `plant = FV55` and `customer` starting `LSI`, `GM` or `GENERAL` get `plant = GM55` **temporarily, for all matching rows**.
2. Build `combined` by appending, **each block in the original row order of `F`, no sorting** (the macro's `Multi_Sort` calls do not change the order):
   * **B1**: `plant` ≠ FV55 and not blank, `spv_scan_copies = 2`, `document_date` in the window: 87 rows.
   * **B2**: `plant` ≠ FV55 and not blank, `serial_number_2nd_party_verification` blank, `document_date` in the window: 262 rows.
   * **B3**: `plant` ≠ FV55 and not blank, `status = PAL`, one row per `parent_serial_number` (first kept): 48 rows.
   * **B4** (not appended): `plant = FV55` (after step 1) and `pallet = 1`: 5 rows. Their serials go to **ZMDESNR by serial / History**; the 5 history rows
     returned (`status = PRI`, `storage_location = 11`) mark those 5 serials **yellow**.
   * **B5**: `pallet` in {1,2} and `plant = FV55`: 9 rows. **B6**: `plant = FV55`, `pallet` blank, `parent_serial_number` blank: 15 rows.
     **B7**: `parent_serial_number` non-blank and `kanban_number` non-blank and not starting `0`: 0 rows in this data (kanban is always blank), so unverified.
   * Order of `combined` = B1, B2, B3, B5, B6, B7 (421 rows). Rows can appear in several blocks (B3 repeats B2 rows with blank verification).
3. Replace `GM55` with `FV55` in `combined`. `_of_labels`: **4** for rows whose `plant` is still FV55 at that moment (the GM55 rows are not), `kanban_number`
   blank, `container` not starting `KC`, `pallet` in {1,2}, and **not yellow** (serial not in the history result), provided at least 2 such rows exist; **2 for every other row**.
   Here: 4 rows get 4 (the pallet-2 rows `373155345`, `373155553`, `373155637`, `373156067`), 417 get 2, total 850.
4. `export_date` = D. Pivot (`customer` × `plant`, sum `_of_labels`, totals) matches (grand total 850).

**Observed side effects (decision needed, items 33-34):** the saved `RELABEL_SN_DEL` sheet (the `sn_del` table) still carries the temporary `GM55` marks
(11 rows show `GM55` instead of `FV55`) **and one extra last row that is empty except `plant = GM55`** (so 6940 rows against 6939 in `snr_orig`). The
same extra row exists in `REPACK_SN_EXCL` (5840 rows against 5839 raw rows). Cause: `replace_column_values` runs on a visible range that includes one row below the data.

**`repack` → `snr_repack`** (`F` = the exclusion export, 5839 rows). Same `GM55` marking first. Exclusion list sent to SAP in the exclude tab of the parent-serial
selection = `lm_serial_number` of the **yellow rows of `combined`** (the 5 history serials; no raw row has one of them as parent). Build `snr_repack` by appending:
* **R1**: `plant` ≠ FV55 and not blank, `parent_serial_number` blank, `net_weight` < 50: 26 rows.
* **R2** (not appended): same with `net_weight` > 50: 36 rows → History export (380 rows, the 36 serials) → keep `warehouse_number = E01` and `status = UNP`: **4 rows appended**.
* **R3**: `plant` ≠ FV55 and not blank, `status = PAL`.
* Then **delete rows whose `container` starts `KC` or `SC`** from the rows collected so far (R1, history rows, R3).
* **R4**: `plant = FV55`, `pallet` blank, `container` not starting `KC` (appended after that deletion).
* Replace `GM55` → `FV55`. Output 280 rows in this order (R1, history, R3, R4), 16 columns incl. `export_date`. Pivot `customer` × `plant`, count of `lm_serial_number`,
  matches (grand total 280).

**New open questions**
33. **Temporary `GM55` marks and the extra row leak into `sn_del` / `repack_sn` outputs.** Decision needed: keep the headless tables clean (recommended: no marks, no extra row)?
34. **The macro never sorts `combined` / `snr_repack`** although the code calls `Multi_Sort` (`customer`, `plant`, `parent_serial_number`, `lm_serial_number`). The observed order is block order,
    each block in SAP order. Keep the observed order (recommended) or sort?
35. **Yellow highlight is a data channel in the macro** (history hit → yellow → excluded from 4 labels and from the repack exclusion list). The runner needs an explicit
    boolean column or list instead of colour.



### 9.6 Full-run verification: `xdock` (2026-11-05, `resources/xdock.zip`)

Files: `11-05-2026_XDOCK.xlsx` (macro output, sheets `XDOCK_VL06O`, `XDOCK_VT11_11-05-2026`, `pivot`) and raw `..._vt11.xlsx` (91 rows), `..._vl06o.xlsx` (1068 rows), `..._zmdetpc.xlsx` (140 rows).
No ZVT11 export is in the zip and the output has no `zvt11` sheet, consistent with 5.3 (ZVT11 is unused and could be dropped, item 10).
Step logic reproduced exactly:

* **VT11**: raw and output hold the same 91 shipments in the same order. Raw columns carry an empty first column `''`, headers are sanitised as documented. `descripof_shipment` is text
  (`10/29/2026`, `10/29/2026 1000`); Excel turned the date-only ones into dates in the output, keep text.
* **VL06O**: raw has **one row per delivery item** (1068 rows, 148 distinct deliveries). The output has 148 rows = **dedup by `delivery`, first row kept** (SAP order),
  then ordered ascending by `delivery` by the ZMDETPC merge (4.12). `number_of_packages` is identical on every item row of a delivery in this sample (first = last = max), so "first row" is safe.
* **Plant is not constant per delivery**: `85614256` has item rows `FV58` and blank, `85650066` has `FV55` and `FV59`; the macro keeps the first row's value. Three deliveries have a blank plant on every row
  (`85603116`, `85628836`, `85640568`), so the pivot has a `(blank)` row (4). **Pivot blank labels are therefore not always omitted**: here `(blank)` is present, Grand Total 1493 = sum of `total`.
  Same question as decision 29 (prefer the row with a plant?) for VL06O: not applied by the macro. *Q36: for xdock/toyota keep the first row, or prefer a row with a plant (would give `85614256` FV58 either way, `85650066` unchanged)?*
* **ZMDETPC**: summarised `delivery, pallet, child`, zero padded in raw (`0085614256`). 140 of the 148 deliveries have a row. For the 8 that do not (`85603116`, `85607680`, `85609181`, `85628836`,
  `85640568`, `85650310`, `85655488`, `8012734818`) the output has `delivery_` = `delivery` and `total` = `number_of_packages` (4.12 step 5). `8012734818` has 10 digits, so zero stripping must keep it as is.
* No delivery has `pallet + child = 0`, so the `0 → 1` replace (item 3) is still untested.
* **Pivot**: `plant` rows, sum of `total`, matches the output sheet exactly.
