# DAMOS pointers — open questions for the operator

The build emits **symbolic recommendations only**. Map names follow the
public EDC15P+ DAMOS / WinOLS / EDC15P Suite tradition; concrete byte
addresses are deliberately not pinned in the tool because they are
file-version-specific and would silently corrupt non-AMF binaries if
hard-coded.

When applying the recommendations the operator must locate every map on
their *actual* `045 906 019 BM` binary via a DAMOS / map-pack pass in
WinOLS. The tool flags the following items as not blockers, but worth
the operator's attention.

## 1. AMF-specific map byte addresses

The MAF/MAP smoke-limiter switch is variously reported in literature at
addresses including `0x51C30`, `0x71C30`, `0x61126`, `0x61170`,
`0x71AF0`/`0x71AF1` — they are file-version-specific. Locate the cell
on the binary at hand and confirm the canonical 0/257 (`0x101`)
semantics before flashing.

## 2. GT1544S compressor map

The 2150 / 2050 mbar caps in [`envelope.rs`] are derived by analogy with
the GT1544V family and forum consensus on the GT1544S journal-bearing
variant. If a real compressor map for the AMF-spec GT1544S becomes
available, the caps should be re-derived from PR vs corrected mass-flow.

## 3. Nm-per-mg-IQ calibration

`NM_PER_MG_IQ = 4.4` is calibration-tuned to the stock 195 Nm @ 44.5 mg
data point (44.5 × 4.4 ≈ 195). Some literature suggests 5.4 Nm/mg for
larger PDs; **do NOT port that figure to AMF** — keep 4.4.

## 4. Idle-stability threshold

The R21 Warn threshold of σ > 25 over 30 s is generous. A stricter
Info-level σ > 15 is included in the rule pack. If the operator's
testbed has known-good injectors, tightening Warn to σ > 15 may be
appropriate (edit `IDLE_INSTABILITY_THRESHOLD_RPM_STD` in
[`platform/amf_edc15p/egr.rs`]).

## 5. Stock SOI / duration map references

The default SOI delta and duration-axis extension are extrapolated from
PD150 / ARL references in the public DAMOS sources. The screening
duration model `iq × 0.55 × sqrt(rpm/3000)` over-estimates compared to
the ARL data and is a heuristic only — replace with a measured
map-derived model if a DAMOS extract becomes available.

## 6. LUK SMF clutch rating

The 240 Nm cap is engineering judgement, not a manufacturer spec. LUK
does not publish a torque rating for this OE clutch on this platform.
Forum consensus for the 1.4 / 1.9 PD75 family LUK SMF is "holds factory
+ ~25 % stage-1, slips around 230–250 Nm sustained."

## 7. DTC group split (Group A vs Group B)

If a future AMF variant or an added EGR-position-sensor retrofit ever
appears, the defensive Group B suppression list (P0404/P0405/P0406) will
need re-evaluation. The default suppression list covers both groups; on
a genuinely retrofitted AMF the Group B codes might fire legitimately.

## 8. EDC15P+ second EGR map (`arwMEAB1KL`)

Confirmed in the public DAMOS map-pack as a paired map but its actual
semantic on AMF (vs PD-130/150) is not 100 % settled. The default-delta
pair zeroes both banks defensively; if a future analysis shows bank B is
unused on AMF this is harmless, and if bank B *is* the active map on a
mis-identified ECU file the parity prevents the delete failing.

## 9. Driver_Wish parallel banks (R22 / Driver_Wish_low_pedal)

EDC15P+ binaries carry **5** parallel `mrwFVH_KF` (Driver_Wish) maps.
Per the public DAMOS `cowFUN_DSV` codeblock-detail symbol, banks 1 and
4 are the automatic-codeblock variants and banks 2, 3, 5 are the manual
variants. On the manual Fabia 6Y2 only banks 2/3/5 are read at runtime,
but standard EDC15P+ tuning practice is to mirror all 5 banks
identically to remain coding-state-invariant. The R22 algorithm
operates on logged behaviour, not on the binary, so the bank count
does not block the rule. The `Driver_Wish_low_pedal` recommendation
must be applied to ALL 5 parallel banks consistently — the symbolic
action description says so, and the WinOLS operator is expected to use
parallel-map mode.

## 10. EDC15P+ fan-stage map naming on AMF (Fan_thresholds / Fan_run_on)

Public DAMOS coverage for EDC15P+ is patchy on the 1.4 PD75 platform —
the same patchiness applies to fan-stage threshold and run-on duration
maps. The tool treats `Fan_thresholds` and `Fan_run_on` as
**symbolic-only** until a known-good DAMOS or A2L is on disk. The
default-deltas describe the four target thresholds (clamped through
`clamp_fan_on_c`) and the additive run-on (clamped through
`clamp_fan_run_on_s`); they do not produce a literal cell list.

## 11. Stock T_coolant fan thresholds for AMF / EDC15P+

Public references describe the *physical* two-stage VAG thermoswitch
(1H0959481B 95-on/84-off + 102-on/91-off; 1J0959481A same; some 6Y2/9N
variants 1H0959481C 90-on/79-off; 701959481 87-on/76-off + 93-on/82-off).
On an ECU-driven scheme the *logical* threshold lives in firmware. The
default constants assume the stock logical threshold is in the
**95–100 °C** stage-1, **~102 °C** stage-2 window. If the AMF firmware
logs disagree (e.g. fan-on observed at 105 °C on a cooled-engine pull-to-
idle), update the `Fan_thresholds` row in code; do NOT widen
`CAPS.fan_on_max_c`.

## 12. Loggable `fan_stage` channel on AMF

Confirmed loggable on some EDC16+ platforms via measuring blocks
130/131 (engine outlet temp, radiator outlet temp, thermostat duty
cycle). On EDC15P+ / AMF the channel is firmware-dependent and treated
as **optional / advisory** — R23 does not require it. If a future
firmware exposes it, the rule's evidence dictionary can be extended to
include observed fan-stage transitions.

## 13. Low-pedal stock IQ values on PD75 specifically

Public PD150 dumps show Driver_Wish maxing out at ~50 mg/stroke at
100 % pedal in mid-rpm, with low-pedal cells in the single digits to
low teens. PD75 (AMF) is reported as a 13×16 layout with 5 parallel
banks (per `cowFUN_DSV`); no canonical numeric reference is embedded
here. The R22 algorithm computes slopes from the **logged** behaviour,
not from a stock-value assumption, so this uncertainty does not block
the rule.
