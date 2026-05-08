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
| Injection | Pumpe Düse, cam-driven, ~2050 bar peak | NOT common-rail |
| Turbo | KKK KP35, wastegated, **fixed geometry** | NOT VNT |
| Intercooler | Air-air, small | Heat-soak limited |
| MAF | Bosch HFM5, datasheet ceiling ~640 kg/h | mg/stroke to ECU |
| MAP | Combined T-MAP after intercooler, ~2.5 bar abs sensor | Saturates ~2495 mbar |
| Clutch / flywheel | LUK SMF clutch | Hard ceiling ≈ 240 Nm |
| Manifold | Cast-iron exhaust manifold | Cracking risk if cycled hard |

## Stock fuelling, boost & torque references

| Channel | Stock value | Source / note |
|---|---|---|
| Peak IQ at WOT (~1750–2000 rpm) | ~44.5 mg/stroke | Generates the 195 Nm peak |
| Peak IQ at high RPM (~4000 rpm) | ~37 mg/stroke | Drops to make peak hp |
| Stock boost target ramp | 1100 mbar @ 1300 rpm → ~1900–2000 @ 2000–3500 → tapering to ~1750 by 4500 (absolute) | Group-011 logs |
| Stock SVBL (boost limiter) | ~2200–2350 mbar absolute | Hard cut-off; ECU goes limp above |
| Stock SOI @ 4000 rpm, 37 mg | ~20–22° BTDC | EDC15P+ SOI maps |
| Stock atm-pressure correction (group 010) | ~990–1010 mbar at sea level | Used for altitude IQ derate |

## Sane Stage 1 target envelope

A "sane" Stage 1 on AMF is **not** the 105–125 hp number quoted by aggressive
shops. Those tunes either smoke, kill the KP35 by overspeed, or chew the LUK
SMF. The conservative target this tool aims at:

| Metric | Stock | Sane Stage 1 target |
|---|---|---|
| Peak power | 55 kW / 75 hp | ~70 kW / 95 hp |
| Peak torque | 195 Nm | ~240 Nm (clutch ceiling) |
| Peak IQ | 44.5 mg | 50–52 mg (Δ +6–8 mg) |
| Peak boost (absolute) | ~2000 mbar | ~2100–2150 mbar (Δ +100–200) |
| Min lambda at WOT | ~1.30 | ≥1.20 |
| Pre-turbo EGT ceiling | <750 °C | <800 °C sustained |
| SOI advance @ 4000 rpm | ~21° | +1.5 to +2.5° |

## Known weak points (drive every rule's rationale)

1. **LUK SMF clutch** — designed around 195 Nm. Above ~240 Nm at the
   flywheel it slips, then judders, then dies. Single dominant longevity
   constraint.
2. **KP35 compressor** — small wastegated turbo. Sustained operation above
   PR ~2.15 (2150 mbar absolute at sea level) puts you off the right edge of
   the compressor map → rapidly rising outlet temp, shaft over-speed
   (rated ~206 000 rpm), bearing failure within months.
3. **PD injectors** — at ~50 mg/stroke they approach the duration headroom
   of the stock cam lobe; EOI starts pushing past 6° ATDC and EGT climbs
   sharply.
4. **Cast-iron manifold** — durable, but cracks at the runner-collector weld
   under repeated 850 °C+ thermal cycles.
5. **Stock pistons** — aluminium, no oil-jets on AMF. SOI advance > 27° BTDC
   at high IQ punches a hole in piston #1 (closest to belt end, hottest).
6. **MAF saturation** — Bosch HFM5 reports up to ~1100 mg/stroke before it
   pegs. Stock peak ~600; Stage 1 will see ~700–750.
7. **MAP sensor** — Bosch saturates at ~2495 mbar; never request above
   ~2400 mbar or you lose closed-loop boost control entirely.

## EDC15P+ map registry (canonical names)

The recommendation engine emits deltas tied to these names so a tuner can
paste them into WinOLS / EDCSuite by hand. The tool does not parse the
`.bin`.

| Map | German alias | Axes | Cell unit | Stage 1 sane Δ |
|---|---|---|---|---|
| `LDRXN` | Ladedruck-Sollwert | RPM × IQ | mbar abs | +100..+200 in 2000–3500 rpm; taper to stock by 4000 |
| `LDOLLR` | LDR-Sollwertbegrenzung | RPM × atm-pressure | mbar | Cap at 2150 mbar at sea level |
| `SVBL` | Ladedruck-Begrenzung absolut | scalar | mbar | Leave stock |
| `Driver_Wish` | Fahrerwunsch | Pedal % × RPM | mg/str | +6..+8 mg in 1800–3500 |
| `Smoke_IQ_by_MAF` | Begrenzungsmenge (MAF) | MAF × RPM | mg/str | Enforce λ ≥ 1.20 |
| `Smoke_IQ_by_MAP` | Begrenzungsmenge (MAP) | Boost × RPM | mg/str | Same λ discipline |
| `Torque_Limiter` | Drehmomentbegrenzer | RPM × atm-pressure | Nm | Cap at 240 Nm |
| `MLHFM` | Luftmassenmesser-Kennlinie | sensor V → kg/h | kg/h | Leave stock unless MAF replaced |
| `SOI` (10 maps by coolant) | Spritzbeginn | RPM × IQ | ° BTDC | +1.5..+2.5° at 4000 rpm column only; never > 26 |
| `Duration` (6 maps by SOI band) | Einspritzdauer | RPM × IQ | ° crank | Extend axis 50 → 52 mg if extending IQ |
| `Pilot` | Voreinspritzmenge / -zeit | RPM × IQ | mg/° | Leave stock |
| `N75_duty` | Ladedruckregler-Tastverhältnis | RPM × diff or IQ | % DC | Leave stock unless steady-state error > 150 mbar |
| `Lambda_limiter` | Lambdawunsch | MAF × RPM | λ | Floor cells at 1.20 |
| `Atmospheric_correction` | Höhenkorrektur LDR | atm scalar | mbar Δ | Leave stock |
| `EGT_model` | Abgastemperatur-Modell | varies | °C | Do not raise |

## VCDS groups to log

| Group | Fields | Tuning relevance |
|---|---|---|
| 001 | RPM · IQ · piston V · coolant | Always log. Idle health + IQ adaptation baseline. |
| 003 | RPM · MAF spec · MAF actual · EGR duty | **Critical** — fueling math depends on this. |
| 004 | RPM · battery · coolant · TDC | Sanity. |
| 008 | RPM · IQ req · IQ limit RPM · IQ limit MAF | **Critical** — shows which limiter is active. |
| 010 | MAF · barometric pressure · TPS | Log key-on/engine-off for ambient pressure. |
| 011 | RPM · boost spec · boost actual · N75 DC | **The boost group**. Most tuning decisions hinge on it. |
| 013 | Smooth-running cyl 1/2/3 (or fuel temp on some firmwares) | Cam/injector health. |
| 015 | RPM · torque request · torque actual | Torque-limiter visibility. |
| 020 | RPM · timing actual · MAP abs · load | **The timing group**. SOI logging. |
| 031 | (variant of MAF) | Fallback if 003 unavailable. |

EDC15P+ talks **KW1281**, not KWP-2000. VCDS achieves ~3.5–4.5 samples/sec
on a single group, ~2/sec on two groups, ~1/sec on three groups. The parser
warns if the median sample interval exceeds 350 ms (`LOW_RATE` flag), and
R09 + R10 are downgraded from `critical` to `warn` on `LOW_RATE` pulls
because SOI transients can be missed at the slow rate.
