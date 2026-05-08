# Platform: AMF / EDC15P+ — long form

## Vehicle & engine

| Parameter | Value | Note |
|---|---|---|
| Chassis | Škoda Fabia Mk1 (6Y2) | A04 / PQ24 platform |
| Kerb weight | ~1120 kg | Light — torque budget is small |
| Engine code | AMF | 04/2003 – 10/2005 in Fabia |
| Displacement | 1422 cc (R3, 79.5 × 95.5 mm) | Three cylinders |
| Compression | 19.5:1 | Sets ignition-delay window |
| Stock power | 55 kW / 75 PS @ 4000 rpm | |
| Stock torque | 195 Nm @ 2200 rpm | |
| Injection | Pumpe-Düse, cam-driven, ~2050 bar peak | NOT common-rail |
| Turbo | Garrett GT1544S, journal-bearing wastegated, **fixed geometry** | OEM 045 145 701 J. NOT VNT. |
| Intercooler | Air-air, small | Heat-soak limited |
| MAF | Bosch HFM5 | Reports mg/stroke to ECU |
| MAP | Combined T-MAP after intercooler, ~2.5 bar abs sensor | Saturates ~2495 mbar |
| Clutch / flywheel | LUK SMF clutch | Engineering-judgement ceiling 240 Nm |
| Manifold | Cast-iron exhaust manifold | Cracking risk if cycled hard |
| ECU | Bosch EDC15P+ | VAG 045 906 019 BM, Bosch HW 0281 011 412, SW 1039S02900 1166 0178 |
| EGR | Vacuum-actuated, N18 solenoid, spring-return-to-closed | No position sensor on AMF |

## Stock fuelling, boost & torque references

| Channel | Stock value | Source / note |
|---|---|---|
| Peak IQ at WOT (~1750–2000 rpm) | ~44.5 mg/stroke | Generates the 195 Nm peak |
| Peak IQ at high RPM (~4000 rpm) | ~37 mg/stroke | Drops to make peak hp |
| Stock boost target ramp | 1100 mbar @ 1300 rpm → ~1900–2000 @ 2000–3500 → tapering to ~1700 by 4500 (absolute) | Group-011 logs; GT1544S shallower taper above 3500 rpm |
| Stock SVBL (boost limiter) | ~2200–2350 mbar absolute | Hard cut-off; ECU goes limp above |
| Stock SOI @ 4000 rpm, 37 mg | ~20–22° BTDC | EDC15P+ SOI maps |
| Stock atm-pressure correction (group 010) | ~990–1010 mbar at sea level | Used for altitude IQ derate |

## Sane Stage 1 target envelope

A "sane" Stage 1 on AMF is **not** the 105–125 hp number quoted by aggressive
shops. Those tunes either smoke, kill the Garrett GT1544S by overspeed, or
chew the LUK SMF. The conservative target this tool aims at:

| Metric | Stock | Target |
|---|---|---|
| Peak power | 55 kW / 75 hp | ~70 kW / 95 hp |
| Peak torque | 195 Nm | ~240 Nm (clutch ceiling) |
| Peak IQ | 44.5 mg | up to 54 mg/stroke (PD75 nozzle headroom) |
| Peak boost (absolute) | ~2000 mbar | ~2100–2150 mbar (Δ +100–200) |
| Min lambda at WOT | ~1.30 | ≥ 1.05 (smoke-tolerant controlled-environment floor) |
| Pre-turbo EGT ceiling | < 750 °C | < 800 °C sustained |
| SOI advance @ 4000 rpm | ~21° | up to 26° BTDC at IQ ≥ 30 mg |

## Known weak points (drive every rule's rationale)

1. **LUK SMF clutch** — designed around 195 Nm. Above ~240 Nm at the
   flywheel it slips, then judders, then dies. Single dominant longevity
   constraint.
2. **Garrett GT1544S compressor** — small wastegated turbo. Sustained
   operation above PR ~2.15 (2150 mbar absolute at sea level) puts the
   shaft past the right edge of the compressor map → rising outlet
   temperature, shaft overspeed, bearing failure within months.
3. **PD75 injectors** — at ~54 mg/stroke they approach the duration
   headroom of the stock cam lobe; EOI starts pushing past 10° ATDC and
   EGT climbs sharply.
4. **Cast-iron manifold** — durable, but cracks at the runner-collector
   weld under repeated 850 °C+ thermal cycles.
5. **Stock pistons** — aluminium, no oil-jets on AMF. SOI advance > 26°
   BTDC at high IQ migrates peak cylinder pressure ahead of TDC and
   stresses the unjacketed pistons.
6. **MAF mg/stroke ceiling** — the EDC15P+ map quantisation tops out
   around 1000 mg/stroke. The Bosch HFM5 itself does not saturate at
   AMF airflows; this is a map-side limit, not a sensor limit.
7. **MAP sensor** — Bosch saturates at ~2495 mbar; never request above
   ~2400 mbar or you lose closed-loop boost control entirely.

## EDC15P+ map registry (canonical names)

The recommendation engine emits deltas tied to these names so a tuner can
paste them into WinOLS / EDC15P Suite by hand. The tool does not parse
the `.bin`.

| Map | German alias | Axes | Cell unit | Sane Stage 1 Δ |
|---|---|---|---|---|
| `LDRXN` | Ladedruck-Sollwert | RPM × IQ | mbar abs | +100..+200 in 2000–3500 rpm; taper to stock by 4000 |
| `LDOLLR` | LDR-Sollwertbegrenzung | RPM × atm-pressure | mbar | Cap at 2150 mbar at sea level |
| `SVBL` | Ladedruck-Begrenzung absolut | scalar | mbar | Leave stock |
| `Driver_Wish` | Fahrerwunsch | Pedal % × RPM | mg/str | Set to 50 mg @ pedal 100 % × rpm 1800–3500 |
| `Driver_Wish_low_pedal` | Fahrerwunsch (Pedal 1..25 %) | Pedal % × RPM | mg/str | Flatten dIQ/dpedal ≤ 0.40 across the 5..25 % band; preserve idle creep |
| `Smoke_IQ_by_MAF` | Begrenzungsmenge (MAF) | MAF × RPM | mg/str | Enforce λ ≥ 1.05 |
| `Smoke_IQ_by_MAP` | Begrenzungsmenge (MAP) | Boost × RPM | mg/str | Same λ discipline |
| `Torque_Limiter` | Drehmomentbegrenzer | RPM × atm-pressure | Nm | Cap at 240 Nm |
| `MLHFM` | Luftmassenmesser-Kennlinie | sensor V → kg/h | kg/h | Leave stock unless MAF replaced |
| `SOI` | Spritzbeginn (10 maps by coolant) | RPM × IQ | ° BTDC | +1.5° at 4000 rpm column; never > 26° at IQ ≥ 30 mg |
| `SOI_warm_cruise` | Spritzbeginn warm (Cruise-Band) | RPM × IQ | ° BTDC | −1.0° in 1500–2500 × 5–15 mg if cruise NVH is objectionable |
| `Duration` | Einspritzdauer (6 maps by SOI band) | RPM × IQ | ° crank | Extend axis 50 → 54 mg |
| `Pilot` | Voreinspritzmenge / -zeit | RPM × IQ | mg/° | Leave stock |
| `N75_duty` | Ladedruckregler-Tastverhältnis | RPM × diff or IQ | % DC | Leave stock unless R01 fires repeatedly |
| `Lambda_limiter` | Lambdawunsch | MAF × RPM | λ | Floor cells at 1.05 |
| `Atmospheric_correction` | Höhenkorrektur LDR | atm scalar | mbar Δ | Leave stock |
| `EGT_model` | Abgastemperatur-Modell | varies | °C | Do not raise |
| `AGR_arwMEAB0KL` | EGR-duty bank A | RPM × IQ | % duty | Zero all cells |
| `AGR_arwMEAB1KL` | EGR-duty bank B | RPM × IQ | % duty | Zero all cells |
| `arwMLGRDKF` | Sollluftmasse / EGR target air mass | RPM × IQ | mg/str | Fill ≥ 850 mg/str (Strategy B) |
| `DTC_thresholds` | DTC-Grenzwerte | per code | mg/s · ms | Widen P0401–P0406 |
| `MAF_MAP_smoke_switch` | Smoke-limiter source switch | scalar | byte | Leave stock at 0x00 (MAF-based) |
| `Idle_fuel` | Leerlauf-Mengenkennfeld | RPM × IQ | mg/str | −1.5 mg/str only if R21 idle stability fires |
| `Fan_thresholds` | Lüfter Schwellenwerte | stage | °C | Stage-1 on/off ≈ 93/88; stage-2 on/off ≈ 98/95; clamped to 88..98 °C |
| `Fan_run_on` | Lüfter-Nachlauf | scalar | s | +60 s post-key-off, capped at 240 s total |

## VCDS groups to log

| Group | Fields | Tuning relevance |
|---|---|---|
| 001 | RPM · IQ · piston V · coolant | Always log. Idle health + IQ adaptation baseline. |
| 003 | RPM · MAF spec · MAF actual · EGR duty | **Critical** — fueling math depends on this. |
| 004 | RPM · battery · coolant · TDC | Sanity. |
| 005 | RPM · load · vehicle speed | Vehicle speed for cruise / idle distinction. |
| 008 | RPM · IQ req · IQ limit RPM · IQ limit MAF | **Critical** — shows which limiter is active. |
| 010 | MAF · barometric pressure · TPS | Log key-on/engine-off for ambient pressure. |
| 011 | RPM · boost spec · boost actual · N75 DC | **The boost group**. Most tuning decisions hinge on it. |
| 013 | Smooth-running cyl 1/2/3 (or fuel temp on some firmwares) | Cam/injector health. |
| 015 | RPM · torque request · torque actual | Torque-limiter visibility. |
| 020 | RPM · timing actual · MAP abs · load | **The timing group**. SOI logging. |
| 031 | (variant of MAF) | Fallback if 003 unavailable. |

EDC15P+ talks **KW1281**, not KWP-2000. VCDS achieves ~3.5–4.5 samples/sec
on a single group, ~2/sec on two groups, ~1/sec on three groups. The parser
warns if the median sample interval exceeds 350 ms (`LOW_RATE` flag); R09
downgrades from `critical` to `warn` on `LOW_RATE` pulls because SOI
transients can be missed at the slow rate. R10 keeps its Warn baseline
regardless of sample rate (already the lowest non-info severity).
