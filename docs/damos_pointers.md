# DAMOS pointers — open questions for the operator

The v4 build emits **symbolic recommendations only**. Map names follow the
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
need re-evaluation. The default v4 list suppresses both groups; on a
genuinely retrofitted AMF the Group B codes might fire legitimately.

## 8. EDC15P+ second EGR map (`arwMEAB1KL`)

Confirmed in the public DAMOS map-pack as a paired map but its actual
semantic on AMF (vs PD-130/150) is not 100 % settled. The default-delta
pair zeroes both banks defensively; if a future analysis shows bank B is
unused on AMF this is harmless, and if bank B *is* the active map on a
mis-identified ECU file the parity prevents the delete failing.
