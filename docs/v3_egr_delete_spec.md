# Claude Code Build-Specification — ECU Datalog Analysis Tool **v3 (EGR-Delete Edition)**
**Target platform: Škoda Fabia Mk1 (6Y2) · 1.4 TDI PD AMF · Bosch EDC15P+**
**Repository:** `https://github.com/Flamchu/ECU-optimization-toolset`

---

## 0. Document Purpose & Audience

This is the **third iteration** of the build prompt for the ECU datalog analysis tool. v1 was multi-platform; v2 narrowed to a single platform (AMF + EDC15P+) with a 15-rule sane Stage 1 envelope; **v3 mandates a software-only EGR delete and a smoke-tolerant lambda floor**, because the car is no longer used on public roads — it is a controlled-environment / off-road bench / closed-circuit testbed only.

The reader is **Claude Code**. Every threshold, map name, default delta, channel, and validation step must be explicit and implementable. Do not invent additional platforms, do not generalize, do not produce an abstraction layer for "future ECUs". The codebase must be tightly coupled to **AMF + EDC15P+** with the EGR-delete assumption baked into the rule pack and default deltas.

If anything in this document conflicts with v2, **v3 wins**. The carry-over items (mechanical limits, basic project structure, channel list) are restated here in full so this prompt is self-contained.

---

## 1. Executive Summary

Build a desktop Python application that:

1. Ingests **VCDS measuring-block CSV logs** and **ME7Logger-style line-oriented logs** from a single car: a Škoda Fabia Mk1 6Y2 1.4 TDI PD, engine code **AMF**, ECU **Bosch EDC15P+ (045 906 019 BM, HW 0281 011 412, SW 1039S02900 1166 0178)**.
2. Parses, normalizes, and time-aligns the channels into a single `pandas.DataFrame` with canonical column names.
3. Runs a **fixed 17-rule diagnostic pack** (R01–R15 carried from v2 plus two new EGR-related rules R16 and R17) against the dataset, producing per-rule findings keyed to specific samples.
4. Produces **map-level recommendations** (default deltas) for a sane Stage 1 with **mandatory software EGR delete and smoke-tolerant lambda floor**, expressed as named-map deltas (LDRXN, MLHFM, AGR/EGR-duty, MAF-spec, DTC-thresholds, IQ-by-MAF, IQ-by-MAP, Driver Wish, Torque Limiter, SOI 0–9, Duration 0–5, N75, SVBL, Pilot, Atm, EGT model).
5. Emits a Markdown report and a JSON artefact, plus an optional PySide6 GUI for interactive review (pyqtgraph plots, table view of findings, side-by-side stock vs. recommended map cells).
6. **Validates an EGR-delete flash** via a dedicated post-flash checklist: zero EGR duty in group 003, MAF actual ≈ MAF spec, no P0401–P0406 DTCs, idle stable in group 001, no smoke-limiter excursions inconsistent with the new λ floor.

The tool is **read-only** — it never writes to the ECU. It only reads logs, applies rules, and prints recommended deltas that the user (or their tuner) applies in WinOLS / VAG-EDCSuite / TunerPro.

---

## 2. Platform Deep-Dive (AMF + EDC15P+) — restated and EGR-delete-annotated

### 2.1 Vehicle / Engine

- **Vehicle:** Škoda Fabia Mk1 (6Y2), 5-door hatch / combi, PQ24 platform.
- **Engine code:** AMF — 1.4 TDI PD, **3-cylinder inline**, 1422 cc, 6 valves (2/cyl), DOHC, cast iron block, aluminium head.
- **Power / torque (stock):** 55 kW (75 hp) @ 4000 rpm, 195 Nm @ 2200 rpm.
- **Injection:** Pumpe-Düse (PD) unit injectors, camshaft-actuated, **NOT common rail**. Rail-pressure maps are absent; the EDC15P+ controls injector solenoid timing and duration directly. SOI / EOI / duration calibration matters; "rail pressure" maps in generic EDC15 docs do not apply to AMF.
- **Turbocharger:** **KP35 fixed-geometry, wastegated** (not VNT). N75 solenoid controls wastegate vacuum. Compressor map is small — practical absolute boost ceiling **~2150 mbar** with taper above 4000 rpm. There is no VNT-vane control, so the LDR PID acts on wastegate duty only.
- **Intake:** Bosch HFM5 hot-film MAF upstream of the turbo inlet (cold-side), 70 mm-class housing. EGR loop tees back into the intake post-intercooler (cooled EGR via a small EGR cooler shared with coolant). MAF is therefore upstream of the EGR junction → **MAF measures fresh air only**, and EGR mass is added downstream out of MAF's view. This is the single most important fact for the EGR-delete strategy below.
- **Exhaust:** cast iron manifold, oxidation cat (no DPF for AMF), no NOx sensor, no broadband lambda. There is no closed-loop AFR sensor — the ECU assumes commanded IQ vs. measured MAF and uses a model.
- **Clutch / transmission:** LUK SMF (solid-mass flywheel) with a single-mass clutch, **~240 Nm hard ceiling** before slip and accelerated wear. This is the binding constraint on Stage 1 torque. Do not exceed 240 Nm modelled flywheel torque.
- **Pistons / rods:** AMF shares architecture with the 1.9 PD family; pistons are aluminium with cast bowl, no oil squirters; sustained EGT > 800 °C in-cylinder will crack ring lands within tens of operating hours.

### 2.2 ECU — Bosch EDC15P+

- **Part number:** VAG **045 906 019 BM**.
- **Bosch HW:** **0281 011 412**.
- **SW:** **1039S02900 1166 0178** (the 6-digit "0178" / sub-id is the calibration ID; treat as opaque but log it).
- **Architecture:** Infineon C167-class MCU, dual-bank flash; most maps are mirrored across two banks (typical offset 0x20000 between banks). Any default-delta written to the bin must be applied to **both** parallel banks (WinOLS "parallel maps" function).
- **Memory layout:** Bin is 512 KB (0x80000). Calibration region typically 0x40000–0x7FFFF. Smoke / driver-wish / torque-limiter / EGR maps live in the 0x4Cxxx and parallel 0x6Cxxx ranges; turbo / N75 / boost limiter / SVBL live in 0x51xxx / 0x56xxx with parallel 0x71xxx / 0x76xxx.
- **Smoke-limiter selection:** EDC15P/P+ has a 1-cell switch (commonly at `0x51C30` / `0x71C30`, value `0x00` = MAF-based smoke limiter, `0x101` (257 dec) = MAP-based). On AMF stock this is **MAF-based**. **v3 leaves this switch alone** — see §3.4 for justification.
- **Lambda model:** there is no measured lambda. The ECU computes a *modelled lambda* internally from MAF (fresh air) ÷ commanded IQ × stoichiometric constant for diesel (~14.5:1 mass). This model is what feeds the smoke limiter (IQ-by-MAF / IQ-by-MAP) and the EGT model.

### 2.3 EGR control architecture on EDC15P+ (the v3 focal point)

This is the section Claude Code most needs to internalize. The EGR control loop on EDC15P+ is **NOT** a simple "EGR duty %" lookup. It is a closed-loop PID where:

- **Setpoint:** "specified MAF" (a.k.a. "expected air mass", "MAF spec", `arwMLGRDKF` in some Bosch labels, frequently called the **EGR target air mass map**) — a 2D map of `f(rpm, IQ) → mg/stroke` representing **the MAF reading the ECU expects when EGR is correctly metered in**. Because EGR displaces fresh air, the spec MAF is **lower** than the no-EGR airflow at the same rpm/IQ.
- **Process variable:** MAF actual (Bosch HFM5 reading, 16-bit internal `mshfm_w`, scaled to mg/stroke per the MLHFM linearization curve).
- **Error:** `MAF_actual − MAF_spec`. If positive → not enough EGR is being added → EGR valve must open further → EGR vacuum solenoid duty is increased. If negative → too much EGR → close the valve → reduce duty.
- **Actuator:** EGR vacuum solenoid (an N18-class valve), driven by a duty-cycle PWM. The output of the PID is the duty %, displayed in **VCDS group 003 field 4**.
- **Gating:** EGR is only engaged when (a) coolant temp ≥ ~40 °C, (b) coolant temp ≤ ~100 °C, (c) IQ ≤ ~25 mg/str (i.e. low-load), and (d) RPM in a windowed range (typically idle–~3000 rpm). Above ~3000 rpm or above ~25 mg/str IQ, the spec MAF map is set so high that the PID never demands EGR (i.e. EGR is effectively off at WOT already). Some EDC15P+ files also have a discrete **EGR enable/temperature hysteresis switch** (`$6111C` / similar in EDC15C variants — `Commut. fonc. Commande EGR avec / sans (1/0) hysteresis`); EDC15P+ AMF generally **does not** expose a pure 1/0 EGR enable scalar — the closest analogue is the combined "MAF/MAP/EGR/ASV" switch at `0x51C30/0x71C30`, but flipping that to `0x101` also flips the smoke limiter source from MAF-based to MAP-based, which v3 does **not** want (see §3.4).

**Therefore:** on AMF + EDC15P+, the canonical software EGR delete is a **two-map operation**, not a switch flip:
1. **Set the EGR-duty map (commonly known as "AGR" map, named in WinOLS commentary as `arwMEAB0KL` / `arwMEAB1KL`, address class `0x4C116` + `0x6C116` on PD-family files)** to its maximum constant value (e.g. `850` or `1000` per the calibration's internal 16-bit scaling, meaning "valve 100% closed" in the inverted-logic convention used by VAG EGR, OR `0` if logic is direct — the tool must check stock value direction in the loaded log: if VCDS group 003 shows EGR duty rising at idle on a known stock car, then "high value in map → high duty → valve open"; if the inverse, the convention is inverted). The tool's recommendation is therefore expressed semantically: **"EGR duty = 0% across all axes"** and the map-level fill value is computed from the loaded stock map polarity.
2. **Re-flatten the spec-MAF / expected-air-mass map (`arwMLGRDKF`, address class `0x4D5xx` / `0x6D5xx` on PD AMF; in some files it appears around `0x55096` / `0x75096` or as a paired pair-of-maps in code blocks 2 and 8) to a value ≥ MAF actual at every (rpm, IQ) cell.** A safe default is the **highest cell of the stock spec MAF (typically 850 mg/str)** filled across the whole table. This makes `MAF_actual − MAF_spec` permanently negative or zero, so the PID never asks for EGR even before the EGR-duty map is consulted.

Belt-and-braces: do **both** map changes. Either alone is fragile across thermal / altitude / RPM corners.

### 2.4 DTC monitoring on EDC15P+ for EGR

EDC15P+ monitors EGR via:
- **P0401 (EGR insufficient flow)** — triggered when (commanded EGR duty > X%) AND (MAF actual − MAF spec > threshold) for > Y seconds. After a software delete, EGR duty is 0% so the *commanded EGR > X%* precondition is never met → P0401 *should* never trigger by itself.
- **P0402 (EGR excessive flow)** — symmetric; never triggered post-delete.
- **P0403 (EGR solenoid circuit)** — electrical fault on the EGR solenoid itself (open / short). The solenoid is still wired and pulsed at 0% duty, so this should be quiet, but if the solenoid is later disconnected, P0403 will fire.
- **P0404 (EGR range/performance)** — plausibility check between commanded vs. actual EGR position (only on EDC15s with feedback potentiometer; AMF EGR valve has no position sensor — N18 solenoid is open-loop pressure-modulated — so P0404 is generally absent on AMF).
- **P0405 / P0406 (EGR position sensor low/high)** — same: AMF has no EGR position sensor, so these codes are not expected to be set on stock AMF and are not a real concern post-delete.

**v3 DTC strategy:** because (a) the controlled-environment context permits emissions DTC suppression and (b) most of these codes are inert on AMF anyway, the tool recommends widening the DTC plausibility windows (or zeroing the DTC enable flags in the EDC15P+ DTC table — usually a region near the codeblock header that contains a list of 16-bit DTC entries; CK Decode and similar services touch these). The tool does **not** generate the actual byte patches — it flags which DTCs to suppress and points the user at the DTC list region. It also flags any of P0401/P0402/P0403/P0404/P0405/P0406 actually present in the log as a **delete-not-applied / electrical-fault** finding rather than a benign curiosity.

### 2.5 What physically changes when EGR is software-deleted (no hardware change)

Hardware stays in place: EGR valve, EGR cooler, EGR pipe, vacuum lines, ASV (anti-shudder valve) all remain installed. Software-only delete means the EGR vacuum solenoid is held at 0% duty, so the EGR valve stays mechanically closed because it is **vacuum-actuated and spring-return-to-closed**. With 0 vacuum, the spring closes the valve. No exhaust flow into the intake.

Resulting physical changes the tool must model:

1. **MAF actual rises** at every rpm / IQ point where stock EGR was active. At idle (820 rpm, ~3 mg/str), stock car shows ~180–220 mg/str (with ~25–30% EGR fraction); EGR-off at the same idle should show ~280–320 mg/str. At 2000 rpm cruise / ~10 mg/str IQ, stock ~350–450 mg/str → EGR-off ~500–600 mg/str. At WOT, EGR was already commanded off, so **no MAF change**.
2. **Lambda becomes leaner at idle and cruise.** Diesel idle lambda was already enormous (~λ 5–8) so going to λ 7–10 has no combustion-stability impact in absolute terms. **Combustion temperature rises slightly** because (a) less inert recirculated gas absorbing heat, (b) more O₂ available for full burn. NOx goes way up (irrelevant in controlled-env). Particulates drop (irrelevant — see §3 smoke tolerance).
3. **NVH at low load.** Diesel knock / clatter is partially suppressed by EGR's inert charge slowing the premixed phase of combustion. With EGR off, the premixed phase is slightly faster and louder. Empirically on EA188 PD engines, this is mitigated by **retarding SOI by 0.5–1.5° at low-load cruise (1500–2500 rpm, 5–15 mg/str IQ)**. It is **not** mitigated by reducing IQ — IQ is already minimal at cruise. Pilot injection should be left stock; pilot is the primary NVH tool and stock pilot timing is already tuned for cold/hot operation.
4. **EGT at low load drops slightly** because intake charge is cooler (no hot exhaust mixed in — stock cooled-EGR is still ~150–250 °C at the EGR cooler outlet, vs. fresh charge at ~30–60 °C post-intercooler). At WOT EGT is unchanged because EGR was off already.
5. **Boost target tracking improves slightly at low-load transients** because the EGR valve isn't briefly venting boost at tip-in. Marginal effect on KP35.
6. **No effect on injectors, no effect on rail/pump pressure (PD has no rail), no effect on turbo wastegate hardware.**

### 2.6 Stock baseline numbers (carry-over from v2)

| Quantity | Stock AMF value | Source |
|---|---|---|
| Peak IQ | 44.5 mg/stroke | Stock torque-limiter peak |
| Peak boost (absolute) | ~2000 mbar | Stock LDRXN max @ 2000–3000 rpm |
| Idle rpm | 820 ± 20 rpm | Group 001 |
| Idle MAF spec (with EGR) | 180–220 mg/str | Group 003 |
| Idle MAF actual (with EGR) | 180–220 mg/str (when PID converged) | Group 003 |
| Cruise MAF spec (with EGR, 2000 rpm 10 mg) | 350–450 mg/str | Group 003 |
| Cruise EGR duty | 30–70% | Group 003 field 4 |
| WOT EGR duty | 0–5% | Group 003 field 4 |
| SVBL (single value boost limiter) | 2620 mbar (default EDC15P+ slot) | `0x51C84/0x71C84` |
| Stock SOI peak advance | ~22° BTDC @ 3500 rpm WOT | SOI maps 0–9 |
| Stock duration peak | ~2200 µs equivalent @ 44.5 mg | Duration maps 0–5 |
| Stock N75 duty | 60–90% mid-rpm | `0x56C32/0x76C32` |
| MAF sensor type | Bosch HFM5 | OEM `038906461C` cross-reference |
| MAF max linear range | ~1000 mg/str (sensor saturation point) | HFM5 datasheet |

---

## 3. EGR Delete Strategy (NEW top-level section, replaces nothing in v2)

### 3.1 Scope of the delete

- **Hardware:** unchanged. EGR valve installed, EGR cooler installed, vacuum lines connected, ASV intact, no block-off plate fitted.
- **Software:** EGR duty driven to 0% across the entire (rpm, IQ, T_coolant, atm) domain by zeroing the EGR-duty map AND raising the spec-MAF map ≥ MAF-actual at every cell. This is a redundant pair so the PID never asks for EGR.
- **DTC behaviour:** P0401, P0402, P0403, P0404, P0405, P0406 enable-flags suppressed (or thresholds widened so wide they will not trigger). Tool flags any of these in a post-flash log as evidence the delete was incomplete OR as evidence of a real wiring fault.
- **MAF strategy:** **MAF stays in closed-loop as the primary smoke-limiter input.** The MAF/MAP smoke-limiter switch at `0x51C30/0x71C30` is **NOT** flipped to MAP. Justification: see §3.4.

### 3.2 Why MAF closed-loop, not MAP-based

There is a community convention to flip EDC15P/P+ to MAP-based smoke limiting whenever EGR is deleted, on the theory that "if you mess with MAF, switch to MAP". **v3 explicitly rejects this for AMF** for these reasons, which Claude Code must surface in the report:

1. **MAF is not being deleted.** The HFM5 sensor is fine, in-spec, and correctly placed upstream of the EGR junction. After EGR delete, MAF actual *increases* — but it is still a valid, accurate, linear measurement of intake fresh air. There is no metrological reason to abandon it.
2. **MAP-based smoke limiting on KP35 is coarse.** MAP-based IQ limiting maps `f(rpm, MAP) → max IQ`. KP35 is a wastegate turbo whose MAP at WOT is repeatable and predictable, so MAP-based works *fine* at WOT — but it is a poor guard at part-throttle transients, where MAF responds in milliseconds and MAP lags by 100–300 ms. Part-throttle smoke / overrun tip-in protection degrades.
3. **The IQ-by-MAP map on AMF stock is often blanked / flat at a single high value** (see §6.5 — typical user reports show "60.00 mg flat" for the unused map). Switching to MAP without recalibrating IQ-by-MAP exposes the engine to no smoke limiter at all. v3 keeps MAF-based, where the IQ-by-MAF map is real, calibrated, and well-shaped.
4. **The post-delete MAF reading is well within HFM5 linear range** (worst-case ~600 mg/str at cruise, ~900 mg/str near WOT — sensor saturates ~1000 mg/str). No need to leave MAF behind.
5. **The only thing that needs adjusting on the MAF side is the spec-MAF (EGR target) map**, not the MAF linearization (MLHFM) and not the IQ-by-MAF smoke limiter (other than the v2-recommended Stage 1 expansion + v3 lambda-floor change in §4.4).

So the rule is: **leave the MAF/MAP switch at 0x00 (MAF-based smoke), zero EGR duty, raise spec-MAF, and rescale MLHFM only if the tool detects a calibration mismatch**.

### 3.3 Spec-MAF rescaling math

Goal: at every (rpm, IQ) cell of the spec-MAF map, **spec-MAF ≥ MAF-actual-with-EGR-off** so the PID never demands EGR.

Two acceptable strategies, in order of preference:

**Strategy A (preferred, "data-driven"):** Run a 5–10-minute idle + part-throttle drive log (groups 001 + 003 + 011) on the car **before** flashing the EGR-delete. Capture MAF-actual at the same (rpm, IQ) grid that the spec-MAF map covers. Then *predict* MAF-actual-EGR-off by dividing the observed MAF-actual by `(1 − EGR_fraction_observed)`. Set spec-MAF to **that predicted value × 1.10** (10% margin). This matches reality and lets the tool report a clean "MAF actual vs. spec" plot post-flash.

**Strategy B (preferred when no pre-delete log is available, "max-fill"):** Set every cell of the spec-MAF map to its maximum permitted value, typically **850 mg/str** (the standard EDC15-family ceiling). Forum-confirmed working approach for AMF and the broader 1.9 PD family. Only weakness: the tool can no longer use spec-MAF as a sanity reference post-flash; it just reports "spec-MAF intentionally saturated, EGR PID neutered".

The tool defaults to **Strategy B** because it does not assume a pre-flash log exists. It mentions Strategy A in the report as an option for users who can capture pre-flash data.

### 3.4 Idle and cruise fueling adjustments post-delete

- **Idle (820 rpm, ~3–5 mg/str):** lambda was ~5–7 with EGR, becomes ~8–11 EGR-off. No mechanical issue. NVH may rise marginally because there is more O₂ for the premixed phase. Two options:
  - **Option 1 (default):** leave idle IQ stock, accept the slight NVH change. KP35 is small enough that idle MAF rise is bounded.
  - **Option 2 (if log shows idle instability or rough idle):** reduce idle IQ by 1–2 mg/str (the idle-fueling map / `mrwSTMGRKF`-adjacent slice). Tool only recommends Option 2 if R-rule R09 (idle stability) flags the log.
- **Low-load cruise (1500–2500 rpm, 5–15 mg/str):** apply **−0.5 to −1.5° SOI retard** to the SOI maps in the corresponding cells. This addresses the diesel-knock/NVH that some EA188 PD owners report after EGR delete. Tool default: **−1.0° SOI retard** in the rectangular region (1500–2500 rpm × 5–15 mg/str) of SOI maps 0, 1, 2, 3 (the warm-running maps). Cold-running SOI maps (4–9) untouched.
- **WOT:** unchanged. Stock WOT SOI / Duration / IQ are EGR-off in the stock calibration, so EGR delete adds nothing. v3 still applies the v2 Stage 1 deltas at WOT (more boost, more IQ, etc. — see §4.4).

### 3.5 DTC suppression approach

The tool recommends one of two paths:

1. **Threshold widening (preferred for safety):** find the DTC plausibility / activation threshold table in the EDC15P+ DTC region and raise the MAF-deviation threshold for P0401/P0402 to a value that is unreachable in normal operation (e.g. set the deviation to 9999 mg/str). Same for time-debounce. Effect: the DTC code path still runs but never trips. Real wiring faults that would trip P0403 are still detectable.
2. **DTC enable-flag zero (if user requests cleaner suppression):** zero the DTC entry in the codeblock DTC list. CK Decode / EDC15P+ DTC-Off services do this. Cleaner but loses the ability to detect a genuine solenoid wiring fault.

The tool defaults to option 1, surfacing option 2 as a possibility. It does not generate the byte patches itself — it identifies the symbolic targets (`DTC_P0401_threshold`, `DTC_P0401_debounce`, etc.) and the user's WinOLS / DTC-off service applies them.

---

## 4. Diagnostic Engine — Rules, Channels, Default Deltas

### 4.1 Tech stack (unchanged from v2)

- **Language:** Python ≥ 3.11.
- **Core libs:** `pandas` (DataFrames), `pydantic` v2 (config + finding schemas), `numpy` (numeric), `pyqtgraph` (plots), **PySide6** (GUI; Qt6 LGPL).
- **Packaging:** `pyproject.toml`, `uv` or `pip-tools` lockfile. Editable install for development.
- **Tests:** `pytest`. At least one test per rule + golden-file tests for each parser.
- **Style:** `ruff` + `black` + `mypy --strict` on `platform/` and `rules/`.
- **No web, no DB, no cloud.** Local desktop only.

### 4.2 Channel canonical names (carry-over with EGR delete fields highlighted)

The parser ingests VCDS measuring-block CSV exports and ME7Logger-style CSVs and produces a `pandas.DataFrame` keyed on `t_s` (seconds since log start) with these canonical columns:

| canonical name | unit | source | notes |
|---|---|---|---|
| `t_s` | s | parser | monotonic |
| `rpm` | 1/min | grp 001 / 004 | |
| `iq_mg` | mg/stroke | grp 013 / 015 / 020 | injected quantity, commanded |
| `maf_actual_mg` | mg/stroke | grp 003 f3 | HFM5 measurement |
| `maf_spec_mg` | mg/stroke | grp 003 f2 | what the ECU expects (with EGR — read this carefully post-delete) |
| **`egr_duty_pct`** | % | grp 003 f4 | **must be 0% post-delete; non-zero is a critical finding** |
| `boost_actual_mbar` | mbar abs | grp 011 f3 | |
| `boost_spec_mbar` | mbar abs | grp 011 f2 | |
| `n75_duty_pct` | % | grp 011 f4 / 008 | wastegate solenoid |
| `t_coolant_c` | °C | grp 001 | |
| `t_iat_c` | °C | grp 008 / 010 | post-intercooler ideally |
| `t_fuel_c` | °C | grp 015 | PD fuel temp |
| `atm_mbar` | mbar abs | grp 010 | |
| `soi_deg_btdc` | ° BTDC | grp 013 / 020 | start of injection |
| `eoi_deg_atdc` | ° ATDC | derived | SOI + duration |
| `duration_us` | µs | grp 013 / 020 | injector pulsewidth equivalent |
| `pilot_q_mg` | mg/stroke | grp 013 | |
| `egt_model_c` | °C | grp 031 / model | EDC15P+ has no real EGT sensor → use the modelled value or compute from MAF/IQ/atm |
| `lambda_model` | ratio | derived | `(maf_actual_mg / (iq_mg × 14.5))` |
| `pedal_pct` | % | grp 013 | |
| `vss_kph` | km/h | grp 004 | road speed |
| `gear` | int | derived | |
| **`dtc_codes`** | str list | OBD scan | **post-delete: must be empty of P0401–P0406** |

The parser MUST be defensive: missing channels → set to `NaN`, never raise. The rules MUST handle `NaN` by skipping (a missing channel is "not enough evidence" for that rule, not a failure).

### 4.3 Rule pack (R01–R17)

Rules live in `rules/pack_amf.py`. Each rule is a function `(df: DataFrame, ctx: PlatformCtx) → list[Finding]`. A `Finding` is a pydantic model: `{rule_id, severity, message, samples_index, suggested_action, suggested_map, suggested_delta}`.

Severities: `info`, `warn`, `critical`.

**R01 — Boost ceiling.** `boost_actual_mbar > 2150` for >0.3 s → `critical`. Suggests dropping LDRXN ceiling at the offending rpm cells.

**R02 — N75 saturation.** `n75_duty_pct ≥ 95%` for > 1 s with `boost_actual_mbar < boost_spec_mbar − 100` → `warn`. Indicates the wastegate is fully closed and still can't make spec — KP35 is at its compressor map limit, *not* an N75 problem.

**R03 — Boost overshoot vs. spec.** `boost_actual − boost_spec > +120 mbar` for > 0.5 s → `warn`. Suggests N75 duty trim down at those cells, or PI gain adjustment in `0x56C32`-region.

**R04 — Torque limiter ceiling.** Modelled flywheel torque (computed from IQ × empirical `Nm/mg` constant ≈ 5.4 for AMF) > 240 Nm → `critical`. **240 Nm clutch limit, hard.**

**R05 — Peak IQ ceiling.** `iq_mg > 54` → `critical` (raised from 52 in v2 because smoke is no longer a concern, but still injector-duty-bound). Suggests reducing torque limiter peak.

**R06 — Lambda floor (UPDATED v3).** `lambda_model < 1.05` for > 0.3 s → `critical`. **Was 1.20 in v2; relaxed to 1.05 because smoke is acceptable, but never below stoichiometric** (below 1.0 is past stoich → incomplete combustion → EGT spike → ring-land cracks). Suggests increasing the IQ-by-MAF or IQ-by-MAP cell at the offending (rpm, MAF) point.

**R07 — EGT model ceiling.** `egt_model_c > 800` sustained > 5 s → `critical`. Suggests reducing IQ peak or trimming SOI advance at offending cells.

**R08 — SOI advance ceiling.** `soi_deg_btdc > 26` → `critical`. Piston / ring-land thermal-shock limit.

**R09 — EOI retard ceiling.** `eoi_deg_atdc > 10` → `warn`. Turbine inlet thermal-overload risk; consider trimming Duration map peak.

**R10 — MAF saturation.** `maf_actual_mg > 950` for > 0.2 s → `warn`. HFM5 is approaching saturation; consider larger MAF housing if it persists. (Unlikely on stock AMF + KP35 even after delete.)

**R11 — Atmospheric correction sanity.** `atm_mbar < 700` and no observed IQ rollback → `warn`. EDC15P+ should pull IQ at altitude; if it's not, the atm-correction map is missed.

**R12 — Idle stability.** RPM std-dev at idle > 25 rpm over 30 s window → `warn`. Especially relevant **post-EGR-delete**.

**R13 — Coolant temperature gating.** Boost / IQ at full Stage 1 levels with `t_coolant_c < 80` → `warn`. Cold operation should taper Stage 1 deltas.

**R14 — N75 duty rationality.** `n75_duty_pct` non-monotonic vs. (boost_spec − boost_actual) error → `warn`. Suggests N75 PID is mistuned or duty map shape is wrong.

**R15 — Pedal / Driver Wish saturation.** `pedal_pct ≥ 99` and `iq_mg < 0.95 × torque_limiter_peak_at_rpm` → `info`. Driver Wish is the binding constraint, not torque limiter — increase Driver Wish at high-pedal cells.

**R16 — EGR duty observed (NEW v3).** `max(egr_duty_pct) > 2%` over the entire log → **`critical`**. Message: "EGR delete not applied in software — observed EGR duty %s at sample %d (rpm %d, IQ %.1f, T_coolant %d°C). Verify (a) EGR-duty map flashed to 0% across all banks, (b) spec-MAF map flashed ≥ MAF actual across all cells. See §3 for procedure." Suggested map: `AGR / arwMEAB0KL+arwMEAB1KL` and `arwMLGRDKF`.

**R17 — MAF deviation post-delete (NEW v3).** Outside cold-start (t_coolant ≥ 70°C) and outside WOT (pedal < 80%): `|maf_actual_mg − maf_spec_mg| / maf_spec_mg > 0.15` for > 2 s → `warn`. Message: "MAF actual deviates >15% from MAF spec at cruise. Spec-MAF map likely not rescaled for EGR-off conditions. Re-flatten arwMLGRDKF to ≥850 mg/str across all cells, or rescale per Strategy A in §3.3."

**(Sub-rule R17b)** If `maf_actual_mg > maf_spec_mg + 50` at cruise AND `egr_duty_pct == 0` AND no DTC P0401/P0402 → `info`. Message: "MAF actual exceeds spec by %d mg/str — expected and harmless after EGR delete with spec-MAF intentionally saturated; verifies delete is functional."

**R18 — Cruise SOI NVH check (NEW v3, optional/info).** In cruise band (1500–2500 rpm, 5–15 mg/str, t_coolant ≥ 80°C, pedal ≤ 30%): if `soi_deg_btdc` is at or above the v2 stock value within ±0.2°, AND `egr_duty_pct == 0` → `info`. Message: "Cruise SOI is unchanged from stock with EGR off. May exhibit increased diesel-knock NVH. v3 default delta is −1.0° SOI retard in this cell band. Apply only if subjective NVH is objectionable."

**R19 — DTC scan check (NEW v3).** If any of {P0401, P0402, P0403, P0404, P0405, P0406} present in `dtc_codes` → `warn`. Distinguish P0403 (electrical fault on the still-installed solenoid — investigate wiring) from P0401/P0402 (delete not fully suppressed at the threshold/enable level — apply the §3.5 patch). P0404/P0405/P0406 on AMF is unusual and should be flagged for investigation regardless.

(The numbering went R16, R17, R17b, R18, R19 — total of **17 distinct rules** as advertised; R16/R17/R18/R19 are the EGR-delete-specific additions.)

### 4.4 Default deltas table (the recommendation engine's output for AMF + EDC15P+ + sane Stage 1 + EGR delete + smoke-tolerant)

Each row is a recommended change to a named map. Banks always parallel.

| # | Map | Address class (PD-family typical, verify per-file) | Stock | v3 recommended delta | Rationale |
|---|---|---|---|---|---|
| 1 | **EGR duty (`AGR` / `arwMEAB0KL` + `arwMEAB1KL`)** | `0x4C116` + `0x6C116` | f(rpm, IQ) → duty% | **All cells = 0% (i.e. valve-closed-fill, polarity per loaded file: typically value `850` or max-of-stock-map)** | Mandatory delete — primary actuator path |
| 2 | **Spec-MAF / Expected Air Mass (`arwMLGRDKF`)** | `0x4D5xx`/`0x6D5xx` (PD AMF; verify) | f(rpm, IQ) → mg/str | **All cells = 850 mg/str** (Strategy B) | Mandatory delete — neuters PID setpoint |
| 3 | **DTC threshold table (P0401, P0402)** | DTC region in codeblock header | small thresholds | **Widen to 9999 mg/str / 9999 s debounce**, OR zero the DTC enable flags | Suppress emissions DTCs in controlled-env |
| 4 | **MAF/MAP smoke switch** | `0x51C30` + `0x71C30` | `0x00` (MAF-based) | **UNCHANGED — leave `0x00`** | v3 keeps MAF closed-loop (§3.2) |
| 5 | **MLHFM (MAF linearization curve)** | calibration curve in MAF region | stock HFM5 curve | **UNCHANGED unless R10 trips** | HFM5 in-spec, no need to touch |
| 6 | **LDRXN (boost target / "turbo map")** | `0x56926` + `0x76926` | peak ~2000 mbar | **+150 mbar in 2000–3500 rpm cells; cap at 2150 mbar** (v2 carry-over). Optional +50 mbar more if R02 shows headroom | KP35 ceiling |
| 7 | **LDOLLR / LDRPMX (boost limiter)** | `0x56F1C` + `0x76F1C` | ~2200 mbar | **Set to 2200 mbar** (≥ LDRXN+50) | Hard limiter above target |
| 8 | **SVBL (single value boost limit)** | `0x51C84` + `0x71C84` | 2620 mbar | **UNCHANGED** | EDC15P+ default headroom |
| 9 | **Driver Wish (`mrwFVH_KF`)** | `0x4D20E` + `0x6D20E` | f(rpm, pedal%) → IQ | **+5 mg/str at pedal ≥ 80% in 1800–3500 rpm** | Lets pedal reach Stage 1 IQ |
| 10 | **Torque Limiter (`mrwBDB_KF`)** | `0x4D8D4` + `0x6D8D4` | peak ~44.5 mg | **+9.5 mg/str → 54 mg peak in 2000–3000 rpm** (v3 raised from 52) | Clutch-limited; smoke removed |
| 11 | **IQ-by-MAF smoke limiter** | `0x4DBF6` (single bank, coolant-dependent on PD-family; v2 mapped this) | shaped curve | **Re-shape so λ floor = 1.05** at every (rpm, MAF) cell — i.e. allow IQ up to `MAF / (1.05 × 14.5)` | v3 lambda floor relaxed |
| 12 | **IQ-by-MAP smoke limiter (backup)** | secondary smoke map | mostly flat 60 mg | **Re-shape mirrored to IQ-by-MAF, λ floor 1.05** | belt-and-braces |
| 13 | **SOI maps 0, 1, 2, 3 (warm-running)** | `0x...` x 4 + parallel | stock advance | **−1.0° in 1500–2500 rpm × 5–15 mg/str cells** (NVH) | EGR-off NVH mitigation |
| 14 | **SOI maps 4–9 (cold/transient)** | `0x...` | stock | **UNCHANGED** | Cold-start integrity |
| 15 | **Duration maps 0–5** | `0x54656, 0x548DC, 0x54B62, 0x54DE8, …` + `0x74…` parallel | stock | **Extend axis to 60 mg/str** (v2 carry-over) and recalibrate pump-voltage by same factor | Lets new IQ peak fire correctly |
| 16 | **N75 duty (`ldwTV_KF`)** | `0x56C32` + `0x76C32` | shaped | **−3% low-rpm low-load; +2% high-rpm high-load** | Smoother boost build-up under new LDRXN |
| 17 | **Pilot injection (`zmwP_KF_P0..5`-pilot)** | pilot region | stock | **UNCHANGED** | Stock pilot best for NVH |
| 18 | **Atmospheric correction** | atm-correction map | stock | **UNCHANGED unless R11 trips** | |
| 19 | **EGT model coefficients** | EGT model region | stock | **UNCHANGED** | Tool only reads, never modifies the model |
| 20 | **Idle fueling (`mrwSTMGRKF`-adjacent slice)** | idle map region | stock idle IQ | **UNCHANGED by default; −1.5 mg/str at idle only if R12 trips** | Conditional |
| 21 | **Cold-start IQ / advance** | start charts | stock | **UNCHANGED** | |

**Map address caveat:** All addresses listed are **typical for the EDC15P/P+ PD-family** (especially the 1.9 ARL / ASZ class, where they are best-documented); for AMF specifically the offsets may shift by ± a few hundred bytes between SW versions. The tool MUST verify per-file by signature search (e.g. find the EGR-duty map by signature: a 13×16 or similar grid where row = rpm 0..3500, column = IQ 0..30 mg, values monotonic in the 0..1000 range — VAGEDCSuite / WinOLS damos do this automatically). The tool emits the **symbolic delta** (e.g. "EGR-duty map: zero all cells in both banks") and lets the user resolve the byte address.

---

## 5. Hard Envelope (v3 — final)

These are absolute caps the recommendation engine never crosses, regardless of log evidence:

| Quantity | v2 cap | **v3 cap** | Reason |
|---|---|---|---|
| Peak boost (absolute) | 2150 mbar | **2150 mbar** | KP35 compressor map ceiling |
| Boost limiter (LDRPMX) | 2200 mbar | **2200 mbar** | LDRXN + 50 |
| SVBL | 2620 mbar | **2620 mbar (untouched)** | EDC15P+ default |
| Peak IQ | 52 mg/str | **54 mg/str** | Injector-duty + clutch; smoke removed |
| Lambda floor | 1.20 | **1.05** | Smoke-tolerant; never below stoich |
| EGT (modelled) | 800 °C | **800 °C** | Piston / turbine thermal limit |
| SOI advance | 26° BTDC | **26° BTDC** | Ring-land thermal-shock limit |
| EOI retard | 10° ATDC | **10° ATDC** | Turbine inlet thermal limit |
| Modelled flywheel torque | 240 Nm | **240 Nm** | LUK SMF clutch hard limit |
| MAF actual | 1000 mg/str | **1000 mg/str** | HFM5 saturation |
| **EGR duty (NEW)** | n/a | **0% in all recommended maps** | Mandatory software delete |
| **Spec-MAF (NEW)** | n/a | **≤ MAF actual at every cell (default 850 mg/str fill)** | Neuter EGR PID |

The tool MUST refuse to emit a delta that would cross any of these. If a log presents evidence that a higher cap is "achievable" (e.g. EGT consistently 750 °C at WOT, suggesting headroom), the tool reports "headroom available, but caps not raised — change envelope manually if you accept the risk".

---

## 6. VCDS Specifics — group emphasis with EGR delete validation

### 6.1 Required groups for AMF + EDC15P+

| Group | Fields (typical) | Purpose | EGR-delete relevance |
|---|---|---|---|
| **001** | rpm, vss, t_coolant, throttle | idle / coolant / vss baseline | **Idle stability post-delete (R12)** |
| **003** | rpm, MAF spec, MAF actual, **EGR duty** | EGR closed loop | **Primary delete validation channel — R16/R17** |
| **004** | rpm, vss, t_coolant, t_oil | thermal | gating context |
| **008** | rpm, IAT, MAP-related | intake temp / map | atm + IAT |
| **010** | atm pressure, alt | altitude correction | R11 |
| **011** | rpm, boost spec, boost actual, N75 duty | boost loop | R01–R03, R14 |
| **013** | IQ, pedal, SOI, pilot, duration | fueling | R04, R05, R08, R09, R15, R18 |
| **015** | IQ, fuel temp, smoke limit active | smoke limiter snapshot | R06 |
| **020** | IQ, SOI, MAF, EGT model | combined | R06, R07, R08 |
| **031** | EGT model, smoke status | EGT | R07 |

### 6.2 Group 003 specifically

The 4 standard fields on EDC15P+ for VAG TDI:

1. **rpm** — engine speed
2. **MAF specified** (mg/str or mg/H — VCDS may report mg/H for older labels; tool normalizes to mg/str)
3. **MAF actual** (HFM5 measurement)
4. **EGR vacuum-solenoid duty cycle %**

**Acceptance criterion for an EGR delete:** at warm idle (t_coolant ≥ 80 °C, after the standard ~2 min EGR-active phase), field 4 must read **0% (or ≤ 2% within sensor noise)** continuously, AND fields 2 and 3 should be close (within 15%) — but if Strategy B was used (spec-MAF flat at 850), field 2 will read 850 mg/str and field 3 will read whatever HFM5 measures (~280–320 idle, ~500–600 cruise, ~900 WOT). That is **expected and correct**; R17b reports it as info.

### 6.3 Group 001 idle stability post-delete

Capture 60 s of warm idle (t_coolant ≥ 85 °C, A/C off, neutral). RPM std-dev ≤ 25 rpm is acceptable (R12). If exceeded, recommend the conditional idle-IQ trim from §3.4 / row 20 of §4.4.

### 6.4 Group 010 atmospheric

Atmospheric pressure check before the log starts. If atm < 950 mbar, flag the dataset as "altitude-affected" and let R11 reason about it.

### 6.5 What VCDS will NOT show

- Real EGT (no EGT sensor on AMF — only modelled).
- Real lambda (no broadband sensor).
- Real EGR position (N18 valve has no position pot).
- Smoke output (no PM sensor).

The tool is therefore inherently model-bound for EGT, λ, smoke. R06, R07 use modelled values; uncertainty must be acknowledged in the report header.

---

## 7. EGR Delete Validation Checklist (NEW dedicated section)

This is what the tool runs against a **post-flash log** to verify the delete was applied correctly. Implement as a single function `validate_egr_delete(df) → ValidationReport` that returns pass/fail per item.

| # | Check | Pass criterion | Fail action |
|---|---|---|---|
| 1 | EGR duty zero at idle | `egr_duty_pct ≤ 2%` for all warm-idle samples (>3 min after start, t_coolant≥80°C, IQ<8 mg/str) | Re-flash EGR-duty map |
| 2 | EGR duty zero at cruise | `egr_duty_pct ≤ 2%` for all cruise samples (1500–2500 rpm, 5–15 mg/str, warm) | Re-flash EGR-duty map; check both banks |
| 3 | EGR duty zero at WOT | `egr_duty_pct ≤ 2%` for all pedal>80% samples | Should already be zero pre-delete; if not, raise spec-MAF further |
| 4 | Spec-MAF saturated (Strategy B used) | `maf_spec_mg ≥ 800` for all warm samples | Re-flash spec-MAF map; both banks |
| 5 | MAF actual within HFM5 range | `100 ≤ maf_actual_mg ≤ 950` for all warm samples | If saturating, larger MAF housing or check intake leak |
| 6 | No P0401 / P0402 in DTC scan | scan returns clean | Widen DTC threshold (§3.5) or zero DTC enable flag |
| 7 | No P0403 in DTC scan | scan returns clean | **Wiring fault on still-installed solenoid — investigate, do NOT just suppress** |
| 8 | No P0404 / P0405 / P0406 | scan returns clean | Unusual on AMF — investigate before suppressing |
| 9 | Idle stable | RPM σ ≤ 25 rpm over 60 s warm idle | Apply conditional idle-IQ trim (row 20) |
| 10 | Cruise NVH proxy | log marker / driver note "cruise feels OK" | If complaint, apply −1.0° SOI retard cruise band |
| 11 | EGT modelled within env | `egt_model_c < 800 °C` sustained | If above, IQ peak too high or SOI too advanced for EGR-off charge |
| 12 | Lambda model within env | `lambda_model ≥ 1.05` sustained | If below, smoke-limiter (IQ-by-MAF) re-shape needs revisit |
| 13 | Boost actual ≤ envelope | `boost_actual_mbar ≤ 2150` peak | If above, LDRXN / LDRPMX too aggressive — drop |
| 14 | No torque limiter trip | modelled torque ≤ 240 Nm | If above, torque limiter map reduce |
| 15 | MAP/MAF switch unchanged | `0x51C30/71C30 == 0x00` (verify by reading bin if available) | Switch NOT supposed to flip in v3 |

Output: a markdown checklist with pass/fail glyphs and per-item evidence (sample indices, observed values).

---

## 8. Project Structure

```
ECU-optimization-toolset/
├── pyproject.toml
├── README.md
├── LICENSE
├── docs/
│   ├── v3_egr_delete_spec.md     # this document
│   ├── platform_amf_edc15p.md
│   └── envelope.md
├── src/
│   └── ecu_opt/
│       ├── __init__.py
│       ├── cli.py                 # argparse: analyze, validate-egr-delete, gui
│       ├── parsers/
│       │   ├── __init__.py
│       │   ├── vcds_csv.py
│       │   ├── me7logger_csv.py
│       │   └── normalize.py       # canonical column mapper
│       ├── platform/
│       │   └── amf_edc15p/
│       │       ├── __init__.py
│       │       ├── channels.py    # canonical names + group mapping
│       │       ├── stock_refs.py  # stock baseline numbers (§2.6)
│       │       ├── envelope.py    # hard caps (§5)
│       │       ├── maps.py        # symbolic map names + addr classes (§4.4)
│       │       ├── default_deltas.py  # the 21-row table from §4.4
│       │       └── egr.py         # NEW v3: EGR delete strategy + validation
│       ├── rules/
│       │   ├── __init__.py
│       │   ├── pack_amf.py        # R01–R19
│       │   └── findings.py        # pydantic Finding model
│       ├── validate/
│       │   ├── __init__.py
│       │   └── egr_delete.py      # the §7 checklist function
│       ├── report/
│       │   ├── __init__.py
│       │   ├── markdown.py
│       │   └── json_artifact.py
│       └── gui/
│           ├── __init__.py
│           ├── main_window.py     # PySide6
│           └── plots.py           # pyqtgraph
└── tests/
    ├── parsers/
    ├── rules/
    ├── validate/
    └── golden/
        └── amf_edc15p/
            ├── stock_baseline.csv
            ├── stage1_pre_delete.csv
            └── stage1_post_delete.csv  # used by validate_egr_delete tests
```

`platform/amf_edc15p/egr.py` exposes:
- `EGR_DUTY_MAP_NAME = "AGR_arwMEAB0KL"` (symbolic)
- `SPEC_MAF_MAP_NAME = "arwMLGRDKF"`
- `DTC_LIST_TO_SUPPRESS = ["P0401", "P0402", "P0403", "P0404", "P0405", "P0406"]`
- `IDLE_INSTABILITY_THRESHOLD_RPM_STD = 25`
- `CRUISE_BAND = (1500, 2500, 5, 15)`  # rpm_lo, rpm_hi, iq_lo, iq_hi mg/str
- `CRUISE_SOI_RETARD_DEG = 1.0`
- `SPEC_MAF_FILL_MGSTR = 850`  # Strategy B
- functions: `recommend_egr_delete_deltas() -> list[MapDelta]`, `validate_egr_delete(df) -> ValidationReport`, `predict_maf_no_egr(df_stock) -> Series`  # for Strategy A.

---

## 9. CLI

```
ecu-opt analyze <log.csv> [--platform amf_edc15p] [--report report.md] [--json findings.json]
ecu-opt validate-egr-delete <post_flash_log.csv>
ecu-opt gui
ecu-opt deltas --platform amf_edc15p --stage 1 --egr-delete --smoke-tolerant
```

`deltas` emits a markdown table identical in shape to §4.4 row-for-row, plus the symbolic targets resolved against any provided `.bin` (if the user passes `--bin path/to/firmware.bin` the tool runs signature search to resolve symbolic names to byte addresses; otherwise it prints "<resolve manually>").

---

## 10. Repository State (Flamchu/ECU-optimization-toolset)

The new repo `https://github.com/Flamchu/ECU-optimization-toolset` (which **replaces** the old `ecu-shenanigans` repo) is the canonical home for v3. Treat it as a greenfield checkout of the v2 codebase: any v2 modules carry over verbatim, and v3 deltas are applied as additive commits:

- **Add:** `platform/amf_edc15p/egr.py`, `validate/egr_delete.py`, the §7 checklist test fixtures.
- **Modify:** `rules/pack_amf.py` (add R16, R17, R17b, R18, R19; modify R05, R06 thresholds), `platform/amf_edc15p/envelope.py` (raise IQ cap to 54, lower λ floor to 1.05, add EGR-duty and spec-MAF caps), `platform/amf_edc15p/default_deltas.py` (add EGR + spec-MAF + DTC + idle + cruise-SOI rows), `report/markdown.py` (new "EGR Delete Strategy" section + new validation-checklist section), `cli.py` (new `validate-egr-delete` subcommand), README (update title to "v3 EGR-delete edition", legal section per §11).
- **Unchanged:** parsers, GUI shell, channels.py, stock_refs.py (those numbers are physical baselines), `maps.py` symbol table (extended, not replaced).

If the repo currently has only a README or a minimal scaffold, treat the entire v2 spec as "to be implemented" and apply v3 on top in a single coherent commit set.

---

## 11. Legal / Safety Disclaimer (v3 — controlled-environment context)

**Reduced emissions/legal disclaimer (v3):** This tool targets a vehicle that is **not registered for road use, not subject to periodic emissions testing, and used only in a controlled environment** (closed circuit / test cell / off-road). Emissions DTC suppression and software EGR delete are therefore acceptable in this context. **The user assumes all responsibility for ensuring the vehicle is not driven on public roads.**

**Mechanical / safety disclaimers retained in full (v3):**
- The 240 Nm flywheel-torque cap is a clutch limit. Exceeding it accelerates clutch wear and risks slip → driveline shock.
- The 2150 mbar boost cap is a KP35 compressor-map limit. Exceeding it overspeeds the turbocharger → bearing failure / shaft breakage.
- The 800 °C EGT cap is a piston ring-land / turbine inlet thermal limit. Sustained operation above it cracks pistons within tens of operating hours and degrades the turbine within hundreds.
- The 1.05 lambda floor is a *physical* combustion limit, not a smoke limit. Below stoichiometric, combustion is incomplete → EGT spike → multiple-component thermal damage.
- The 26° BTDC SOI cap is a piston thermal-shock limit.
- This is a Stage 1 calibration **assuming OE injectors, OE turbo (KP35), OE clutch (LUK SMF), OE pistons, OE timing belt in good service.** Any hardware change invalidates the envelope.
- The tool does NOT write to the ECU. Flashing a recommended delta is the user's responsibility, performed with their own tooling, with verified bin checksum, and with bench-flash recovery available.
- Recommendations are derived from log evidence and physical models; logs from a faulty sensor (e.g. failing HFM5) will produce wrong recommendations. Sensor health must be verified before trusting deltas.

---

## 12. Milestones

| M | Title | Deliverable | EGR-delete relevance |
|---|---|---|---|
| M1 | Parser + canonical schema | VCDS + ME7Logger CSV → DataFrame; tests | adds `egr_duty_pct`, `dtc_codes` cols |
| M2 | Platform module | `amf_edc15p/{channels,stock_refs,envelope,maps}` populated; tests | envelope includes v3 caps |
| M3 | Rule pack v3 (R01–R19) | `rules/pack_amf.py`; per-rule unit tests | R16, R17, R17b, R18, R19 |
| M4 | Default-deltas table v3 | `default_deltas.py` returns the 21-row table from §4.4 | EGR + spec-MAF + DTC + idle + cruise-SOI rows |
| M5 | EGR delete module | `platform/amf_edc15p/egr.py`; recommend + predict_maf + validate | new module |
| M6 | **EGR-delete validation milestone (NEW)** | `validate/egr_delete.py`; CLI subcommand; checklist markdown emitter; golden post-flash test fixture | the §7 checklist |
| M7 | Markdown + JSON report | report/ pipeline | EGR-delete section + checklist section |
| M8 | GUI | PySide6 main window + pyqtgraph plots | overlay of MAF actual vs MAF spec; EGR duty heatmap |
| M9 | End-to-end | golden log → final report; reproducible | covers v3 happy path |
| M10 | Docs + README | docs/v3_egr_delete_spec.md (this file); updated README | controlled-env disclaimer |

---

## 13. Acceptance Criteria

The v3 build is complete when ALL of the following hold:

1. `ecu-opt analyze tests/golden/amf_edc15p/stage1_post_delete.csv` produces a markdown report containing exactly the sections: Summary, Platform, **EGR Delete Strategy**, Findings (R01..R19), Default Deltas (21 rows), Hard Envelope, **EGR Delete Validation Checklist (15 items)**, Legal/Safety, Appendix-channels.
2. The 21-row Default Deltas table contains the EGR-duty row (0%), the spec-MAF row (≥850), the DTC suppression row (P0401–P0406), the idle row (conditional), and the cruise-SOI row (−1.0°).
3. The Hard Envelope shows λ ≥ 1.05 (not 1.20), IQ ≤ 54 (not 52), EGR duty = 0%, spec-MAF ≤ MAF-actual, plus the unchanged 2150 mbar / 800 °C / 240 Nm / 26° BTDC / 1000 mg/str caps.
4. `ecu-opt validate-egr-delete tests/golden/amf_edc15p/stage1_post_delete.csv` exits 0 and prints a 15-item checklist with all PASS.
5. `ecu-opt validate-egr-delete tests/golden/amf_edc15p/stage1_pre_delete.csv` exits non-zero (R16 critical: EGR duty observed > 0%).
6. R17b correctly distinguishes "MAF actual exceeds spec by N mg/str — expected after delete" from "MAF actual deviates from spec — delete incomplete".
7. The MAF/MAP smoke-limiter switch (row 4 of §4.4 table) is documented as **explicitly unchanged**, with the §3.2 justification embedded in the report.
8. The hardware-stays-installed assumption is stated in the report header (no block-off plate, no mechanical EGR removal).
9. The repo URL `https://github.com/Flamchu/ECU-optimization-toolset` is referenced everywhere — README, CLI banner, report footer.
10. `mypy --strict src/ecu_opt/{platform,rules,validate}` passes.
11. `pytest -q` passes including the post-flash golden-log test.

---

## 14. Open Questions / Caveats

- **Address class precision.** Map addresses listed in §4.4 are PD-family typicals (best-documented on 1.9 ARL/ASZ/AVF). For the 3-cylinder AMF variant of the EDC15P+ family, offsets shift by a small amount per-SW-version. The tool's `default_deltas.py` MUST emit symbolic targets and, when given the bin, resolve via signature search rather than hard-coded offsets. The user must verify on their specific 1039S02900 1166 0178 file.
- **EGR-duty map polarity.** Some PD calibrations use "high value = valve open" (set to 0 for delete); others (the more common "MEAB" convention) use "high value = airflow setpoint, low value = more EGR" (set to max for delete). The tool's recommendation is **semantic**: "EGR duty = 0% across all conditions". The byte-level fill (0 vs. 850 vs. 1000) is determined by checking the loaded stock map's polarity — if higher cells correlate with idle/cruise (where EGR is open), polarity is inverted-fill = max; if higher cells correlate with WOT (where EGR is closed), polarity is direct-fill = 0. The tool reports the inferred polarity in its output.
- **Strategy A vs. B for spec-MAF.** Default is B (saturate at 850). Strategy A (data-driven rescaling using a pre-flash log) is implemented in `egr.predict_maf_no_egr()` but not used by default.
- **DTC suppression byte-level.** Tool flags symbolic targets only. Actual byte patches require either a DTC-off service (CK Decode etc.) or a damos/A2L for the specific 1039S02900 1166 0178 SW. The README must list this caveat.
- **EGT and lambda are modelled, not measured.** AMF has no EGT sensor and no broadband λ. Rules R06 and R07 operate on modelled values; their accuracy is bounded by the fidelity of the EDC15P+ EGT/λ model and by the accuracy of MAF and IQ. This is stated explicitly in the report header.
- **No claim of road legality.** v3 explicitly assumes controlled-environment use. Deploying this calibration on a road-registered vehicle is the user's responsibility and outside the tool's scope.
- **Repo state at time of writing.** The repo `https://github.com/Flamchu/ECU-optimization-toolset` may currently be empty or scaffold-only — the v3 spec assumes a greenfield implementation that absorbs the v2 design verbatim plus the EGR-delete additions. Claude Code should not attempt to fetch the repo URL during the build (web access unnecessary); all required context is in this prompt.

---

**End of v3 build specification.** All thresholds, map names, default deltas, and validation steps in this document are explicit and implementable. Claude Code may proceed.