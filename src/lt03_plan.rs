//! LT03 bin-picking planner: pure, SAP-free and deterministic (port of the snapshot planner of the
//! `manual_to` macro, `lt03_.bas`: `PlanPicksFromSnapshot` and its helpers).
//!
//! Input is a snapshot of the stock rows SAP shows for one material of a delivery
//! ([`SnapRow`]), the open quantity and the options ([`PlanOpts`]). Output is the list of picks
//! ([`Plan`]): which bin, how much. Nothing here touches SAP, the clock (the caller passes
//! `today`), the file system or the console, so a headless caller can use it as a library and the
//! dry run can show exactly what a real run would enter.
//!
//! Deliberate difference to the macro: bins already used in pass 1 (full multiples) are not offered
//! again in pass 2 with their full stock, so a plan never draws more than a bin holds. When no bin
//! is used twice the result is identical to the macro.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use chrono::NaiveDate;

/// How eligible bins are ordered / chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Snapshot order (macro default).
    Sequential,
    /// Deterministic pseudo-random rotation / tie-break (seeded by delivery, material, date).
    LoadBalance,
    /// Oldest goods receipt first.
    Fifo,
    /// Newest goods receipt first.
    Lifo,
}

impl Strategy {
    /// `sequential` (also empty), `load_balance`, `fifo`, `lifo` (case-insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "sequential" => Ok(Self::Sequential),
            "load_balance" => Ok(Self::LoadBalance),
            "fifo" => Ok(Self::Fifo),
            "lifo" => Ok(Self::Lifo),
            other => Err(anyhow!(
                "unknown bin_pick_strategy '{other}' (use fifo, lifo, load_balance or sequential)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::LoadBalance => "load_balance",
            Self::Fifo => "fifo",
            Self::Lifo => "lifo",
        }
    }

    pub fn uses_gr_date(self) -> bool {
        matches!(self, Self::Fifo | Self::Lifo)
    }
}

/// What to do with a material when the delivery has blocked stock (blacklisted bins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlacklistPolicy {
    /// Skip the whole material when any of its bins is blacklisted (macro default).
    SkipIfAny,
    /// Keep the material, only its blacklisted bins are skipped.
    AllowMaterialSkipBins,
}

impl BlacklistPolicy {
    /// Unknown values map to the safe legacy behaviour (`skip_if_any`), like the macro.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "allow_material_skip_bins" => Self::AllowMaterialSkipBins,
            _ => Self::SkipIfAny,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SkipIfAny => "skip_if_any",
            Self::AllowMaterialSkipBins => "allow_material_skip_bins",
        }
    }
}

/// One stock row of the LT03 table (only rows with a bin and a positive available quantity).
#[derive(Debug, Clone, PartialEq)]
pub struct SnapRow {
    /// Absolute row number in the SAP table (page * page size + visible row).
    pub abs_row: usize,
    pub bin: String,
    pub avail: f64,
    /// Quantity SAP already shows as selected (informational).
    pub sel: f64,
    /// Bin is on the ship-to whitelist.
    pub is_wl: bool,
    /// Goods receipt date; `None` when SAP shows none or it did not parse.
    pub gr_date: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub struct PlanOpts {
    pub strategy: Strategy,
    /// 0 = off. Only applies with fifo/lifo.
    pub age_cutoff_days: u32,
    pub allow_relabel: bool,
    pub delivery: String,
    pub material: String,
    pub today: NaiveDate,
}

impl PlanOpts {
    /// Seed of the load-balance rotation; 0 for the other strategies.
    pub fn seed(&self) -> u32 {
        if self.strategy == Strategy::LoadBalance {
            hash32(&format!(
                "{}|{}|{}",
                self.delivery,
                self.material,
                self.today.format("%Y%m%d")
            ))
        } else {
            0
        }
    }
}

/// One planned pick (one bin; several reasons are merged into one pick like the macro's dictionary).
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub bin: String,
    pub qty: f64,
    pub is_wl: bool,
    pub gr_date: Option<NaiveDate>,
    pub abs_row: usize,
    /// Rule(s) that produced the pick, e.g. `full multiples`, `exact`, `smallest >= open`.
    pub reason: String,
    /// The quantity is not a whole number of full pallets: the bin is broken up (a repack).
    pub repack: bool,
}

impl Pick {
    /// The bin belongs to another ship-to than the delivery's: it has to be relabeled.
    pub fn relabel(&self) -> bool {
        !self.is_wl
    }
}

/// A pick is a repack when it is not a whole multiple of the full pallet (or no pallet size is
/// known, so there is nothing to compare with).
pub fn is_repack(qty: f64, full_pallet: f64) -> bool {
    full_pallet <= 0.0 || vba_mod(qty, full_pallet) != Some(0)
}

/// Things the planner wants logged ("Bin Skipped", GR fallback).
#[derive(Debug, Clone, PartialEq)]
pub enum PlanEvent {
    /// Bin dropped by the GR age cutoff (`missing GR` or `older than <floor>`).
    AgeCutoffSkip { bin: String, reason: String },
    /// fifo/lifo requested but some bins have no GR date: sequential picks for this material.
    GrFallback { missing: usize },
}

/// A snapshot prepared for planning (GR mode resolved, age cutoff applied).
#[derive(Debug, Clone)]
pub struct Prepared {
    pub rows: Vec<SnapRow>,
    /// fifo/lifo ordering is in effect for this material.
    pub gr_active: bool,
    pub gr_fallback: bool,
    pub events: Vec<PlanEvent>,
}

/// The material cannot be prepared because some bins have no GR date and the caller does not
/// allow the fallback to sequential picks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGrDate {
    pub missing: usize,
}

impl std::fmt::Display for MissingGrDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bin(s) have no GR date", self.missing)
    }
}

impl std::error::Error for MissingGrDate {}

/// Result of [`plan`].
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub picks: Vec<Pick>,
    /// Quantity that could not be covered.
    pub remaining: f64,
    pub full_pallet: f64,
    pub can_use_non_wl: bool,
}

impl Plan {
    pub fn total(&self) -> f64 {
        self.picks.iter().map(|p| p.qty).sum()
    }
}

// ---------------------------------------------------------------------------------------------
// helpers

/// Deterministic 31-bit hash (the macro's `Hash32`: djb2-like with xor, kept non-negative).
pub fn hash32(s: &str) -> u32 {
    const MOD31: f64 = 2_147_483_648.0;
    let mut h = 5381.0_f64;
    for c in s.chars() {
        let byte = (c as u32 & 0xFF) as i64;
        h *= 33.0;
        h -= (h / MOD31).floor() * MOD31;
        h = (((h as i64) ^ byte) & 0x7FFF_FFFF) as f64;
    }
    h as u32
}

fn floor_to_multiple(x: f64, m: f64) -> f64 {
    if m <= 0.0 {
        0.0
    } else {
        (x / m).trunc() * m
    }
}

/// VBA `a Mod b`: both operands are rounded (banker's rounding) to integers first.
fn vba_mod(a: f64, b: f64) -> Option<i64> {
    let (a, b) = (a.round_ties_even() as i64, b.round_ties_even() as i64);
    if b == 0 {
        None
    } else {
        Some(a % b)
    }
}

/// Excel serial day number (sort key of the GR date).
fn serial(d: NaiveDate) -> f64 {
    (d - NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()).num_days() as f64
}

fn age_floor(opts: &PlanOpts) -> NaiveDate {
    opts.today - chrono::Duration::days(i64::from(opts.age_cutoff_days))
}

// ---------------------------------------------------------------------------------------------
// snapshot preparation

/// Resolve the GR-date mode and apply the age cutoff (macro: `ResolveGrDatePickForMaterial`,
/// `FilterSnapByAgeCutoff`).
///
/// With fifo/lifo and bins without a GR date (and no age cutoff, which would drop them) the
/// macro asks whether to continue sequentially; `proceed_without_gr` is that answer (headless:
/// `true`). `Err(MissingGrDate)` means "no" and the material is skipped.
pub fn prepare_snapshot(
    snap: &[SnapRow],
    opts: &PlanOpts,
    proceed_without_gr: bool,
) -> Result<Prepared, MissingGrDate> {
    let mut events = Vec::new();
    let mut gr_active = false;
    let mut gr_fallback = false;
    if opts.strategy.uses_gr_date() {
        let missing = if opts.age_cutoff_days > 0 {
            0 // dropped by the filter below, not a fallback trigger
        } else {
            snap.iter().filter(|r| r.gr_date.is_none()).count()
        };
        if missing == 0 {
            gr_active = true;
        } else if proceed_without_gr {
            gr_fallback = true;
            events.push(PlanEvent::GrFallback { missing });
        } else {
            return Err(MissingGrDate { missing });
        }
    }

    let rows = if opts.age_cutoff_days > 0 && gr_active {
        let floor = age_floor(opts);
        let mut kept = Vec::with_capacity(snap.len());
        for r in snap {
            match r.gr_date {
                None => events.push(PlanEvent::AgeCutoffSkip {
                    bin: r.bin.clone(),
                    reason: "missing GR".to_string(),
                }),
                Some(d) if d < floor => events.push(PlanEvent::AgeCutoffSkip {
                    bin: r.bin.clone(),
                    reason: format!("older than {}", floor.format("%Y-%m-%d")),
                }),
                Some(_) => kept.push(r.clone()),
            }
        }
        kept
    } else {
        snap.to_vec()
    };
    Ok(Prepared {
        rows,
        gr_active,
        gr_fallback,
        events,
    })
}

// ---------------------------------------------------------------------------------------------
// full-pallet detection

fn sum_avail(rows: &[SnapRow], whitelist_only: bool) -> f64 {
    rows.iter()
        .filter(|r| r.is_wl == whitelist_only)
        .map(|r| r.avail)
        .sum()
}

/// Whitelist availability, and (when relabel may widen to other ship-tos) the total.
/// Returns `(wl_avail, total_avail, can_use_non_wl)`: strict priority, non-whitelist bins only
/// count when the whitelist cannot cover the open quantity.
pub fn availability(rows: &[SnapRow], open_qty: f64, allow_relabel: bool) -> (f64, f64, bool) {
    let wl = sum_avail(rows, true);
    let can_use_non_wl = allow_relabel && wl < open_qty;
    let total = if can_use_non_wl {
        wl + sum_avail(rows, false)
    } else {
        wl
    };
    (wl, total, can_use_non_wl)
}

/// Full pallet size of the material: the most common bin quantity of the snapshot (ties: the
/// larger one). All bins count, ship-to bins and the bins of other ship-tos alike, because the
/// pallet size belongs to the material. Without any bin the DB hint (largest quantity seen for
/// the material) is used.
pub fn detect_full(rows: &[SnapRow], db_hint: f64) -> f64 {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for r in rows.iter().filter(|r| r.avail > 0.0) {
        match counts.iter_mut().find(|(v, _)| *v == r.avail) {
            Some(e) => e.1 += 1,
            None => counts.push((r.avail, 1)),
        }
    }
    let (mut best_cnt, mut best_val) = (0usize, 0.0);
    for (v, c) in counts {
        if c > best_cnt || (c == best_cnt && v > best_val) {
            best_cnt = c;
            best_val = v;
        }
    }
    if best_val > 0.0 {
        best_val
    } else {
        db_hint.max(0.0)
    }
}

// ---------------------------------------------------------------------------------------------
// planner

struct Ctx<'a> {
    rows: &'a [SnapRow],
    /// Quantity still available per snapshot row (shrinks as picks are planned).
    left: Vec<f64>,
    opts: &'a PlanOpts,
    gr_active: bool,
    seed: u32,
    picks: Vec<Pick>,
}

impl Ctx<'_> {
    fn n(&self) -> usize {
        self.rows.len()
    }

    fn rot_start(&self, n: usize) -> usize {
        if n == 0 {
            0
        } else if self.opts.strategy == Strategy::LoadBalance {
            (self.seed & 0x7FFF_FFFF) as usize % n
        } else {
            0
        }
    }

    /// 0-based index into a candidate list of `n`.
    fn choose_index(&self, n: usize, salt: &str) -> usize {
        if self.opts.strategy == Strategy::LoadBalance && n > 1 {
            let h = hash32(&format!(
                "{}|{}|{}",
                self.opts.delivery, self.opts.material, salt
            ));
            (((h & 0x7FFF_FFFF) ^ (self.seed & 0x7FFF_FFFF)) as usize) % n
        } else {
            0
        }
    }

    /// Candidate (index into `rows`) chosen among `cands`.
    fn pick_from(&self, cands: &[usize], salt: &str) -> Option<usize> {
        match cands.len() {
            0 => None,
            1 => Some(cands[0]),
            _ if self.gr_active => {
                let lifo = self.opts.strategy == Strategy::Lifo;
                let mut best: Option<NaiveDate> = None;
                for &i in cands {
                    if let Some(d) = self.rows[i].gr_date {
                        best = Some(match best {
                            None => d,
                            Some(b) if lifo && d > b => d,
                            Some(b) if !lifo && d < b => d,
                            Some(b) => b,
                        });
                    }
                }
                let ties: Vec<usize> = cands
                    .iter()
                    .copied()
                    .filter(|&i| best.is_none() || self.rows[i].gr_date == best)
                    .collect();
                ties.into_iter().min_by_key(|&i| self.rows[i].abs_row)
            }
            n => Some(cands[self.choose_index(n, salt)]),
        }
    }

    /// Rows of the wanted whitelist class in fifo/lifo order (date, then absolute row).
    fn gr_sorted(&self, whitelist_only: bool) -> Vec<usize> {
        let lifo = self.opts.strategy == Strategy::Lifo;
        let key = |i: usize| -> f64 {
            match self.rows[i].gr_date {
                None if lifo => 0.0,
                None => 9_999_999.0,
                Some(d) if lifo => -serial(d),
                Some(d) => serial(d),
            }
        };
        let mut idx: Vec<usize> = (0..self.n())
            .filter(|&i| self.rows[i].is_wl == whitelist_only)
            .collect();
        idx.sort_by(|&a, &b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(self.rows[a].abs_row.cmp(&self.rows[b].abs_row))
        });
        idx
    }

    /// Whole, untouched bins (whitelist first, then the others when allowed, in fifo / snapshot
    /// order) whose quantities add up to exactly `rem`; `None` when no such set exists. Earlier
    /// bins win (0/1 knapsack over whole quantities).
    fn whole_cover(&self, rem: f64, can_use_non_wl: bool) -> Option<Vec<usize>> {
        if rem <= 0.0 || rem.fract() != 0.0 || rem > 200_000.0 {
            return None;
        }
        let target = rem as usize;
        let n = self.n();
        let mut order: Vec<usize> = Vec::new();
        for wl in [true, false] {
            if !wl && !can_use_non_wl {
                continue;
            }
            if self.gr_active {
                order.extend(self.gr_sorted(wl));
            } else {
                let start = self.rot_start(n);
                order.extend(
                    (0..n)
                        .map(|k| (start + k) % n)
                        .filter(|&i| self.rows[i].is_wl == wl),
                );
            }
        }
        let cands: Vec<usize> = order
            .into_iter()
            .filter(|&i| {
                let a = self.left[i];
                a > 0.0 && a == self.rows[i].avail && a.fract() == 0.0 && a <= rem
            })
            .collect();
        let mut reach = vec![false; target + 1];
        let mut via: Vec<Option<usize>> = vec![None; target + 1];
        reach[0] = true;
        for (ci, &i) in cands.iter().enumerate() {
            let q = self.left[i] as usize;
            for s in (q..=target).rev() {
                if !reach[s] && reach[s - q] {
                    reach[s] = true;
                    via[s] = Some(ci);
                }
            }
        }
        if !reach[target] {
            return None;
        }
        let mut chosen: Vec<usize> = Vec::new();
        let mut s = target;
        while s > 0 {
            let ci = via[s]?;
            chosen.push(ci);
            s -= self.left[cands[ci]] as usize;
        }
        chosen.sort_unstable();
        Some(chosen.into_iter().map(|ci| cands[ci]).collect())
    }

    fn add_pick(&mut self, i: usize, qty: f64, reason: &str) {
        if qty <= 0.0 {
            return;
        }
        self.left[i] -= qty;
        let bin = &self.rows[i].bin;
        if let Some(p) = self.picks.iter_mut().find(|p| &p.bin == bin) {
            p.qty += qty;
            if !p.reason.split('+').any(|r| r == reason) {
                p.reason.push('+');
                p.reason.push_str(reason);
            }
        } else {
            let r = &self.rows[i];
            self.picks.push(Pick {
                bin: r.bin.clone(),
                qty,
                is_wl: r.is_wl,
                gr_date: r.gr_date,
                abs_row: r.abs_row,
                reason: reason.to_string(),
                repack: false,
            });
        }
    }
}

/// Plan the picks for `open_qty` of one material from the prepared snapshot.
///
/// Pass 1 takes full-pallet multiples from bins that fit completely (whitelist first, then the
/// other bins when relabel widened the search). Pass 2 covers the rest: an exact bin, else the
/// smallest bin that is large enough, else the largest one that fits, repeated until covered or
/// nothing is left to pick (at most 200 rounds).
pub fn plan(
    prepared: &Prepared,
    open_qty: f64,
    full_pallet: f64,
    can_use_non_wl: bool,
    opts: &PlanOpts,
) -> Plan {
    let rows = &prepared.rows;
    let mut cx = Ctx {
        rows,
        left: rows.iter().map(|r| r.avail).collect(),
        opts,
        gr_active: prepared.gr_active,
        seed: opts.seed(),
        picks: Vec::new(),
    };
    let mut rem = open_qty;

    // ---- pass 1: full multiples (whitelist, then the others)
    if full_pallet > 0.0 {
        for whitelist_only in [true, false] {
            if !whitelist_only && !can_use_non_wl {
                continue;
            }
            let n = cx.n();
            if n == 0 {
                continue;
            }
            let order: Vec<usize> = if cx.gr_active {
                cx.gr_sorted(whitelist_only)
            } else {
                let start = cx.rot_start(n);
                (0..n).map(|k| (start + k) % n).collect()
            };
            for i in order {
                if rem <= 0.0 {
                    break;
                }
                if rows[i].is_wl != whitelist_only {
                    continue;
                }
                // whole bins only: a bin that is not a multiple of the pallet is never split here
                let av = cx.left[i];
                if av >= full_pallet
                    && av <= rem
                    && av == cx.rows[i].avail
                    && vba_mod(av, full_pallet) == Some(0)
                {
                    cx.add_pick(i, av, "full multiples");
                    rem -= av;
                }
            }
        }
    }

    // ---- pass 2: partials
    let eligible =
        |cx: &Ctx, i: usize| -> bool { cx.left[i] > 0.0 && (can_use_non_wl || rows[i].is_wl) };
    let mut guard = 0;
    while rem > 0.0 {
        guard += 1;
        if guard > 200 {
            break;
        }

        // 2A: a bin holding exactly the rest (whitelist first)
        let exact: Vec<usize> = (0..cx.n())
            .filter(|&i| eligible(&cx, i) && cx.left[i] == rem)
            .collect();
        let (wl, nw): (Vec<usize>, Vec<usize>) = exact.iter().partition(|&&i| rows[i].is_wl);
        let chosen = if !wl.is_empty() {
            cx.pick_from(&wl, &format!("exact|{rem}"))
        } else if can_use_non_wl && !nw.is_empty() {
            cx.pick_from(&nw, &format!("exactNW|{rem}"))
        } else {
            None
        };
        if let Some(i) = chosen {
            cx.add_pick(i, rem, "exact");
            rem = 0.0;
            break;
        }

        // 2A+: several whole bins that add up to exactly the rest, before any bin is split
        if let Some(set) = cx.whole_cover(rem, can_use_non_wl) {
            for i in set {
                let q = cx.left[i];
                cx.add_pick(i, q, "whole bins");
            }
            rem = 0.0;
            break;
        }

        let mut picked = false;

        // 2B/2C: smallest bin >= rest (a non-whitelist one wins a tie when allowed)
        let mut best_ge = 0.0_f64;
        let mut ge: Vec<usize> = Vec::new();
        for i in 0..cx.n() {
            if !eligible(&cx, i) || cx.left[i] < rem {
                continue;
            }
            if best_ge == 0.0 || cx.left[i] < best_ge {
                best_ge = cx.left[i];
                ge = vec![i];
            } else if cx.left[i] == best_ge {
                ge.push(i);
            }
        }
        if !ge.is_empty() {
            let (wl_ge, nw_ge): (Vec<usize>, Vec<usize>) = ge.iter().partition(|&&i| rows[i].is_wl);
            let chosen = if can_use_non_wl && !nw_ge.is_empty() {
                cx.pick_from(&nw_ge, &format!("smallestGE_NW|{rem}"))
            } else if !wl_ge.is_empty() {
                cx.pick_from(&wl_ge, &format!("smallestGE_WL|{rem}"))
            } else {
                None
            };
            if let Some(i) = chosen {
                let mut take = rem;
                if full_pallet > 0.0 {
                    let flo = floor_to_multiple(rem, full_pallet);
                    if flo > 0.0 {
                        take = flo;
                    }
                }
                if take <= 0.0 {
                    take = rem;
                }
                cx.add_pick(i, take, "smallest >= open");
                rem -= take;
                picked = true;
            }
        }

        // 2D: largest bin <= rest (whitelist first)
        if !picked {
            let mut best_le = -1.0_f64;
            let mut le: Vec<usize> = Vec::new();
            for i in 0..cx.n() {
                if !eligible(&cx, i) || cx.left[i] > rem {
                    continue;
                }
                if cx.left[i] > best_le {
                    best_le = cx.left[i];
                    le = vec![i];
                } else if cx.left[i] == best_le {
                    le.push(i);
                }
            }
            let (wl_le, nw_le): (Vec<usize>, Vec<usize>) = le.iter().partition(|&&i| rows[i].is_wl);
            let chosen = if !wl_le.is_empty() {
                cx.pick_from(&wl_le, &format!("largestLE_WL|{rem}"))
            } else if can_use_non_wl && !nw_le.is_empty() {
                cx.pick_from(&nw_le, &format!("largestLE_NW|{rem}"))
            } else {
                None
            };
            if let Some(i) = chosen {
                let take = cx.left[i];
                cx.add_pick(i, take, "largest <= open");
                rem -= take;
                picked = true;
            }
        }

        if !picked {
            break;
        }
        if rem < 0.000_001 {
            rem = 0.0;
        }
    }

    let mut picks = cx.picks;
    for p in &mut picks {
        p.repack = is_repack(p.qty, full_pallet);
    }
    Plan {
        picks,
        remaining: rem.max(0.0),
        full_pallet,
        can_use_non_wl,
    }
}

/// Convenience: availability check + full-pallet detection + [`plan`] in the macro's order.
/// `Err(total_avail)` when the available quantity cannot cover `open_qty` ("Insufficient Quantity").
pub fn plan_material(
    prepared: &Prepared,
    open_qty: f64,
    db_full_pallet: f64,
    opts: &PlanOpts,
) -> Result<Plan, f64> {
    let (_wl, total, can_use_non_wl) = availability(&prepared.rows, open_qty, opts.allow_relabel);
    if total < open_qty {
        return Err(total);
    }
    let full = detect_full(&prepared.rows, db_full_pallet);
    Ok(plan(prepared, open_qty, full, can_use_non_wl, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn row(abs: usize, bin: &str, avail: f64, wl: bool, gr: Option<NaiveDate>) -> SnapRow {
        SnapRow {
            abs_row: abs,
            bin: bin.to_string(),
            avail,
            sel: 0.0,
            is_wl: wl,
            gr_date: gr,
        }
    }

    fn opts(strategy: Strategy) -> PlanOpts {
        PlanOpts {
            strategy,
            age_cutoff_days: 0,
            allow_relabel: false,
            delivery: "D1".into(),
            material: "M1".into(),
            today: d(2026, 11, 13),
        }
    }

    fn run(rows: Vec<SnapRow>, open: f64, db_full: f64, o: &PlanOpts) -> Plan {
        let prep = prepare_snapshot(&rows, o, true).unwrap();
        plan_material(&prep, open, db_full, o).unwrap()
    }

    fn bins(p: &Plan) -> Vec<(&str, f64)> {
        p.picks.iter().map(|p| (p.bin.as_str(), p.qty)).collect()
    }

    #[test]
    fn strategy_and_policy_parse() {
        assert_eq!(Strategy::parse("FIFO").unwrap(), Strategy::Fifo);
        assert_eq!(Strategy::parse("").unwrap(), Strategy::Sequential);
        assert!(Strategy::parse("random").is_err());
        assert_eq!(
            BlacklistPolicy::parse("allow_material_skip_bins"),
            BlacklistPolicy::AllowMaterialSkipBins
        );
        assert_eq!(
            BlacklistPolicy::parse("nonsense"),
            BlacklistPolicy::SkipIfAny
        );
    }

    #[test]
    fn hash32_is_stable_and_31_bit() {
        assert_eq!(hash32(""), 5381);
        let h = hash32("85453394|35500078|20261113");
        assert_eq!(h, hash32("85453394|35500078|20261113"));
        assert!(h <= 0x7FFF_FFFF);
        assert_ne!(h, hash32("85453394|35500078|20261114"));
    }

    #[test]
    fn repack_is_a_pick_that_is_not_a_whole_pallet_multiple() {
        assert!(!is_repack(120.0, 40.0));
        assert!(is_repack(100.0, 40.0));
        assert!(is_repack(40.0, 0.0));
        let o = opts(Strategy::Sequential);
        // pallet 50 (open 150 is made of 50s): all picks are whole pallets
        let p = run(
            vec![
                row(0, "A", 50.0, true, None),
                row(1, "B", 50.0, true, None),
                row(2, "C", 50.0, true, None),
            ],
            150.0,
            0.0,
            &o,
        );
        assert!(p.picks.iter().all(|x| !x.repack && !x.relabel()));
        // 70 ship-to + 30 other ship-to (relabel allowed) cover 100; pallet 20: the 30 is no multiple
        let mut ro = opts(Strategy::Sequential);
        ro.allow_relabel = true;
        let p = run(
            vec![
                row(0, "A", 30.0, false, None),
                row(1, "B", 70.0, true, None),
            ],
            100.0,
            20.0,
            &ro,
        );
        let a = p.picks.iter().find(|x| x.bin == "A").unwrap();
        assert!(a.relabel());
        assert!(p.picks.iter().any(|x| x.repack));
    }

    #[test]
    fn small_ship_to_bins_that_add_up_beat_a_full_pallet_of_another_ship_to() {
        // need 240; the only 240 pallet belongs to another ship-to, but the ship-to's own
        // 100 + 50 + 50 + 40 add up to exactly 240: no relabel, those four bins are taken whole
        let mut o = opts(Strategy::Sequential);
        o.allow_relabel = true;
        let rows = vec![
            row(0, "OTHER-240", 240.0, false, None),
            row(5, "OTHER-240B", 240.0, false, None),
            row(1, "A100", 100.0, true, None),
            row(2, "B50", 50.0, true, None),
            row(3, "C50", 50.0, true, None),
            row(4, "D40", 40.0, true, None),
        ];
        let p = run(rows, 240.0, 0.0, &o);
        let mut got: Vec<(&str, f64)> = bins(&p);
        got.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(
            got,
            vec![("A100", 100.0), ("B50", 50.0), ("C50", 50.0), ("D40", 40.0)]
        );
        assert!(!p.can_use_non_wl && p.picks.iter().all(|x| !x.relabel()));
        // the pallet is 240 (the most common quantity, other ship-to's bins count): all four
        // ship-to bins are smaller than a pallet, so every one of them is a repack
        assert_eq!(p.full_pallet, 240.0);
        assert!(p.picks.iter().all(|x| x.repack));
        assert_eq!(p.remaining, 0.0);
    }

    #[test]
    fn exact_bin_wins() {
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A", 100.0, true, None),
                row(1, "B", 40.0, true, None),
            ],
            40.0,
            0.0,
            &o,
        );
        assert_eq!(bins(&p), vec![("B", 40.0)]);
        assert_eq!(p.remaining, 0.0);
    }

    #[test]
    fn full_multiples_then_exact_rest() {
        // pallet 50 detected (open 150 divisible by 50, two bins of 50)
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A", 50.0, true, None),
                row(1, "B", 50.0, true, None),
                row(2, "C", 50.0, true, None),
            ],
            150.0,
            0.0,
            &o,
        );
        assert_eq!(p.full_pallet, 50.0);
        assert_eq!(bins(&p), vec![("A", 50.0), ("B", 50.0), ("C", 50.0)]);
        assert_eq!(p.remaining, 0.0);
    }

    #[test]
    fn smallest_ge_is_split_when_nothing_else_adds_up() {
        // open 120, pallet 40 (DB hint, a bin holds >= 40); no bin <= 120 divisible except 400
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A", 400.0, true, None),
                row(1, "B", 80.0, true, None),
            ],
            120.0,
            40.0,
            &o,
        );
        // no pallet fits (400) and no whole bins add up to 120: the smallest large-enough bin is split
        assert_eq!(bins(&p), vec![("A", 120.0)]);
        assert_eq!(p.remaining, 0.0);
    }

    #[test]
    fn largest_le_when_nothing_is_large_enough() {
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A", 30.0, true, None),
                row(1, "B", 70.0, true, None),
                row(2, "C", 20.0, true, None),
            ],
            100.0,
            0.0,
            &o,
        );
        assert_eq!(p.remaining, 0.0);
        assert_eq!(p.total(), 100.0);
        // the largest of the equally common quantities is the pallet (70)
        assert_eq!(p.full_pallet, 70.0);
    }

    #[test]
    fn whitelist_is_used_before_other_ship_tos() {
        let mut o = opts(Strategy::Sequential);
        o.allow_relabel = true;
        // whitelist covers the open quantity: the non-whitelist bin is never used
        let p = run(
            vec![
                row(0, "NW", 100.0, false, None),
                row(1, "WL", 100.0, true, None),
            ],
            100.0,
            0.0,
            &o,
        );
        assert!(!p.can_use_non_wl);
        assert_eq!(bins(&p), vec![("WL", 100.0)]);
    }

    #[test]
    fn relabel_widens_only_when_whitelist_is_short() {
        let mut o = opts(Strategy::Sequential);
        o.allow_relabel = true;
        let p = run(
            vec![
                row(0, "NW", 100.0, false, None),
                row(1, "WL", 60.0, true, None),
            ],
            100.0,
            0.0,
            &o,
        );
        assert!(p.can_use_non_wl);
        assert_eq!(p.total(), 100.0);
        assert_eq!(p.remaining, 0.0);
        // without relabel the same snapshot is insufficient
        let o2 = opts(Strategy::Sequential);
        let prep = prepare_snapshot(
            &[
                row(0, "NW", 100.0, false, None),
                row(1, "WL", 60.0, true, None),
            ],
            &o2,
            true,
        )
        .unwrap();
        assert_eq!(plan_material(&prep, 100.0, 0.0, &o2).unwrap_err(), 60.0);
    }

    #[test]
    fn fifo_takes_the_oldest_gr_first_lifo_the_newest() {
        let rows = vec![
            row(0, "NEW", 50.0, true, Some(d(2026, 11, 10))),
            row(1, "OLD", 50.0, true, Some(d(2026, 10, 20))),
            row(2, "MID", 50.0, true, Some(d(2026, 11, 1))),
        ];
        let p = run(rows.clone(), 50.0, 0.0, &opts(Strategy::Fifo));
        assert_eq!(bins(&p), vec![("OLD", 50.0)]);
        let p = run(rows, 50.0, 0.0, &opts(Strategy::Lifo));
        assert_eq!(bins(&p), vec![("NEW", 50.0)]);
    }

    #[test]
    fn fifo_ties_go_to_the_lower_row() {
        let rows = vec![
            row(5, "B", 50.0, true, Some(d(2026, 11, 1))),
            row(2, "A", 50.0, true, Some(d(2026, 11, 1))),
        ];
        let p = run(rows, 50.0, 0.0, &opts(Strategy::Fifo));
        assert_eq!(bins(&p), vec![("A", 50.0)]);
    }

    #[test]
    fn age_cutoff_drops_old_and_undated_bins_and_logs_them() {
        let mut o = opts(Strategy::Fifo);
        o.age_cutoff_days = 30;
        let rows = vec![
            row(0, "OLD", 50.0, true, Some(d(2026, 8, 1))),
            row(1, "NODATE", 50.0, true, None),
            row(2, "OK", 50.0, true, Some(d(2026, 11, 1))),
        ];
        let prep = prepare_snapshot(&rows, &o, true).unwrap();
        assert!(prep.gr_active && !prep.gr_fallback);
        assert_eq!(prep.rows.len(), 1);
        assert_eq!(
            prep.events,
            vec![
                PlanEvent::AgeCutoffSkip {
                    bin: "OLD".into(),
                    reason: "older than 2026-10-14".into()
                },
                PlanEvent::AgeCutoffSkip {
                    bin: "NODATE".into(),
                    reason: "missing GR".into()
                },
            ]
        );
        let p = plan_material(&prep, 50.0, 0.0, &o).unwrap();
        assert_eq!(bins(&p), vec![("OK", 50.0)]);
    }

    #[test]
    fn age_cutoff_is_ignored_without_fifo_lifo() {
        let mut o = opts(Strategy::Sequential);
        o.age_cutoff_days = 30;
        let rows = vec![row(0, "OLD", 50.0, true, Some(d(2020, 1, 1)))];
        let prep = prepare_snapshot(&rows, &o, true).unwrap();
        assert_eq!(prep.rows.len(), 1);
        assert!(prep.events.is_empty());
    }

    #[test]
    fn missing_gr_date_falls_back_or_refuses() {
        let o = opts(Strategy::Fifo);
        let rows = vec![
            row(0, "A", 50.0, true, None),
            row(1, "B", 50.0, true, Some(d(2026, 11, 1))),
        ];
        let prep = prepare_snapshot(&rows, &o, true).unwrap();
        assert!(!prep.gr_active && prep.gr_fallback);
        assert_eq!(prep.events, vec![PlanEvent::GrFallback { missing: 1 }]);
        // sequential order: A first
        assert_eq!(
            bins(&plan_material(&prep, 50.0, 0.0, &o).unwrap()),
            vec![("A", 50.0)]
        );
        assert_eq!(
            prepare_snapshot(&rows, &o, false).unwrap_err(),
            MissingGrDate { missing: 1 }
        );
    }

    #[test]
    fn load_balance_is_deterministic_and_spreads() {
        let rows: Vec<SnapRow> = (0..6)
            .map(|i| row(i, &format!("B{i}"), 50.0, true, None))
            .collect();
        let mut o = opts(Strategy::LoadBalance);
        let a = run(rows.clone(), 50.0, 0.0, &o);
        let b = run(rows.clone(), 50.0, 0.0, &o);
        assert_eq!(a, b);
        // different deliveries rotate to different bins at least once
        let mut seen = std::collections::HashSet::new();
        for k in 0..20 {
            o.delivery = format!("D{k}");
            seen.insert(run(rows.clone(), 50.0, 0.0, &o).picks[0].bin.clone());
        }
        assert!(seen.len() > 1, "{seen:?}");
    }

    #[test]
    fn insufficient_stock_is_reported() {
        let o = opts(Strategy::Sequential);
        let prep = prepare_snapshot(&[row(0, "A", 30.0, true, None)], &o, true).unwrap();
        assert_eq!(plan_material(&prep, 100.0, 0.0, &o).unwrap_err(), 30.0);
    }

    #[test]
    fn a_bin_is_never_drawn_beyond_its_stock() {
        // pass 1 uses A completely (100 = 2 x 50); pass 2 must not offer A again
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A", 100.0, true, None),
                row(1, "B", 30.0, true, None),
            ],
            130.0,
            50.0,
            &o,
        );
        for pick in &p.picks {
            let stock = if pick.bin == "A" { 100.0 } else { 30.0 };
            assert!(pick.qty <= stock, "{pick:?}");
        }
        assert_eq!(p.remaining, 0.0);
    }

    #[test]
    fn full_pallet_is_the_most_common_bin_quantity_ties_larger() {
        let r = |a: f64, wl: bool| row(0, "B", a, wl, None);
        // 240 is the most common quantity (other ship-to's bins count too)
        let rows = vec![
            r(240.0, false),
            r(240.0, false),
            r(100.0, true),
            r(50.0, true),
            r(50.0, true),
            r(40.0, true),
        ];
        assert_eq!(detect_full(&rows, 0.0), 240.0);
        // a tie goes to the larger quantity; all different = the largest
        assert_eq!(detect_full(&[r(40.0, true), r(120.0, true)], 0.0), 120.0);
        assert_eq!(
            detect_full(
                &[r(40.0, true), r(40.0, true), r(120.0, true), r(120.0, true)],
                0.0
            ),
            120.0
        );
        // no bins: the DB hint
        assert_eq!(detect_full(&[], 60.0), 60.0);
        assert_eq!(detect_full(&[], 0.0), 0.0);
    }

    #[test]
    fn whole_bins_are_preferred_to_splitting_one() {
        // need 240; ship-to bins 100 + 50 + 50 + 40 + 60 (300): 100+50+50+40 covers it, no bin is split
        let o = opts(Strategy::Sequential);
        let p = run(
            vec![
                row(0, "A100", 100.0, true, None),
                row(1, "B50", 50.0, true, None),
                row(2, "C50", 50.0, true, None),
                row(3, "D40", 40.0, true, None),
                row(4, "E60", 60.0, true, None),
            ],
            240.0,
            0.0,
            &o,
        );
        assert_eq!(p.remaining, 0.0);
        assert_eq!(p.total(), 240.0);
        for pick in &p.picks {
            let stock = match pick.bin.as_str() {
                "A100" => 100.0,
                "B50" | "C50" => 50.0,
                "D40" => 40.0,
                _ => 60.0,
            };
            assert_eq!(pick.qty, stock, "{pick:?} must be a whole bin");
        }
    }

    #[test]
    fn a_bin_is_split_only_when_no_whole_bins_add_up() {
        let o = opts(Strategy::Sequential);
        // 70 + 70 cannot make 100: one bin is split
        let p = run(
            vec![
                row(0, "A70", 70.0, true, None),
                row(1, "B70", 70.0, true, None),
            ],
            100.0,
            0.0,
            &o,
        );
        assert_eq!(p.total(), 100.0);
        assert_eq!(p.picks.len(), 2);
        assert!(p.picks.iter().any(|x| x.qty < 70.0));
    }
}
