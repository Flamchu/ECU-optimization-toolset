# Rule pack — R01..R21 (v4)

Every rule below is a pure function. Each rule's docstring carries its
rationale (per spec §6) so it surfaces in `--help` output and in the
generated Markdown report. Thresholds live in
`platform/amf_edc15p/envelope.rs::CAPS` so a single edit retunes them.

## Severity legend

- `info`     — informational; no action required, but the data may be
                unrepresentative.
- `warn`     — something is off; investigate before tuning further.
- `critical` — a longevity envelope is breached; do not raise.

## Scope

Rules carry a `RuleScope`:

- `PerPull` — evaluated once for each detected WOT pull (R01..R15, R17,
  R18, R20).
- `Global` — evaluated exactly once over the whole log, even if no pull
  was detected (R16, R19, R21). Their findings carry pull id `0`
  (rendered as `G` in the report).

## Rules

| ID | Rule | Threshold | Severity | Scope | Rationale |
|---|---|---|---|---|---|
| R01 | Underboost | `boost_actual < boost_spec − 150` for ≥ 1.0 s above 2000 rpm | warn | per-pull | Persistent underboost: leak, dirty MAF, sticky wastegate, or LDRXN ramp too steep. |
| R02 | Overboost spike | `boost_actual > boost_spec + 200` OR `> 2200 mbar abs` | critical | per-pull | Garrett GT1544S sustained over 2150 mbar pushes shaft past the right edge of the compressor map → overspeed. |
| R03 | Boost target excessive | Any `boost_spec > 2150` mbar abs, or > stock+250 | critical | per-pull | Hard envelope ceiling for Garrett GT1544S longevity. |
| R04 | High-RPM boost not tapering | `boost_spec @ 4500 > boost_spec @ 3000 − 100` | warn | per-pull | Garrett GT1544S is choke-flow-limited above 4000 rpm. |
| R05 | MAF below spec | `MAF_actual < MAF_spec − 8 %` over a pull | warn | per-pull | MAF drift / dirty intake / boost leak — fueling decisions become wrong. |
| R06 | Lambda floor breach | `MAF / (IQ × 14.5) < 1.05` at any sample | critical | per-pull | Below λ = 1.05 = past stoich → incomplete combustion → EGT spike → ring-land cracks. |
| R07 | Peak IQ above sane envelope | `iq_requested > 54 mg/stroke` | critical | per-pull | Above 54 mg/str the PD75 nozzle duration headroom and LUK clutch torque ceiling run out. |
| R08 | Modelled torque above clutch | `iq_requested × 4.4 Nm/mg > 240 Nm` | critical | per-pull | LUK SMF engineering ceiling 240 Nm. |
| R09 | SOI excess | `soi_actual > 26° BTDC` at any IQ ≥ 30 mg | critical (→ warn on LOW_RATE) | per-pull | Beyond 26° BTDC at IQ ≥ 30 mg, peak cylinder pressure migrates ahead of TDC → piston crown stress. |
| R10 | EOI late | `SOI − duration_model > 10° ATDC` | warn | per-pull | Combustion past 10° ATDC dumps unburned heat into the turbine. **No LOW_RATE downgrade** — Warn baseline. Duration is a screening heuristic. |
| R11 | Coolant too low for pull | Coolant < 80 °C during the pull | info | per-pull | EDC15P+ uses cold SOI map below 80 °C — not representative. |
| R12 | Atmospheric correction missing | Group 010 absent / atm_pressure all-NaN | info | per-pull | Without ambient pressure capture, altitude derate can't be assessed. |
| R13 | Fuel temp high | `fuel_temp_c > 80 °C` during pull | warn | per-pull | High fuel temp → lower density → lower delivered IQ for same duration. |
| R14 | Smooth-running deviation | Any cylinder > ±2.0 mg from mean | warn | per-pull | Worn injector cam lobe (PD weak point) or failing injector. |
| R15 | Limp / N75 clamped | `n75_duty` clamped (zero spread) over the pull | warn | per-pull | ECU is in limp mode — log is not valid for tuning. |
| R16 | EGR observed | `max egr_duty > 2 %` anywhere in log | critical | global | v4 mandates a software EGR delete. Any EGR duty means the delete was not flashed, was applied to only one bank, or was overridden by the spec-MAF map polarity. |
| R17 | MAF deviation post-delete | `\|MAF_actual − MAF_spec\| / MAF_spec > 15 %` for >2 s at warm cruise (`coolant ≥ 70 °C`, `pedal_pct < 80 %`) | warn | per-pull | Post-delete cruise MAF should track spec or sit above it. Reads `pedal_pct` (not `tps_pct`); SKIPPED if `pedal_pct` missing. |
| R18 | Cruise SOI NVH | `SOI ≥ 18°` in cruise band (1500–2500 rpm × 5–15 mg) with EGR=0, warm | info | per-pull | Cruise-band SOI unchanged from stock with EGR off; faster premixed phase can raise NVH. Apply −1.0° SOI retard if subjective NVH is objectionable. |
| R19 | DTC scan | Any P0401/P0402/P0403 (Group A) or P0404/P0405/P0406 (Group B) in sidecar `<base>.dtc.txt` | warn | global | Reads from `VcdsLog.dtcs` (sidecar `Vec<String>`), NOT a synthetic float channel. Skipped if no sidecar provided. |
| R20 | Cruise spec-MAF excess | `MAF_actual − MAF_spec ≥ 50 mg` with EGR = 0 | info | per-pull | Strategy-B confirmation: spec-MAF map fill is conservative — soft alert that the delete is functional. (Was R17b in v3.) |
| R21 | Idle stability | RPM σ > 25 over 30-s warm-idle window (`coolant_c ≥ 70`, `pedal_pct < 5`, `vehicle_speed = 0`) | warn (info if window < 30 s OR σ > 15) | global | Catches injector / mech imbalance; replaces the broken R12 reference in the v3 `Idle_fuel` default delta. |

## Skip + downgrade behaviour

- **Group missing** — every rule declares its `requires_groups`. If a
  required VCDS group isn't in the log, the rule emits a SKIPPED Finding
  rather than a partial result.
- **`LOW_RATE` pulls** — the parser flags pulls where the median sample
  interval exceeds 350 ms. R09 downgrades from `critical` to `warn` on
  those pulls because SOI transients are missed at the slow VCDS rate.
  **R10 has no downgrade** because its baseline severity is already Warn.

## Recommendation outcomes

For each row in the §9 default-deltas table (22 rows in v4) the
recommendation engine emits exactly one of:

- `APPLY` — at least one triggering rule fired AND the resulting value
  stays inside the envelope.
- `SKIP` — no triggering rule fired; the row is omitted ("leave stock").
- `BLOCKED — envelope cap` — a rule fired AND the proposed value would
  exit the envelope; the cap that fired is named in the rationale.

The clamper (`platform/amf_edc15p/envelope.rs`) is property-tested with
proptest: 1024 random inputs per `clamp_*` function, none ever escape
the envelope.
