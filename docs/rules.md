# Rule pack — R01..R15

Every rule below is a pure function `predicate(df, pull) -> list[Finding]`.
Each rule's docstring carries its rationale (per spec §10) so it surfaces in
`--help` output and in the generated Markdown report. Thresholds live in
`platform/amf_edc15p/envelope.py:CAPS` so a single edit retunes them.

## Severity legend

- `info`     — informational; no action required, but the data may be
                unrepresentative.
- `warn`     — something is off; investigate before tuning further.
- `critical` — a longevity envelope is breached; do not raise.

## Rules

| ID | Rule | Threshold | Severity | Rationale |
|---|---|---|---|---|
| R01 | Underboost | `boost_actual < boost_spec − 150` for ≥ 1.0 s above 2000 rpm | warn | KP35 PID can't keep up — leak, sticky wastegate, or LDRXN ramp too steep. |
| R02 | Overboost spike | `boost_actual > boost_spec + 200` OR `> 2200 mbar abs` | critical | Sustained > 2150 mbar pushes the KP35 shaft past the right edge of the compressor map. |
| R03 | Boost target excessive | Any `boost_spec > 2150` mbar abs (sea-level), or > stock+250 | critical | Hard envelope ceiling for KP35 longevity. |
| R04 | High-RPM boost not tapering | `boost_spec @ 4500 > boost_spec @ 3000 − 100` | warn | KP35 is choke-flow-limited above 4000 rpm. |
| R05 | MAF below spec | `MAF_actual < MAF_spec − 8 %` over a pull | warn | MAF drift / dirty intake / boost leak — fueling decisions become wrong. |
| R06 | Lambda floor breach | `MAF / (IQ × 14.5) < 1.20` at any sample | critical | Below λ = 1.20 PD smokes + EGT spike. Hard physics floor 1.05; we keep 0.15 margin. |
| R07 | Peak IQ above sane envelope | `iq_requested > 52 mg/stroke` | critical | Above 52 mg the LUK clutch + stock injectors run out of headroom. |
| R08 | Modelled torque above clutch | `iq_requested × 4.4 Nm/mg > 240 Nm` | critical | LUK SMF clutch ceiling. |
| R09 | SOI excess | `soi_actual > 26° BTDC` at any IQ ≥ 30 mg | critical | Beyond 26° BTDC peak cylinder pressure migrates ahead of TDC → piston-crown stress. |
| R10 | EOI late | `SOI − duration_model > 10° ATDC` | warn | Combustion past 6–10° ATDC dumps unburned heat into the turbine. Duration is modelled. |
| R11 | Coolant too low for pull | Coolant < 80 °C during the pull | info | EDC15P+ uses cold SOI map below 80 °C — not representative. Re-do the pull. |
| R12 | Atmospheric correction missing | Group 010 absent / atm_pressure all-NaN | info | Without ambient pressure capture, altitude derate can't be assessed. |
| R13 | Fuel temp high | `fuel_temp_c > 80 °C` during pull | warn | High fuel temp → lower density → lower delivered IQ for same duration. |
| R14 | Smooth-running deviation | Any cylinder > ±2.0 mg from mean | warn | Worn injector cam lobe (PD weak point) or failing injector. |
| R15 | Limp / DTC interruption | `n75_duty` clamped (zero spread) over the pull | warn | ECU is in limp mode — log is not valid for tuning. |

## Skip + downgrade behaviour

- **Group missing** — every rule declares its `requires_groups`. If a
  required VCDS group isn't in the log, the rule emits a SKIPPED Finding
  per pull rather than a partial result.
- **`LOW_RATE` pulls** — the parser flags pulls where the median sample
  interval exceeds 350 ms. R09 and R10 downgrade from `critical` to `warn`
  on those pulls because SOI transients are missed at the slow VCDS rate.

## Recommendation outcomes

For each row in the §4.4 default-deltas table the recommendation engine
emits exactly one of:

- `APPLY` — at least one triggering rule fired AND the resulting value
  stays inside the envelope.
- `SKIP` — no triggering rule fired; the row is omitted ("leave stock").
- `BLOCKED — envelope cap` — a rule fired AND the proposed value would
  exit the envelope; the cap that fired is named in the rationale.

The clamper (`platform/amf_edc15p/envelope.py`) is property-tested with
hypothesis: 21 200 random inputs across all clamp_* functions, none ever
escape the envelope.
