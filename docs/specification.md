# Claude Code Build Specification — `ecu-shenanigans` v2

**Project:** ECU Datalog Analysis & "Sane Stage 1" Recommendation Tool
**Single target platform:** Škoda Fabia Mk1 (6Y2) · 1.4 TDI PD · engine code **AMF** · Bosch **EDC15P+** (HW `0281 011 412`, VAG PN `045 906 019 BM`, SW `1039S02900 1166 0178`, 16-Nov-2004)
**Tuning intent:** Conservative, longevity-first Stage 1
**Repo to refactor:** https://github.com/Flamchu/ecu-shenanigans
**Audience:** Claude Code agent — every threshold, map, and channel below is implementable as written.

---

## 1. Mission

Build a desktop tool that ingests **VCDS** (`.csv`) datalogs, reconstructs them into a typed, channel-aligned timeseries, plots the diagnostic channels a tuner actually cares about, evaluates a fixed **rule pack** specific to AMF + EDC15P+, and recommends **bounded, "sane Stage 1" deltas** to ECU map cells. The tool is **read-only against the ECU**: it never writes, never flashes, never talks to the OBD port. It only analyzes logs and emits suggestions.

The previous version of this repo carried a second platform (Honda Civic R18 / K-series). **That platform is being removed entirely.** All references, folders, channel maps, rule files, fixtures, and tests for `civic_r18` must be deleted. The tool is now single-platform.

## 2. Platform deep-dive — AMF / EDC15P+ / KP35

### 2.1 Vehicle & engine

| Parameter | Value | Note |
|---|---|---|
| Chassis | Škoda Fabia Mk1, type 6Y2 | A04/PQ24 platform |
| Kerb weight | ~1120 kg | Light, so torque budget is small |
| Engine code | AMF | 04/2003 – 10/2005 in Fabia |
| Displacement | 1422 cc (R3, 79.5 × 95.5 mm) | Three cylinders, "half a 1.9 PD" |
| Compression | 19.5:1 | Sets ignition-delay window |
| Stock power | 55 kW / 75 PS @ 4000 rpm | |
| Stock torque | 195 Nm @ 2200 rpm | |
| Injection | **Pumpe Düse** unit injectors, cam-driven, ~2050 bar peak | NOT common-rail |
| Turbo | **KKK KP35**, wastegated, **fixed geometry** | NOT VNT — this matters for control philosophy |
| Intercooler | Air-air, small | Heat-soak limited |
| MAF | Bosch HFM5 (hot-film), housing matched to ~70 mm/3-cyl, **datasheet ceiling ~640 kg/h** | Reports `mg/stroke` to ECU |
| MAP | Combined T-MAP after intercooler, ~2.5 bar absolute sensor range (saturates ~2495 mbar) | |
| Clutch / flywheel | LUK SMF (single-mass flywheel) clutch, OEM | Rated to handle ~195 Nm + ~20 % headroom ≈ **240 Nm hard ceiling** |
| Manifold | Cast-iron exhaust manifold | Tolerant of EGT but cracking risk if cycled hard |

### 2.2 Stock fuelling, boost & torque references (use these as defaults when no base map is loaded)

These are the values the recommendation engine should assume the **OEM map** contains for AMF. They come from community datalogs and EDC15P+ damos analysis; treat each as ±5 %.

| Channel | Stock value | Source / note |
|---|---|---|
| Peak IQ at WOT (low RPM, ~1750–2000 rpm) | **~44.5 mg/stroke** | Log-confirmed; this is what generates the 195 Nm torque peak |
| Peak IQ at high RPM (~4000 rpm) | **~37 mg/stroke** | Drops to make peak hp |
| Stock boost target ramp | 1100 mbar @ 1300 rpm → **~1900–2000 mbar @ 2000–3500 rpm** (absolute) → tapering to ~1750 mbar by 4500 rpm | Group-011 logs of similar 1.4 PD75 |
| Stock SVBL (boost limiter) | ~2200–2350 mbar absolute | Hard cut-off; ECU goes limp above |
| Stock SOI @ 4000 rpm, 37 mg | ~20–22° BTDC | EDC15P+ SOI maps |
| Stock atm-pressure correction (group 010) | ~990–1010 mbar at sea level | Used for altitude IQ derate |

### 2.3 Sane Stage 1 target envelope (final figures)

A "sane" Stage 1 on AMF is **not** the 105–125 hp number quoted by aggressive shops. Those tunes either smoke, kill the KP35 by overspeed, or chew the LUK SMF. The target here:

| Metric | Stock | **Sane Stage 1 target** | Aggressive (NOT this tool's target) |
|---|---|---|---|
| Peak power | 55 kW / 75 hp | **~70 kW / 95 hp** | 77 kW / 105 hp + |
| Peak torque | 195 Nm | **~240 Nm** (clutch ceiling) | 270–300 Nm |
| Peak IQ | 44.5 mg | **50–52 mg** (Δ +6–8 mg) | 55–60 mg |
| Peak boost (absolute) | ~2000 mbar | **~2100–2150 mbar** (Δ +100–200) | 2200–2300 mbar |
| Min lambda at WOT | ~1.30 | **≥1.20** | 1.10–1.15 |
| Pre-turbo EGT ceiling | <750 °C | **<800 °C sustained** | 850–900 °C |
| SOI advance @ 4000 rpm | ~21° | **+1.5 to +2.5°** | +4° or more |

### 2.4 Known weak points (drive every rule's rationale)

1. **LUK SMF clutch** — designed around 195 Nm. Above ~240 Nm at the flywheel it slips, then judders, then dies. This is the single dominant longevity constraint.
2. **KP35 compressor** — small wastegated turbo. Compressor map peak efficiency island sits roughly at PR ~1.9–2.1 / ~7–8 lb/min. Sustained operation above PR 2.15 (i.e. >2150 mbar absolute at sea level) puts you off the right edge of the map → rapidly rising outlet temp, shaft over-speed (rated ~206 000 rpm), bearing failure within months. Community-quoted longevity ceiling: **~2100 mbar absolute, with taper to ~1900 by 4500 rpm.**
3. **PD injectors** at ~50 mg/stroke approach the duration headroom of the stock cam lobe — EOI starts pushing past 6° ATDC and EGT climbs sharply.
4. **Cast-iron manifold** is durable but cracks at the runner-collector weld under repeated 850 °C+ thermal cycles.
5. **Stock pistons** — aluminium, no oil-jets on AMF. SOI advance >27° BTDC at high IQ punches a hole in piston #1 (closest to belt end, hottest).
6. **MAF saturation** — Bosch HFM5 reports up to ~1100 mg/stroke before it pegs. Stock peak is around 600 mg/stroke; Stage 1 will see ~700–750.
7. **Bosch MAP sensor** saturates at ~2495 mbar — never request above ~2400 or you lose closed-loop boost control entirely.

### 2.5 EDC15P+ map structure (canonical names + axes the rule engine references)

These map identifiers come from the public EDC15P+ damos / WinOLS / VAGEDCSuite tradition. The tool **does not edit the binary** — but its recommendation engine emits deltas tied to these names so a tuner can paste them into WinOLS/EDCSuite by hand.

| Map (canonical name) | German alias | Axes | Cell unit | Typical dim | Stage 1 sane Δ |
|---|---|---|---|---|---|
| **LDRXN** — Boost target | *Ladedruck-Sollwert* | RPM × IQ (mg/str) | mbar absolute | 16 × 10 | +100 to +200 mbar in 2000–3500 rpm band, **taper to stock by 4000 rpm** |
| **LDOLLR / LDRPMX** — Boost limiter (max permitted absolute) | *LDR-Sollwertbegrenzung* | RPM × atm-pressure | mbar | 16 × 10 | Cap at **2150 mbar** at sea level; preserve altitude derate |
| **SVBL** — Single Value Boost Limit (overboost cut) | *Lade­druck-Begrenzung absolut* | scalar | mbar | 1×1 | Leave stock (~2350) — never raise above 2400 |
| **Driver Wish (DW)** | *Fahrer­wunsch* | Pedal % × RPM | mg/str | 8 × 16 | Raise WOT column to +6 to +8 mg in 1800–3500 rpm band |
| **Smoke limiter — IQ by MAF** | *Begrenzungs­menge (MAF)* | MAF (mg/str) × RPM | mg/str | 13 × 16 | Re-scale axis & enforce ≥1.20 λ everywhere |
| **Smoke limiter — IQ by MAP** (active on AMF, switch byte = 257) | *Begrenzungs­menge (MAP)* | Boost (mbar) × RPM | mg/str | 11 × 16 | Same lambda discipline, in MAP space |
| **Torque limiter** | *Drehmoment­begrenzer* | RPM × atm-pressure | Nm (then Nm→IQ map) | 20 × 3 | Cap at **240 Nm** flywheel — clutch-protective |
| **MLHFM** — MAF linearisation | *Luftmassen­messer-Kennlinie* | sensor V (or raw) → kg/h | kg/h | 256 pts | Leave stock unless MAF is replaced; flag if log-derived MAF deviates >10 % from spec |
| **SOI map 0…9** — Start of injection (10 maps selected by coolant temp) | *Spritzbeginn* | RPM × IQ | ° BTDC | 10 × 10 (cold maps) and 10 × 16 (hot maps) | +1.5 to +2.5° at 4000 rpm column only; **never exceed 26° BTDC** |
| **Duration maps 0…5** — Injection duration (selected by SOI band) | *Einspritz­dauer* | RPM × IQ | ° crank | 10 × 10 / 16 × 15 | Extend axis to support 52 mg/str if extending IQ |
| **Pilot quantity & timing** | *Vor­ein­spritz­menge / -zeit* | RPM × IQ | mg/str / ° | 10 × 10 | **Leave stock** for sane Stage 1 |
| **N75 duty / boost actuator** | *Lade­druck­regler-Tast­verhältnis* | RPM × spec/actual diff or RPM × IQ | % DC | 10 × 16 | Leave PID and N75 base map alone unless data shows steady-state error >150 mbar |
| **Lambda / AFR limiter** (some firmwares) | *Lambda­wunsch / Rauch­begrenzung* | MAF × RPM | λ | 13 × 16 | Floor cells at 1.20 |
| **Atmospheric / altitude correction** | *Höhen­korrektur LDR* | atm-pressure scalar | mbar Δ | 10 × 1 | Leave stock |
| **EGT model / fuel cut on temp** (where present in EDC15P+) | *Abgas­temperatur-Modell* | RPM × IQ × MAF | °C | varies | Do not raise — used as a backstop |

The recommendation engine **operates in the symbolic domain** — it emits suggestions like:
> "Raise `LDRXN[2000:3500, 40-50mg]` by +150 mbar; raise corresponding `Smoke_IQ_by_MAP[2050-2150 mbar, 2000-3500 rpm]` to 50 mg; raise `Driver_Wish[100%, 1800-3500]` to 50 mg; verify `Torque_Limiter` peak ≤ 240 Nm; SOI +1.5° at 4000 rpm column only."

It does **not** parse the .bin. (.bin parsing is explicitly out of scope.)

## 3. Tech stack & coding conventions

* **Python 3.11+**, single-machine desktop app.
* GUI: **PySide6** (Qt 6).
* Plotting: **pyqtgraph** for live/scrubbable timeseries, **matplotlib** for static export.
* Data: **pandas**, **numpy**.
* Schema/validation: **pydantic v2** for the channel model and rule output.
* Tests: **pytest**, **hypothesis** for property tests on the rule engine, golden-file tests on rule output.
* Lint/format: **ruff** + **black** (line length 100).
* Type-checking: **mypy --strict** in `src/`.
* Packaging: **uv** for lockfile + venv; entrypoint `ecu-shenanigans` exposed via `pyproject.toml`.
* No network calls anywhere in the runtime path. The tool is fully offline.

## 4. Functional requirements

### 4.1 Log ingestion

Single supported source: VCDS `.csv` exports (groups 001/003/004/008/010/011/013/015/020/031 — see §6).

* Detect VCDS vs generic CSV by inspecting first 8 lines.
* Parse VCDS' two-line group/field header pair into a flat column map.
* Support both EU (`,` decimal, `;` separator) and US (`.` decimal, `,` separator) localizations — VCDS varies by Windows locale.
* Handle missing groups gracefully: if the user only logged 001+003+004, downstream rules that need 011 are reported as **`SKIPPED — channel boost_actual not present`** rather than crashing.
* Re-sample to a uniform 5 Hz timebase (linear interp for continuous channels, last-observation-carried-forward for status/binary). 5 Hz ≈ the ceiling VCDS achieves on EDC15P+ over KW1281 with one or two groups (~3.5–4.5 samples/sec); we slightly oversample then smooth, never extrapolate.

### 4.2 Visualisation

* 4-pane synchronized plot: **(a)** RPM, **(b)** boost spec vs actual + N75 DC, **(c)** IQ + MAF spec vs actual, **(d)** SOI + coolant + MAP duty.
* Drag-to-zoom, shared X axis, crosshair readout, region-select to send a window to the rule engine.
* Per-WOT-pull auto-detection: a "pull" is `pedal ≥ 95 %` AND `RPM rising` AND duration ≥ 2 s. Each pull becomes a clickable marker.
* Export plot as PNG/SVG.

### 4.3 Rule pack — TDI/EDC15P+ specific (~12 rules, every threshold cited)

Every rule is a `pydantic` model with: `id`, `severity` (`info | warn | critical`), `predicate(df)`, `rationale_one_liner`, `recommended_delta_ref` (points to a row in the §4.4 table). The engine evaluates them per WOT pull and aggregates.

| ID | Rule | Threshold | Severity | One-line rationale |
|---|---|---|---|---|
| **R01** | Underboost | `boost_actual < boost_spec − 150 mbar` for ≥ 1.0 s above 2000 rpm | warn | KP35 PID can't keep up: leak, sticky wastegate, or LDRXN ramp too steep for turbo. |
| **R02** | Overboost spike | `boost_actual > boost_spec + 200 mbar` OR `> 2200 mbar absolute` | critical | KP35 sustained over 2150 mbar pushes shaft past the right edge of the compressor map → over-speed. |
| **R03** | Boost target excessive | Any `boost_spec` cell > **2150 mbar** absolute (sea-level), or > stock+250 mbar | critical | Hard envelope ceiling for KP35 longevity. |
| **R04** | High-RPM boost not tapering | `boost_spec @ 4500 rpm > boost_spec @ 3000 rpm − 100 mbar` | warn | KP35 is choke-flow-limited: you must back off above 4000 to keep it in the efficiency island. |
| **R05** | MAF below spec | `MAF_actual < MAF_spec − 8 %` over a pull | warn | MAF drift, dirty intake, boost leak, or MAF aging — fueling decisions become wrong. |
| **R06** | Lambda floor breach | `MAF_actual / IQ_actual < 1.20 × 14.5` (i.e. λ < 1.20) at any sample | critical | Below λ = 1.20 on PD = visible smoke + EGT spike + DPF/cat damage. **1.05 is the hard "would melt pistons" floor**; we set the user-facing floor at 1.20 for margin. |
| **R07** | Peak IQ above sane envelope | `IQ_actual > 52 mg/stroke` | critical | Above 52 mg the stock LUK clutch and stock injectors run out of headroom. |
| **R08** | Torque-equivalent above clutch | Modelled torque (IQ → Nm via stock 4.4 Nm/mg constant) > **240 Nm** | critical | LUK SMF rated ~195 Nm + ~20 % = 240 Nm hard ceiling. Above this the clutch slips within weeks. |
| **R09** | SOI excess | Any logged SOI > **26° BTDC** at any IQ ≥ 30 mg | critical | Beyond 26° BTDC peak cylinder pressure migrates ahead of TDC → piston-crown stress; physical cam-lobe limit on PD is ~35° but the safe usable limit is 26–28°. |
| **R10** | EOI late | Computed EOI = `SOI − duration` > **10° ATDC** | warn | Combustion past ~6–10° ATDC dumps unburned heat into the turbine → high EGT, poor BSFC. |
| **R11** | Coolant too low for pull | Any WOT-pull region with coolant < 80 °C | info | EDC15P+ uses cold SOI map below 80 °C — your data isn't representative of "warm" calibration. Re-do the pull. |
| **R12** | Atmospheric correction missing | Group 010 absent and elevation unknown | info | Without ambient pressure capture (key-on, engine-off, group 010), altitude derate can't be assessed. |
| **R13** | Fuel temp high | Group 013 fuel temp > 80 °C during a pull | warn | High fuel temp = lower density = lower delivered IQ for same duration → boost target overshoots fuelling. |
| **R14** | Smooth-running deviation | Any cylinder in group 013 > ±2.0 mg from mean | warn | Indicates worn injector cam lobe (PD weak point) or failing injector. Tuning a sick engine = killing it faster. |
| **R15** | Limp / DTC interruption | Any group with `Boost_DC` clamped to single value across pull | warn | ECU is in limp mode — log is not valid for tuning. |

Each finding produces a `Finding` object with: `pull_id`, `t_start`, `t_end`, `rule_id`, `severity`, `observed_extreme`, `threshold`, `rationale`, `recommended_action_ref`.

### 4.4 Recommendation engine + default sane Stage 1 deltas

When findings indicate "headroom available" (not "envelope breached"), the engine emits suggested deltas. **All deltas are bounded by §5.** When no base-map context is loaded, fall back to this default table:

| Map | Cells affected | Default sane Δ | Bounded by |
|---|---|---|---|
| `LDRXN` (boost target) | RPM 2000–3500 × IQ 40–50 mg | +150 mbar (clamped to absolute ≤ 2150 mbar) | R02, R03, §5 |
| `LDRXN` taper | RPM 4000–4500 | hold at stock − 50 mbar | R04 |
| `Driver_Wish` | Pedal 100 % × RPM 1800–3500 | raise to 50 mg | R07 |
| `Smoke_IQ_by_MAP` | Boost 2000–2150 mbar × RPM 2000–3500 | enforce λ ≥ 1.20 by computed IQ cap | R06 |
| `Smoke_IQ_by_MAF` | MAF 600–750 mg/str × RPM 2000–3500 | same | R06 |
| `Torque_Limiter` | full surface | clamp peak to 240 Nm equivalent | R08 |
| `SOI` (warm map only) | RPM 3500–4500 × IQ 40–50 mg | +1.5° BTDC, capped at 26° BTDC absolute | R09 |
| `Duration` | extend X-axis from 50 → 52 mg | proportional extension only | R07 |
| `SVBL` | scalar | **leave stock** | R03 |
| `N75 duty / PID` | full | **leave stock** unless R01 fires | R01 |
| `Pilot injection` (qty & timing) | full | **leave stock** | (NVH, not power) |
| `MLHFM` | full | **leave stock** | R05 |

The output of the recommendation engine is a Markdown report with: (a) summary table of findings, (b) per-pull plots, (c) the table above with each row marked APPLY / SKIP / BLOCKED-BY-ENVELOPE based on what the log shows.

## 5. Sane Stage 1 envelope — hard guardrails (engine MUST NEVER exceed)

These are absolute caps. The recommendation engine **clamps every suggested delta** against this table before emitting it. If a rule wants a value outside the envelope, the engine emits **"BLOCKED — envelope cap"** instead of the delta, and explains which cap.

| Quantity | Hard cap | Why this number |
|---|---|---|
| Peak boost (absolute) | **2150 mbar** | Right edge of KP35 efficient compressor map at AMF flow rates. |
| Peak boost above 4000 rpm | **2050 mbar** | KP35 chokes; sustained PR > 2.0 at high mass-flow → over-speed. |
| Peak IQ | **52 mg/stroke** | Stock injector duration headroom; LUK clutch torque ceiling. |
| Lambda floor (λ_min) | **1.20** | Below this PD smokes and EGT climbs faster than the EGT model can derate. (Hard physics floor is 1.05; we keep 0.15 of margin.) |
| Pre-turbo EGT (modelled or measured) | **800 °C sustained** | Cast-iron manifold creep + KP35 turbine wheel material limit + AMF has no oil-jet pistons. |
| SOI advance | **26° BTDC** at any IQ ≥ 30 mg | Beyond this peak cylinder pressure ahead of TDC stresses the unjacketed pistons. Physical cam-lobe limit is 35°. |
| EOI (SOI − duration) | **10° ATDC** | Past this, heat dumps into the turbine, hurting both EGT and BSFC. |
| Modelled flywheel torque | **240 Nm** | LUK SMF clutch ceiling (195 Nm × 1.23 headroom). |
| MAF reading | **1000 mg/stroke** | HFM5 starts non-linear above this; stay below to keep MLHFM valid. |
| SVBL change | **0** | Never touch the overboost cut; it's the last line of defence. |

Every rule's recommended delta passes through `clamp_to_envelope()` before reaching the report.

## 6. VCDS log import specifics

### 6.1 Groups to log on AMF/EDC15P+ (this is the canonical list the tool expects)

| Group | Fields (1 → 4) | Units | Tuning relevance |
|---|---|---|---|
| **001** | RPM · injected qty · modulating-piston voltage · coolant temp | rpm · mg/str · V · °C | Always log. Idle health + IQ adaptation baseline. |
| **003** | RPM · MAF spec · MAF actual · EGR duty | rpm · mg/str · mg/str · % | **Critical** — fuelling math depends on this. |
| **004** | RPM · battery V · coolant · TDC sensor | rpm · V · °C · — | Sanity. |
| **008** | RPM · IQ requested · IQ RPM-limit · IQ MAF-limit | rpm · mg/str · mg/str · mg/str | **Critical** — shows which limiter is active during a pull. |
| **010** | MAF · barometric pressure · TPS % · — | mg/str · mbar · % · — | **Log key-on/engine-off** to capture ambient pressure for altitude correction. |
| **011** | RPM · boost spec · boost actual · N75 duty cycle | rpm · mbar abs · mbar abs · % | **The boost group.** This is what most tuning decisions hinge on. |
| **013** | Smooth-running cyl 1 · cyl 2 · cyl 3 · — (or fuel temp on some firmwares) | mg/str (or °C) · — | Cam/injector health; fuel temp affects density. |
| **015** | RPM · cruise/torque request · torque actual · — | rpm · Nm · Nm · — | Torque limiter visibility. |
| **020** | RPM · timing actual · MAP abs · load | rpm · ° BTDC · mbar · % | **The timing group.** SOI logging. |
| **031** | (on EDC15P+ TDI: variant of MAF) | varies | Fallback if 003 unavailable. |

### 6.2 VCDS CSV header signature

VCDS exports a two-row banner per group followed by a metadata row, then data rows. The expected pattern:

```
Group A:,001,Group B:,003,Group C:,011
Engine speed,Injection quantity,Engine speed,Air mass spec,Engine speed,Charge pressure spec,...
RPM,mg/H,RPM,mg/H,RPM,mbar,...
TIME,STAMP,001-1,001-2,003-1,003-2,003-3,003-4,011-1,011-2,011-3,011-4
0.0,12:34:56.789,820,4.5,820,310,302,12.5,820,1010,1005,8
...
```

The parser:
1. Strips the Group banner row, extracts `(group_id → field_name → unit)` triples.
2. Maps each `NNN-K` column header to a **canonical channel name** (see §8).
3. Coerces locale-dependent decimal commas.
4. Builds a `pandas.DataFrame` indexed by absolute time (`TIME`+`STAMP` parsed to `datetime64[ns]`).
5. Validates that at minimum **groups 003, 008, and 011** are present for any pull-analysis to run; otherwise a friendly error.

### 6.3 Sample-rate reality

EDC15P+ talks **KW1281** (not KWP-2000). VCDS achieves ~3.5–4.5 samples/sec on a single group, ~2/sec on two groups, ~1/sec on three groups. The tool warns the user if the median sample interval exceeds 350 ms and flags affected pulls as `LOW_RATE` (rules R09–R10 are downgraded from `critical` to `warn` in low-rate pulls because SOI transients can be missed).

## 7. Project structure

```
ecu-shenanigans/
├── pyproject.toml
├── README.md
├── LICENSE
├── src/
│   └── ecu_shenanigans/
│       ├── __init__.py
│       ├── app.py                    # PySide6 entrypoint
│       ├── ui/
│       │   ├── main_window.py
│       │   ├── plot_panel.py         # pyqtgraph 4-pane sync
│       │   └── report_view.py
│       ├── ingest/
│       │   ├── vcds_csv.py           # VCDS CSV parser
│       │   └── canonicalize.py       # NNN-K → canonical channels
│       ├── platform/
│       │   └── amf_edc15p/           # SINGLE platform folder
│       │       ├── __init__.py
│       │       ├── channels.py       # canonical channel registry
│       │       ├── stock_refs.py     # §2.2 stock values
│       │       ├── envelope.py       # §5 hard caps
│       │       ├── maps.py           # §2.5 map registry (names + axes only, no .bin)
│       │       └── default_deltas.py # §4.4 default sane deltas
│       ├── rules/
│       │   ├── base.py               # Rule, Finding, Severity pydantic models
│       │   ├── pack_amf.py           # R01–R15
│       │   └── runner.py
│       ├── recommend/
│       │   ├── engine.py             # delta proposal + clamp_to_envelope()
│       │   └── report.py             # Markdown + plots export
│       └── util/
│           ├── timebase.py           # 5 Hz resample
│           └── pulls.py              # WOT-pull detection
├── tests/
│   ├── fixtures/
│   │   ├── vcds_amf_001_003_011.csv
│   │   ├── vcds_amf_008_011.csv
│   │   └── vcds_amf_020_021.csv
│   ├── test_vcds_parser.py
│   ├── test_rules_pack_amf.py        # one test per R01–R15
│   ├── test_envelope_clamp.py
│   └── test_recommend_engine.py
└── docs/
    ├── platform_amf.md               # all of §2 in long form
    └── rules.md                      # R01–R15 with rationales
```

### 7.1 Refactoring steps from current repo state

The agent should execute, in order:

1. **Delete** any/all of: `civic_r18/`, `platform/civic_r18/`, `rules/pack_civic*.py`, `tests/test_civic*.py`, `fixtures/civic*`, any `K-Pro` / `KTuner` / `Hondata` references.
2. **Delete** any multi-platform dispatcher / "platform-selector" UI element. The single platform is hard-wired.
3. **Keep & refactor** any existing ingest code if it already handles VCDS CSV — but rewrite the canonicalization layer to use the §8 channel registry exclusively.
4. **Keep** the `pyqtgraph` plot scaffolding if present; rewire it to the 4-pane layout in §4.2.
5. **Replace** any old generic rule pack with the AMF-specific R01–R15 in `rules/pack_amf.py`.
6. **Add** `platform/amf_edc15p/envelope.py`, `default_deltas.py`, `maps.py`, `stock_refs.py` as new modules.
7. **Add** the entire `tests/fixtures/` directory with at least three VCDS CSV samples (one healthy stock, one with R02 overboost, one with R06 lambda breach).
8. Make sure `pyproject.toml` declares only the AMF entrypoint and that `ecu-shenanigans --help` shows no platform flag.

## 8. Canonical channel registry (only channels actually loggable on EDC15P+/AMF via VCDS)

All downstream code references channels by these snake_case names. The VCDS canonicalizer maps `NNN-K` to these. Channels NOT in this list are not implemented.

| Canonical name | Source group-field | Unit | Notes |
|---|---|---|---|
| `rpm` | 001-1, 003-1, 008-1, 011-1, 020-1 | rpm | Use whichever is present; cross-validate. |
| `iq_actual` | 001-2 | mg/stroke | Idle/cruise IQ; not WOT-IQ. |
| `iq_requested` | 008-2 | mg/stroke | The number that matters at WOT. |
| `iq_limit_rpm` | 008-3 | mg/stroke | RPM-based fuel limit. |
| `iq_limit_maf` | 008-4 | mg/stroke | Smoke-limiter cap. |
| `coolant_c` | 001-4, 004-3 | °C | Used by R11. |
| `battery_v` | 004-2 | V | Sanity. |
| `maf_actual` | 003-3 | mg/stroke | The fueling-decision input. |
| `maf_spec` | 003-2 | mg/stroke | EGR closed-loop target. |
| `egr_duty` | 003-4 | % | Used to confirm EGR closed at WOT. |
| `boost_spec` | 011-2 | mbar abs | LDRXN output. |
| `boost_actual` | 011-3 | mbar abs | The PID-controlled value. |
| `n75_duty` | 011-4 | % | Boost actuator drive. |
| `atm_pressure` | 010-2 | mbar abs | Captured key-on engine-off. |
| `tps_pct` | 010-3 | % | Pedal position proxy. |
| `soi_actual` | 020-2 | ° BTDC | Logged timing. |
| `map_abs` | 020-3 | mbar abs | Cross-check with 011-3. |
| `load_pct` | 020-4 | % | Engine load. |
| `torque_request` | 015-2 | Nm | From driver wish via TL. |
| `torque_actual` | 015-3 | Nm | Modelled actual. |
| `srcv_cyl1`,`_cyl2`,`_cyl3` | 013-1..3 | mg/str | Smooth-running balance (only 3 cyl on AMF — cyl4 column will be empty/zero). |
| `fuel_temp_c` | 013-? (firmware-dependent) | °C | Where exposed. |

Channels we **do not** claim to log (because EDC15P+ doesn't expose them or VCDS can't pull them at usable rate): rail pressure (PD has no rail), wideband λ (no factory wideband), per-injector duty cycle, EGT (no factory pre-turbo thermocouple). The tool models these where needed (lambda from MAF/IQ; EGT from a simple thermal model gated by R10) and labels them clearly as **modelled, not measured**.

## 9. Milestones

This is a **single-platform refinement** project, not a multi-platform expansion. Milestones reflect that.

| # | Milestone | Acceptance |
|---|---|---|
| **M0** | Honda removal | `grep -ri civic\|honda\|k20\|r18 src/ tests/` returns 0 hits. CI green. |
| **M1** | VCDS CSV ingest + canonical channels | Parses all three fixtures; round-trips through `canonicalize.py` with no warnings; mypy strict clean. |
| **M2** | 4-pane plot + WOT-pull detector | Loading the "healthy" fixture identifies ≥3 WOT pulls with sensible bounds. |
| **M3** | Rule pack R01–R15 | Each rule has a unit test triggered by a hand-crafted micro-fixture; golden Markdown report matches. |
| **M4** | Envelope clamper + default deltas table | Fuzz/property test: 10 000 random suggested deltas; none ever exit §5 envelope. |
| **M5** | Markdown + PNG report export | One-click "Analyse" → produces `report_<timestamp>.md` + plots in `out/`. |
| **M6** | Polishing | `ruff`, `black`, `mypy --strict`, `pytest -q` all green. README updated with a worked example. Tag `v2.0.0`. |

## 10. Out of scope (do not implement, do not pretend to implement)

* Reading/writing the ECU bin (no WinOLS/EDCSuite/KESS/KTAG integration).
* Live OBD/KKL communication. The tool reads CSVs offline only.
* Any flashing, EEPROM access, immobiliser bypass, or DTC clearing.
* Any platform other than AMF/EDC15P+. (No Civic R18, no other VAG diesels, no common-rail TDIs, no EDC16/EDC17.)
* Wideband-AFR estimation outside the documented `λ = MAF / (IQ × 14.5)` formula.
* Dyno/power estimation. (You can implement Virtual-Dyno-style RPM-vs-time later, but not in v2.)
* DPF / EGR delete recommendations. The tool will not suggest these.

## 11. Safety, legal, and disclaimer (must appear verbatim in the GUI splash, the CLI banner, and the exported report header)

> **`ecu-shenanigans` is an analysis and educational tool. It does NOT modify your ECU. Any tuning changes are performed at the user's sole risk, on private property only, on a vehicle the user owns. Modifying engine calibration may void your warranty, render the vehicle non-roadworthy, contravene type-approval / emissions regulations in your jurisdiction (e.g. EU Regulation 2018/858, UK MOT diesel smoke limits, US CAA §203), and may damage the engine, turbocharger, clutch, or particulate after-treatment. The "sane Stage 1" envelope encoded in this tool is a conservative community heuristic, not a manufacturer specification. The authors accept no liability. If the tool says BLOCKED — envelope cap, do not work around it.**

---

### Implementation note for Claude Code

When in doubt, prefer correctness and explicit refusal over silent fallback. If a log is missing group 011, the boost-rules section of the report should literally say:

> *Boost analysis SKIPPED — VCDS group 011 not present in this log. Re-log with at least groups 003 + 008 + 011 to enable rules R01–R04.*

…rather than producing degraded, misleading output. Every threshold in this document has a one-line rationale; preserve those rationales as docstrings on the rule classes so they appear in `--help` and in the Markdown report.