//! HTML summary of a `manual_to` (LT03) run, built only from the files the run left on disk
//! (`summary.json`, `log.tsv` and the source tables next to them). No SAP, no clock in the
//! renderer: `--tcode lt03 --summarize[=YYYY-MM-DD]` and the end of every run use it.
//!
//! A run is a folder (`MANUAL_TO\<date>\run_HHMMSS`, or the day folder itself for runs made before
//! the run folders existed) plus its kind: the files of a dry run carry the `dryrun_` prefix. A run
//! counts as summarized when its `summary.html` exists.
#![allow(dead_code)]

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One run on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRef {
    pub dir: PathBuf,
    pub dry_run: bool,
}

impl RunRef {
    pub fn new(dir: &Path, dry_run: bool) -> Self {
        Self {
            dir: dir.to_path_buf(),
            dry_run,
        }
    }

    fn prefix(&self) -> &'static str {
        if self.dry_run {
            "dryrun_"
        } else {
            ""
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}{name}", self.prefix()))
    }

    pub fn html_path(&self) -> PathBuf {
        self.file("summary.html")
    }

    pub fn summarized(&self) -> bool {
        self.html_path().is_file()
    }

    /// `run_HHMMSS`, or `(day folder)` for a run stored directly in the day folder.
    pub fn id(&self) -> String {
        match self
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
        {
            Some(n) if n.starts_with("run_") => n,
            _ => "(day folder)".to_string(),
        }
    }

    /// `HH:MM:SS` of a `run_HHMMSS` folder, else empty.
    pub fn time(&self) -> String {
        let id = self.id();
        let digits: String = id
            .strip_prefix("run_")
            .unwrap_or("")
            .chars()
            .take(6)
            .collect();
        if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit()) {
            format!("{}:{}:{}", &digits[0..2], &digits[2..4], &digits[4..6])
        } else {
            String::new()
        }
    }
}

/// The runs of a day, oldest first (day folder, then the run folders; real before dry run).
pub fn list_runs(day_dir: &Path) -> Vec<RunRef> {
    let mut runs = Vec::new();
    for dir in crate::lt03_module::run_folders(day_dir) {
        for dry in [false, true] {
            let r = RunRef::new(&dir, dry);
            if r.file("summary.json").is_file() {
                runs.push(r);
            }
        }
    }
    runs
}

/// A tab separated file as text cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Tsv {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn read_tsv(path: &Path, name: &str) -> Option<Tsv> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let headers: Vec<String> = lines
        .next()?
        .trim_start_matches('\u{feff}')
        .split('\t')
        .map(str::to_string)
        .collect();
    let rows = lines
        .map(|l| {
            let mut c: Vec<String> = l.split('\t').map(str::to_string).collect();
            if c.len() < headers.len() {
                c.resize(headers.len(), String::new());
            }
            c
        })
        .collect();
    Some(Tsv {
        name: name.to_string(),
        headers,
        rows,
    })
}

/// Everything of a run that goes into the summary.
#[derive(Debug, Clone)]
pub struct RunData {
    pub run: RunRef,
    pub summary: Value,
    pub log: Option<Tsv>,
    /// The tables SAP returned (VL06O, `to_sn`, ...), by file name.
    pub sources: Vec<Tsv>,
}

pub fn load_run(run: &RunRef) -> Result<RunData> {
    let path = run.file("summary.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let summary: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid json", path.display()))?;
    let own = [
        run.file("log.tsv"),
        run.file("result.tsv"),
        run.file("picks.tsv"),
    ];
    let mut names: Vec<String> = std::fs::read_dir(&run.dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.to_ascii_lowercase().ends_with(".tsv"))
        // the files of the other kind of run can sit in the same (day) folder
        .filter(|n| n.starts_with("dryrun_") == run.dry_run)
        .filter(|n| {
            !own.iter()
                .any(|o| o.file_name().is_some_and(|f| f == n.as_str()))
        })
        .collect();
    names.sort();
    let sources = names
        .iter()
        .filter_map(|n| {
            let shown = n.strip_prefix(run.prefix()).unwrap_or(n);
            read_tsv(&run.dir.join(n), shown)
        })
        .collect();
    Ok(RunData {
        run: run.clone(),
        log: read_tsv(&run.file("log.tsv"), "log"),
        summary,
        sources,
    })
}

/// The numbers of the header cards.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stats {
    pub shipments: usize,
    pub deliveries: usize,
    /// Created (or planned in a dry run).
    pub ok: usize,
    pub no_items: usize,
    /// Deliveries that need attention, plus failed units.
    pub failed: usize,
    pub picks: usize,
    pub qty: f64,
    pub relabel: usize,
    pub repack: usize,
    pub skipped_materials: usize,
}

fn arr<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v[key].as_array().into_iter().flatten()
}

fn txt(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn num(v: &Value) -> String {
    match v.as_f64() {
        Some(q) => fmt_num(q),
        None => txt(v),
    }
}

fn fmt_num(q: f64) -> String {
    if q.fract() == 0.0 && q.abs() < 1e15 {
        format!("{}", q as i64)
    } else {
        q.to_string()
    }
}

pub fn stats(summary: &Value) -> Stats {
    let mut s = Stats::default();
    let mut ships: Vec<String> = Vec::new();
    for u in arr(summary, "units") {
        for sh in arr(u, "shipments").map(txt) {
            if !ships.contains(&sh) {
                ships.push(sh);
            }
        }
        if u["status"] == "failed" {
            s.failed += 1;
        }
        for d in arr(u, "deliveries") {
            s.deliveries += 1;
            match d["status"].as_str().unwrap_or("") {
                "created" | "planned" => s.ok += 1,
                "no_items" => s.no_items += 1,
                _ => s.failed += 1,
            }
            for m in arr(d, "materials") {
                if m["skipped"].is_string() {
                    s.skipped_materials += 1;
                }
                for p in arr(m, "picks") {
                    s.picks += 1;
                    s.qty += p["qty"].as_f64().unwrap_or(0.0);
                    s.relabel += usize::from(p["relabel"] == true);
                    s.repack += usize::from(p["repack"] == true);
                }
            }
        }
    }
    s.shipments = ships.len();
    s
}

/// One line for the list of runs.
pub fn describe(run: &RunRef) -> String {
    let what = match load_run(run) {
        Ok(d) => {
            let s = stats(&d.summary);
            format!(
                "{} shipment(s), {} delivery(ies): {} {}, {} failed",
                s.shipments,
                s.deliveries,
                s.ok,
                if run.dry_run { "planned" } else { "created" },
                s.failed
            )
        }
        Err(_) => "unreadable summary.json".to_string(),
    };
    format!(
        "{:<12}  {:<8}  {:<7}  {what}  [{}]",
        run.id(),
        run.time(),
        if run.dry_run { "dry run" } else { "real" },
        if run.summarized() {
            "summarized"
        } else {
            "not yet summarized"
        }
    )
}

// ---------------------------------------------------------------------------------------------
// html

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

const CSS: &str = r#"
:root{--bg:#f6f6f4;--card:#fff;--ink:#1d1e20;--mute:#6c7076;--line:#e5e5e1;--head:#fafaf8;--hover:#f4f6f8;
--ok:#1f7a4d;--ok-bg:#e7f3ec;--bad:#b2382c;--bad-bg:#fbeae7;--warn:#8a6200;--warn-bg:#faf2da;--vio:#6748ad;--vio-bg:#efeaf9;--mute-bg:#eeeeea}
@media (prefers-color-scheme:dark){:root{--bg:#131415;--card:#1b1c1e;--ink:#e7e7e4;--mute:#9a9da2;--line:#2b2d30;--head:#1f2123;--hover:#222528;
--ok:#72cf9d;--ok-bg:#15301f;--bad:#f08d83;--bad-bg:#3a1b18;--warn:#e2bd5c;--warn-bg:#322910;--vio:#baa5ee;--vio-bg:#272040;--mute-bg:#26282b}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 "Segoe UI",system-ui,-apple-system,Roboto,Arial,sans-serif;-webkit-font-smoothing:antialiased}
main{max-width:1240px;margin:0 auto;padding:36px 24px 56px}
header{display:flex;flex-wrap:wrap;align-items:flex-end;justify-content:space-between;gap:12px;padding-bottom:20px;border-bottom:1px solid var(--line)}
h1{margin:0;font-size:22px;font-weight:600;letter-spacing:-.01em}
.sub{margin-top:4px;color:var(--mute);font-size:13px}
.sub b{color:var(--ink);font-weight:500}
.pill{display:inline-block;padding:1px 9px;border-radius:999px;font-size:12px;font-weight:500;line-height:1.7;white-space:nowrap}
.ok{color:var(--ok);background:var(--ok-bg)}.bad{color:var(--bad);background:var(--bad-bg)}
.warn{color:var(--warn);background:var(--warn-bg)}.vio{color:var(--vio);background:var(--vio-bg)}
.mute{color:var(--mute);background:var(--mute-bg)}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:12px;margin:22px 0 8px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px 16px}
.card .k{color:var(--mute);font-size:11px;text-transform:uppercase;letter-spacing:.06em}
.card .v{margin-top:2px;font-size:24px;font-weight:600;font-variant-numeric:tabular-nums;letter-spacing:-.01em}
.card.is-bad .v{color:var(--bad)}.card.is-ok .v{color:var(--ok)}
.note{margin:12px 0 0;padding:10px 14px;border-radius:8px;font-size:13px}
.set{display:flex;flex-wrap:wrap;gap:6px 18px;margin:18px 0 0;padding:12px 16px;background:var(--card);border:1px solid var(--line);border-radius:10px;font-size:12.5px}
.set span{color:var(--mute)}.set b{font-weight:500;color:var(--ink)}
details.sec{margin-top:18px;background:var(--card);border:1px solid var(--line);border-radius:10px;overflow:hidden}
details.sec>summary{list-style:none;cursor:pointer;display:flex;align-items:center;gap:10px;padding:13px 16px;font-weight:600;font-size:14px;user-select:none}
details.sec>summary::-webkit-details-marker{display:none}
details.sec>summary::before{content:"";width:6px;height:6px;border-right:1.5px solid var(--mute);border-bottom:1.5px solid var(--mute);transform:rotate(-45deg);transition:transform .15s}
details.sec[open]>summary::before{transform:rotate(45deg)}
details.sec>summary .n{font-weight:400;color:var(--mute);font-size:12.5px}
details.sec>summary .f{margin-left:auto;font-weight:400;color:var(--mute);font-size:12px}
.bar{display:flex;align-items:center;gap:12px;padding:0 16px 12px}
.bar input{width:260px;max-width:100%;padding:6px 10px;border:1px solid var(--line);border-radius:7px;background:var(--bg);color:var(--ink);font:inherit;font-size:13px;outline:none}
.bar input:focus{border-color:var(--mute)}
.bar .c{color:var(--mute);font-size:12px}
.wrap{max-height:68vh;overflow:auto;border-top:1px solid var(--line)}
table{border-collapse:collapse;width:100%;font-size:13px;font-variant-numeric:tabular-nums}
th{position:sticky;top:0;z-index:1;background:var(--head);color:var(--mute);font-weight:500;font-size:11.5px;text-transform:uppercase;letter-spacing:.05em;text-align:left;padding:8px 14px;border-bottom:1px solid var(--line);white-space:nowrap;cursor:pointer}
th[data-s=a]::after{content:" \2191"}th[data-s=d]::after{content:" \2193"}
td{padding:7px 14px;border-bottom:1px solid var(--line);vertical-align:top;white-space:nowrap}
td.w{white-space:normal;min-width:240px}
td.r,th.r{text-align:right}
tbody tr:last-child td{border-bottom:0}
tbody tr:hover td{background:var(--hover)}
tr.skip td{color:var(--bad)}
tr.l-green td:first-child{box-shadow:inset 3px 0 var(--ok)}
tr.l-yellow td:first-child{box-shadow:inset 3px 0 var(--warn)}
tr.l-red td:first-child{box-shadow:inset 3px 0 var(--bad)}
.empty{padding:14px 16px;color:var(--mute);border-top:1px solid var(--line)}
h2{margin:34px 0 0;font-size:12px;font-weight:600;color:var(--mute);text-transform:uppercase;letter-spacing:.08em}
footer{margin-top:32px;color:var(--mute);font-size:12px}
@media print{body{background:#fff}.bar{display:none}.wrap{max-height:none;overflow:visible}details.sec{break-inside:auto}th{position:static}}
"#;

const JS: &str = r#"
document.querySelectorAll('input[data-filter]').forEach(function(inp){
  var t=document.getElementById(inp.dataset.filter),c=document.getElementById(inp.dataset.filter+'-c');
  inp.addEventListener('input',function(){
    var q=inp.value.toLowerCase(),rows=t.tBodies[0].rows,n=0;
    for(var i=0;i<rows.length;i++){var s=!q||rows[i].textContent.toLowerCase().indexOf(q)>=0;rows[i].hidden=!s;if(s)n++;}
    c.textContent=q?n+' of '+rows.length+' rows':rows.length+' rows';
  });
});
document.querySelectorAll('table th').forEach(function(th){
  th.addEventListener('click',function(){
    var tb=th.closest('table').tBodies[0],i=th.cellIndex,asc=th.dataset.s!=='a',rs=[].slice.call(tb.rows);
    rs.sort(function(a,b){
      var x=a.cells[i].textContent.trim(),y=b.cells[i].textContent.trim();
      var r=(x!==''&&y!==''&&!isNaN(x)&&!isNaN(y))?x-y:x.localeCompare(y);
      return asc?r:-r;
    });
    [].forEach.call(th.parentNode.cells,function(c){c.removeAttribute('data-s');});
    th.dataset.s=asc?'a':'d';
    rs.forEach(function(r){tb.appendChild(r);});
  });
});
"#;

/// A table row: css class and cells that are already html.
struct Row {
    class: &'static str,
    cells: Vec<String>,
}

/// A collapsible section with a filter box and a sortable table. `right` = right-aligned
/// columns, `wide` = columns whose text may wrap.
#[allow(clippy::too_many_arguments)]
fn section(
    out: &mut String,
    id: &str,
    title: &str,
    file: &str,
    open: bool,
    headers: &[String],
    right: &[usize],
    wide: &[usize],
    rows: &[Row],
) {
    out.push_str(&format!(
        "<details class=\"sec\"{}><summary><span>{}</span><span class=\"n\">{}</span>",
        if open { " open" } else { "" },
        esc(title),
        rows.len()
    ));
    if !file.is_empty() {
        out.push_str(&format!("<span class=\"f\">{}</span>", esc(file)));
    }
    out.push_str("</summary>");
    if rows.is_empty() {
        out.push_str("<div class=\"empty\">No rows.</div></details>\n");
        return;
    }
    out.push_str(&format!(
        "<div class=\"bar\"><input type=\"search\" placeholder=\"Filter rows\" \
         data-filter=\"{id}\"><span class=\"c\" id=\"{id}-c\">{} rows</span></div>\
         <div class=\"wrap\"><table id=\"{id}\"><thead><tr>",
        rows.len()
    ));
    let class = |i: usize| {
        if right.contains(&i) {
            " class=\"r\""
        } else if wide.contains(&i) {
            " class=\"w\""
        } else {
            ""
        }
    };
    for (i, h) in headers.iter().enumerate() {
        let c = if right.contains(&i) {
            " class=\"r\""
        } else {
            ""
        };
        out.push_str(&format!("<th{c}>{}</th>", esc(h)));
    }
    out.push_str("</tr></thead><tbody>\n");
    for r in rows {
        if r.class.is_empty() {
            out.push_str("<tr>");
        } else {
            out.push_str(&format!("<tr class=\"{}\">", r.class));
        }
        for (i, c) in r.cells.iter().enumerate() {
            out.push_str(&format!("<td{}>{c}</td>", class(i)));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody></table></div></details>\n");
}

fn heads(h: &[&str]) -> Vec<String> {
    h.iter().map(|s| s.to_string()).collect()
}

fn status_pill(key: &str) -> String {
    let (class, label) = match key {
        "created" => ("ok", "created"),
        "planned" => ("ok", "planned"),
        "no_items" => ("mute", "no items"),
        "in_process" => ("warn", "in process"),
        "nothing_picked" => ("bad", "nothing picked"),
        "failed" => ("bad", "failed"),
        other => ("mute", other),
    };
    format!("<span class=\"pill {class}\">{}</span>", esc(label))
}

fn card(out: &mut String, label: &str, value: &str, class: &str) {
    out.push_str(&format!(
        "<div class=\"card {class}\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
        esc(label),
        esc(value)
    ));
}

/// Column of a log / source table by header name (case-insensitive).
fn col(t: &Tsv, name: &str) -> Option<usize> {
    t.headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(name))
}

/// The whole page. `generated` is a display text (the caller's clock).
pub fn render_html(d: &RunData, generated: &str) -> String {
    let s = stats(&d.summary);
    let dry = d.run.dry_run || d.summary["dry_run"] == true;
    let date = txt(&d.summary["date"]);
    let mut o = String::new();
    o.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">");
    o.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    o.push_str(&format!(
        "<title>Manual TO {} {}</title><style>{CSS}</style></head><body><main>\n",
        esc(&date),
        esc(&d.run.id())
    ));

    // header
    let (first, last) = d
        .log
        .as_ref()
        .and_then(|t| {
            let c = col(t, "Timestamp")?;
            let mut ts: Vec<&str> = t
                .rows
                .iter()
                .filter_map(|r| r.get(c).map(String::as_str))
                .filter(|v| !v.is_empty())
                .collect();
            ts.sort_unstable();
            Some((ts.first()?.to_string(), ts.last()?.to_string()))
        })
        .unwrap_or_default();
    let time_of = |t: &str| t.split(' ').nth(1).unwrap_or(t).to_string();
    let mut sub = format!("<b>{}</b>", esc(&date));
    if !first.is_empty() {
        sub.push_str(&format!(
            " &middot; {} to {}",
            esc(&time_of(&first)),
            esc(&time_of(&last))
        ));
    }
    sub.push_str(&format!(" &middot; {}", esc(&d.run.id())));
    o.push_str(&format!(
        "<header><div><h1>Manual TO &middot; LT03</h1><div class=\"sub\">{sub}</div></div><div>"
    ));
    if dry {
        o.push_str("<span class=\"pill warn\">Dry run, nothing created</span> ");
    }
    if s.failed == 0 {
        o.push_str("<span class=\"pill ok\">No failures</span>");
    } else {
        o.push_str(&format!(
            "<span class=\"pill bad\">{} failure(s)</span>",
            s.failed
        ));
    }
    o.push_str("</div></header>\n<div class=\"cards\">");
    card(&mut o, "Shipments", &s.shipments.to_string(), "");
    card(&mut o, "Deliveries", &s.deliveries.to_string(), "");
    card(
        &mut o,
        if dry { "Planned" } else { "TO created" },
        &s.ok.to_string(),
        if s.ok > 0 { "is-ok" } else { "" },
    );
    card(
        &mut o,
        "Failed",
        &s.failed.to_string(),
        if s.failed > 0 { "is-bad" } else { "" },
    );
    card(&mut o, "No items", &s.no_items.to_string(), "");
    card(&mut o, "Picks", &s.picks.to_string(), "");
    card(&mut o, "Quantity", &fmt_num(s.qty), "");
    card(&mut o, "Relabel", &s.relabel.to_string(), "");
    card(&mut o, "Repack", &s.repack.to_string(), "");
    card(
        &mut o,
        "Skipped materials",
        &s.skipped_materials.to_string(),
        "",
    );
    o.push_str("</div>\n");

    // units that did not run through
    for u in arr(&d.summary, "units") {
        let status = u["status"].as_str().unwrap_or("");
        if status == "done" || status.is_empty() {
            continue;
        }
        let ships: Vec<String> = arr(u, "shipments").map(txt).collect();
        o.push_str(&format!(
            "<div class=\"note {}\">Shipment(s) {}: {} &middot; {}</div>\n",
            if status == "failed" { "bad" } else { "mute" },
            esc(&ships.join(", ")),
            if status == "failed" {
                "failed"
            } else {
                "no data, skipped"
            },
            esc(&txt(&u["message"]))
        ));
    }

    // settings of the run (first log line)
    if let Some(t) = &d.log {
        let settings = col(t, "Action").zip(col(t, "Details")).and_then(|(a, dd)| {
            t.rows
                .iter()
                .find(|r| r.get(a).is_some_and(|v| v == "Run Settings"))
                .and_then(|r| r.get(dd).cloned())
        });
        if let Some(text) = settings {
            o.push_str("<div class=\"set\">");
            for part in text.split(", ") {
                let (k, v) = part.split_once('=').unwrap_or((part, ""));
                o.push_str(&format!(
                    "<div><span>{}</span> <b>{}</b></div>",
                    esc(&k.replace('_', " ")),
                    esc(v)
                ));
            }
            o.push_str("</div>\n");
        }
    }

    // deliveries
    let mut rows = Vec::new();
    for u in arr(&d.summary, "units") {
        for del in arr(u, "deliveries") {
            let mats: Vec<&Value> = arr(del, "materials").collect();
            let skipped = mats.iter().filter(|m| m["skipped"].is_string()).count();
            rows.push(Row {
                class: "",
                cells: vec![
                    esc(&txt(&del["shipment"])),
                    esc(&txt(&del["delivery"])),
                    esc(&txt(&del["ship_to"])),
                    status_pill(del["status"].as_str().unwrap_or("")),
                    esc(&txt(&del["detail"])),
                    (mats.len() - skipped).to_string(),
                    skipped.to_string(),
                ],
            });
        }
    }
    section(
        &mut o,
        "t-del",
        "Deliveries",
        "",
        true,
        &heads(&[
            "Shipment",
            "Delivery",
            "Ship-to",
            "Status",
            "Detail",
            "Materials picked",
            "Skipped",
        ]),
        &[5, 6],
        &[4],
        &rows,
    );

    // picks
    let mut rows = Vec::new();
    for u in arr(&d.summary, "units") {
        for del in arr(u, "deliveries") {
            for m in arr(del, "materials") {
                let lead = |bin: String, qty: String, gr: String, flags: String, why: String| {
                    vec![
                        esc(&txt(&del["shipment"])),
                        esc(&txt(&del["delivery"])),
                        esc(&txt(&m["material"])),
                        esc(&num(&m["open_qty"])),
                        bin,
                        qty,
                        gr,
                        flags,
                        why,
                    ]
                };
                if let Some(why) = m["skipped"].as_str() {
                    rows.push(Row {
                        class: "skip",
                        cells: lead(
                            String::new(),
                            String::new(),
                            String::new(),
                            "<span class=\"pill bad\">skipped</span>".to_string(),
                            esc(why),
                        ),
                    });
                }
                for p in arr(m, "picks") {
                    let mut flags = String::new();
                    if p["relabel"] == true {
                        flags.push_str("<span class=\"pill warn\">relabel</span> ");
                    }
                    if p["repack"] == true {
                        flags.push_str("<span class=\"pill vio\">repack</span>");
                    }
                    rows.push(Row {
                        class: "",
                        cells: lead(
                            esc(&txt(&p["bin"])),
                            esc(&num(&p["qty"])),
                            esc(&txt(&p["gr_date"])),
                            flags,
                            esc(&txt(&p["reason"])),
                        ),
                    });
                }
            }
        }
    }
    section(
        &mut o,
        "t-pick",
        if dry { "Planned picks" } else { "Picks" },
        "",
        true,
        &heads(&[
            "Shipment", "Delivery", "Material", "Open", "Bin", "Qty", "GR date", "Flags", "Reason",
        ]),
        &[3, 5],
        &[8],
        &rows,
    );

    // log
    if let Some(t) = &d.log {
        let color = col(t, "Color");
        let details = col(t, "Details");
        let shown: Vec<usize> = (0..t.headers.len()).filter(|i| Some(*i) != color).collect();
        let rows: Vec<Row> = t
            .rows
            .iter()
            .map(|r| Row {
                class: match color.and_then(|c| r.get(c)).map(String::as_str) {
                    Some("green") => "l-green",
                    Some("yellow") => "l-yellow",
                    Some("red") => "l-red",
                    _ => "",
                },
                cells: shown
                    .iter()
                    .map(|i| esc(r.get(*i).map(String::as_str).unwrap_or("")))
                    .collect(),
            })
            .collect();
        let wide: Vec<usize> = shown
            .iter()
            .position(|i| Some(*i) == details)
            .into_iter()
            .collect();
        let headers: Vec<String> = shown.iter().map(|i| t.headers[*i].clone()).collect();
        section(
            &mut o,
            "t-log",
            "Log",
            &format!("{}log.tsv", d.run.prefix()),
            false,
            &headers,
            &[],
            &wide,
            &rows,
        );
    }

    // source tables
    if !d.sources.is_empty() {
        o.push_str("<h2>Source files</h2>\n");
    }
    for (n, t) in d.sources.iter().enumerate() {
        let rows: Vec<Row> = t
            .rows
            .iter()
            .map(|r| Row {
                class: "",
                cells: r.iter().map(|c| esc(c)).collect(),
            })
            .collect();
        let title = t.name.trim_end_matches(".tsv").replace('_', " ");
        section(
            &mut o,
            &format!("t-src{n}"),
            &title,
            &format!("{}{}", d.run.prefix(), t.name),
            false,
            &t.headers,
            &[],
            &[],
            &rows,
        );
    }

    o.push_str(&format!(
        "<footer>Generated {} from {}</footer>\n</main><script>{JS}</script></body></html>\n",
        esc(generated),
        esc(&d.run.dir.display().to_string())
    ));
    o
}

/// Build and write the summary of one run; returns the html file.
pub fn summarize(run: &RunRef) -> Result<PathBuf> {
    let data = load_run(run)?;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let path = run.html_path();
    std::fs::write(&path, render_html(&data, &now))
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Open the file with its default program when a person is at the terminal (never in a
/// scheduled / piped run). `SAP_NO_OPEN` switches it off.
pub fn open_if_terminal(path: &Path) {
    use std::io::IsTerminal;
    if std::env::var_os("SAP_NO_OPEN").is_some()
        || !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
    {
        return;
    }
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();
}

/// Without a prompt: every run that has no summary yet, else the newest run (rebuilt).
pub fn pick_unattended(runs: &[RunRef]) -> Vec<usize> {
    let pending: Vec<usize> = (0..runs.len()).filter(|i| !runs[*i].summarized()).collect();
    if pending.is_empty() {
        runs.len().checked_sub(1).into_iter().collect()
    } else {
        pending
    }
}

/// Date folders under `MANUAL_TO` that hold at least one run, newest last.
pub fn days_with_runs(reports_dir: &Path) -> Vec<String> {
    let mut days: Vec<String> = std::fs::read_dir(reports_dir.join("MANUAL_TO"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !list_runs(&e.path()).is_empty())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    days.sort();
    days
}

/// `--tcode lt03 --summarize[=YYYY-MM-DD]`: summarize a run that is already on disk (no SAP).
/// One run of the day is summarized right away; with several the list is shown and, at a
/// terminal, the user picks one (or all that have no summary yet). Without a terminal every run
/// that has no summary yet is summarized.
pub fn run_summarize(date: Option<NaiveDate>) -> Result<()> {
    let st = crate::lt03_module::settings_from_environment()?;
    let day = date.unwrap_or_else(|| chrono::Local::now().date_naive());
    let day_dir = crate::lt03_module::output_dir(&st.reports_dir, day);
    let runs = list_runs(&day_dir);
    if runs.is_empty() {
        let days = days_with_runs(&st.reports_dir);
        bail!(
            "no LT03 run found in {}{}",
            day_dir.display(),
            if days.is_empty() {
                String::new()
            } else {
                format!(
                    " (days with runs: {}; use --summarize=YYYY-MM-DD)",
                    days.join(", ")
                )
            }
        );
    }
    println!("Manual TO runs of {day} in {}:", day_dir.display());
    let lines: Vec<String> = runs.iter().map(describe).collect();
    for (i, l) in lines.iter().enumerate() {
        println!("  {}. {l}", i + 1);
    }
    let pending = runs.iter().filter(|r| !r.summarized()).count();
    let chosen: Vec<usize> = if runs.len() == 1 {
        vec![0]
    } else if crate::utils::cli_overrides::can_prompt() {
        let mut items = lines.clone();
        if pending >= 2 {
            items.push(format!("All {pending} runs that are not yet summarized"));
        }
        let default = runs.iter().position(|r| !r.summarized()).unwrap_or(0);
        let pick = dialoguer::Select::new()
            .with_prompt("Which run do you want summarized? (Esc = none)")
            .items(&items)
            .default(default)
            .interact_opt()
            .map_err(|e| anyhow!("selection failed: {e}"))?;
        match pick {
            None => {
                println!("Nothing summarized.");
                return Ok(());
            }
            Some(i) if i < runs.len() => vec![i],
            Some(_) => pick_unattended(&runs),
        }
    } else {
        pick_unattended(&runs)
    };
    let mut written = Vec::new();
    for i in &chosen {
        let f = summarize(&runs[*i])?;
        println!("wrote {}", f.display());
        written.push(f);
    }
    if let [one] = written.as_slice() {
        open_if_terminal(one);
    }
    crate::lt03_module::print_json_line(&serde_json::json!({
        "schema": 1, "tcode": "LT03", "kind": "summarize", "ok": true,
        "day": day.to_string(),
        "written": written.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(dry: bool) -> String {
        serde_json::json!({
            "tcode": "LT03", "date": "2026-11-13", "dry_run": dry, "ok": false, "failures": 1,
            "units": [{
                "unit": "all", "shipments": ["900", "100"], "status": "done", "message": "",
                "deliveries": [
                    {"delivery": "D1", "shipment": "900", "ship_to": "S<1>", "status":
                        if dry { "planned" } else { "created" }, "detail": "TO 123 & more",
                     "materials": [
                        {"material": "M1", "open_qty": 150.0, "skipped": null, "picks": [
                            {"bin": "B1", "qty": 100.0, "reason": "whole bin", "relabel": false,
                             "repack": false, "gr_date": "2026-11-01"},
                            {"bin": "B2", "qty": 50.5, "reason": "split", "relabel": true,
                             "repack": true}]},
                        {"material": "M2", "open_qty": 10.0, "skipped": "blocked stock",
                         "picks": []}]},
                    {"delivery": "D2", "shipment": null, "ship_to": null, "status": "failed",
                     "detail": "boom", "materials": []},
                    {"delivery": "D3", "shipment": "100", "ship_to": "S2", "status": "no_items",
                     "detail": "", "materials": []}]
            }, {
                "unit": "x", "shipments": ["777"], "status": "no_data", "message": "empty list",
                "deliveries": []
            }]
        })
        .to_string()
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lt03_sum_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_run(dir: &Path, dry: bool) {
        std::fs::create_dir_all(dir).unwrap();
        let p = if dry { "dryrun_" } else { "" };
        std::fs::write(dir.join(format!("{p}summary.json")), summary(dry)).unwrap();
        std::fs::write(
            dir.join(format!("{p}log.tsv")),
            "Main Object\tItem\tAction\tDetails\tTimestamp\tQty\tBin\tColor\r\n\
             Run\t\tRun Settings\tbin_pick_strategy=fifo, age_cutoff=30\t2026-11-13 09:00:01\t\t\t\r\n\
             D1\tM1\tPick\t<b>whole</b>\t2026-11-13 09:02:30\t100\tB1\tgreen\r\n",
        )
        .unwrap();
        std::fs::write(dir.join(format!("{p}result.tsv")), "unit\tdelivery\n").unwrap();
        std::fs::write(
            dir.join(format!("{p}all_vl06o.tsv")),
            "delivery\tmaterial\nD1\tM1\nD2\n",
        )
        .unwrap();
    }

    #[test]
    fn stats_count_deliveries_picks_and_flags() {
        let s = stats(&serde_json::from_str(&summary(false)).unwrap());
        assert_eq!(
            s,
            Stats {
                shipments: 3,
                deliveries: 3,
                ok: 1,
                no_items: 1,
                failed: 1,
                picks: 2,
                qty: 150.5,
                relabel: 1,
                repack: 1,
                skipped_materials: 1,
            }
        );
    }

    #[test]
    fn runs_of_a_day_are_listed_with_their_summary_state() {
        let day = temp("list");
        assert!(list_runs(&day).is_empty());
        // a run of the old layout (files in the day folder), a real run and a dry run
        write_run(&day, true);
        write_run(&day.join("run_090000"), false);
        write_run(&day.join("run_141500_2"), true);
        std::fs::create_dir_all(day.join("other")).unwrap();
        let runs = list_runs(&day);
        let ids: Vec<(String, bool)> = runs.iter().map(|r| (r.id(), r.dry_run)).collect();
        assert_eq!(
            ids,
            [
                ("(day folder)".to_string(), true),
                ("run_090000".to_string(), false),
                ("run_141500_2".to_string(), true)
            ]
        );
        assert_eq!(runs[1].time(), "09:00:00");
        assert_eq!(runs[2].time(), "14:15:00");
        assert_eq!(runs[0].time(), "");
        // nothing summarized yet: all of them; afterwards only the newest is rebuilt
        assert_eq!(pick_unattended(&runs), [0, 1, 2]);
        let f = summarize(&runs[1]).unwrap();
        assert!(f.ends_with("run_090000/summary.html") || f.ends_with("run_090000\\summary.html"));
        assert!(runs[1].summarized() && !runs[0].summarized());
        assert_eq!(pick_unattended(&runs), [0, 2]);
        assert!(describe(&runs[1]).contains("[summarized]"));
        assert!(describe(&runs[0]).contains("[not yet summarized]"));
        assert!(describe(&runs[0]).contains("3 shipment(s), 3 delivery(ies): 1 planned, 1 failed"));
        summarize(&runs[0]).unwrap();
        summarize(&runs[2]).unwrap();
        assert!(day.join("dryrun_summary.html").is_file());
        assert_eq!(pick_unattended(&runs), [2]);
        assert!(pick_unattended(&[]).is_empty());
        let _ = std::fs::remove_dir_all(&day);
    }

    #[test]
    fn page_has_every_part_and_escapes_the_data() {
        let day = temp("page");
        let dir = day.join("run_090000");
        write_run(&dir, false);
        // a dry-run table in the same folder is not a source of the real run
        std::fs::write(dir.join("dryrun_all_vl06o.tsv"), "a\nb\n").unwrap();
        let data = load_run(&RunRef::new(&dir, false)).unwrap();
        assert_eq!(data.sources.len(), 1);
        assert_eq!(data.sources[0].name, "all_vl06o.tsv");
        // short rows are padded
        assert_eq!(data.sources[0].rows[1], ["D2", ""]);
        let html = render_html(&data, "2026-11-13 10:00:00");
        for part in [
            "<title>Manual TO 2026-11-13 run_090000</title>",
            "09:00:01 to 09:02:30",
            "1 failure(s)",
            "TO created",
            "S&lt;1&gt;",
            "TO 123 &amp; more",
            "&lt;b&gt;whole&lt;/b&gt;",
            "<span class=\"pill warn\">relabel</span>",
            "<span class=\"pill vio\">repack</span>",
            "<span class=\"pill bad\">skipped</span>",
            "blocked stock",
            "<span>bin pick strategy</span> <b>fifo</b>",
            "<tr class=\"l-green\">",
            "no data, skipped &middot; empty list",
            "Source files",
            "all vl06o",
            "50.5",
            "2026-11-01",
            "Generated 2026-11-13 10:00:00",
        ] {
            assert!(html.contains(part), "missing {part:?}");
        }
        assert!(!html.contains("<b>whole</b>"));
        assert!(!html.contains("Dry run"));
        // the dry run of the same data says so
        write_run(&dir, true);
        let dry = render_html(&load_run(&RunRef::new(&dir, true)).unwrap(), "now");
        assert!(dry.contains("Dry run, nothing created") && dry.contains("Planned picks"));
        assert!(load_run(&RunRef::new(&day, false)).is_err());
        let _ = std::fs::remove_dir_all(&day);
    }
}
