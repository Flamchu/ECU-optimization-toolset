# ecu-shenanigans (Rust, v3 EGR-delete edition)

Single-platform ECU datalog analyzer for **Skoda Fabia Mk1 (6Y2) · 1.4 TDI PD ·
engine code AMF · Bosch EDC15P+**, rewritten in Rust. Ingests **VCDS** `.csv`
exports, runs a fixed AMF-specific 19-rule pack (R01–R15 + R16, R17, R17b,
R18, R19), and emits Stage 1 tuning recommendations clamped to a hard
longevity envelope.

**v3 mandates a software-only EGR delete and a smoke-tolerant lambda floor.**
The car is targeted as a controlled-environment / off-road / closed-circuit
testbed, not a road vehicle.

The tool is **read-only against the ECU**. It never writes the `.bin`, never
talks to the OBD port, never flashes anything. Recommendations are emitted in
the symbolic EDC15P+ damos vocabulary (`LDRXN`, `Smoke_IQ_by_MAP`, `SOI`,
`AGR_arwMEAB0KL`, `arwMLGRDKF`, …) so they can be pasted into WinOLS /
VAGEDCSuite by hand.

> **`ecu-shenanigans` is an analysis and educational tool. It does NOT modify
> your ECU.** This v3 build assumes a vehicle that is not registered for road
> use, not subject to periodic emissions testing, and used only in a
> controlled environment. Emissions DTC suppression and software EGR delete
> are acceptable in that context only. The user is solely responsible for
> ensuring the vehicle is not driven on public roads. Mechanical safety caps
> (240 Nm clutch, 2150 mbar boost, 800 °C EGT, 1.05 λ floor, 26° BTDC SOI)
> are *physical* limits, not regulatory ones.

## Sane Stage 1 + EGR-delete envelope (hard caps, v3)

| Quantity | v2 | **v3** | Reason |
|---|---|---|---|
| Peak boost (abs) | 2150 mbar | **2150 mbar** | Right edge of KP35 efficient compressor map |
| Peak boost above 4000 rpm | 2050 mbar | **2050 mbar** | KP35 chokes; sustained PR > 2.0 over-speeds the shaft |
| Peak IQ | 52 mg/stroke | **54 mg/stroke** | Stock injector duration + LUK clutch ceiling. v3 raised because smoke is removed |
| λ floor | 1.20 | **1.05** | v3 smoke-tolerant; never below stoich (incomplete combustion) |
| EGT (sustained) | 800 °C | **800 °C** | Cast-iron manifold creep + AMF has no oil-jet pistons |
| SOI advance | 26° BTDC | **26° BTDC** | Beyond this, peak cylinder pressure migrates ahead of TDC |
| Modelled flywheel torque | 240 Nm | **240 Nm** | LUK SMF clutch ceiling (195 Nm × 1.23) |
| **EGR duty** (NEW) | n/a | **0 %** | Mandatory software EGR delete |
| **Spec-MAF fill** (NEW) | n/a | **≥ 850 mg/stroke** | Strategy-B saturation; PID never demands EGR |

See [`docs/platform_amf.md`](docs/platform_amf.md) for the platform deep-dive
and [`docs/rules.md`](docs/rules.md) for v2 rule rationales (R01–R15 carry
over with R06 and R07 retuned in v3). The full v3 EGR-delete specification
is kept locally under `docs/dev/` (gitignored).

## Build

Requires Rust 1.75 (2021 edition).

```sh
cargo build --release
```

The binary lands at `target/release/ecu-shenanigans`.

## Usage

### `analyse` — produce a Markdown report

```sh
ecu-shenanigans analyse path/to/vcds_log.csv --out ./out
```

Produces `out/report_<utc_timestamp>.md` containing:

- the verbatim disclaimer,
- log metadata (groups present, median sample interval, pull count),
- **EGR Delete Strategy (v3)** — the six recommended map deltas with
  rationale and v3 envelope summary,
- a findings table sorted by severity (R01–R19 + R17b),
- a per-pull breakdown,
- the full recommendation table (APPLY / SKIP / BLOCKED),
- the list of rules SKIPPED because a required VCDS group is missing.

### `validate-egr-delete` — run the §7 post-flash checklist

```sh
ecu-shenanigans validate-egr-delete path/to/post_flash_log.csv
```

Runs the 15-item checklist from spec §7 and prints a Markdown report. Exits
**0 on PASS**, **2 on any FAIL**. Use it to verify a freshly flashed
EGR-delete bin: idle / cruise / WOT EGR duty zero, spec-MAF saturated, MAF
actual in HFM5 range, no P0401–P0406 DTCs, idle stable, modelled λ / EGT /
boost / torque inside the v3 envelope.

## VCDS log requirements

The parser expects the standard VCDS group banner. Required minimum groups
for any pull-analysis to run: **003 (MAF + EGR), 008 (IQ + limiters),
011 (boost spec/actual + N75)**. Strongly recommended additions: **001**
(idle baseline + coolant), **010** (ambient + TPS — log key-on engine-off),
**013** (smooth-running, injector health), **020** (timing).

If a required group is missing, the affected rules emit
`SKIPPED — required VCDS group(s) [...] not present` rather than producing
a misleading partial result.

## Tests

```sh
cargo test            # 129 tests across 9 binaries
cargo clippy --all-targets -- -D warnings
```

Unit tests cover every clamp function (incl. v3 EGR-duty / spec-MAF clamps),
every rule (R01–R15 + R16/R17/R17b/R18/R19), the canonicalizer, the
resampler, the pull detector, the recommendation engine, and the 15-item
EGR-delete validation checklist. Integration tests parse three v2 fixtures
plus the v3 `vcds_amf_pre_delete.csv` and `vcds_amf_post_delete.csv`
end-to-end: pre-delete log fails validation (exit 2, R16 critical),
post-delete log passes (exit 0). Property tests exercise every envelope
clamp with 1024 random inputs each — none ever escape the envelope.

## Project structure

```
src/
├── main.rs                          # CLI: analyse + validate-egr-delete
├── lib.rs                           # crate root
├── disclaimer.rs                    # verbatim §11 text
├── error.rs                         # crate error type
├── ingest/                          # VCDS CSV parser + canonicalizer
├── platform/amf_edc15p/             # the only supported platform
│   ├── channels.rs                  # canonical channel registry
│   ├── stock_refs.rs                # stock IQ / boost / SOI baselines
│   ├── envelope.rs                  # hard caps + clamp_to_envelope (v3)
│   ├── maps.rs                      # EDC15P+ map registry (21 entries)
│   ├── default_deltas.rs            # sane Stage 1 default deltas (v3)
│   └── egr.rs                       # NEW v3: EGR-delete strategy module
├── rules/                           # base types + R01..R15 + R16..R19
├── recommend/                       # engine + Markdown report
├── util/                            # 5 Hz resample + WOT-pull detection
└── validate/                        # NEW v3: §7 EGR-delete checklist

tests/
├── fixtures/
│   ├── vcds_amf_001_003_011.csv     # v2 healthy
│   ├── vcds_amf_008_011.csv         # v2 overboost
│   ├── vcds_amf_020_021.csv         # v2 lambda + SOI
│   ├── vcds_amf_pre_delete.csv      # NEW v3: EGR active
│   └── vcds_amf_post_delete.csv     # NEW v3: EGR delete applied
├── integration_egr.rs               # NEW v3: spec §13 acceptance criteria
├── integration_engine.rs            # ingest → analyse → report
├── integration_envelope.rs          # property tests for clamp_*
├── integration_pulls.rs             # pull-detection invariants
├── integration_rules.rs             # one test per R01..R19
└── integration_vcds.rs              # parser end-to-end
```

## Out of scope

- Reading or writing the ECU `.bin` (no WinOLS / EDCSuite / KESS / KTAG
  integration). The tool emits symbolic deltas only.
- Live OBD / KKL communication.
- Any platform other than AMF / EDC15P+.
- DPF delete recommendations (AMF has no DPF anyway).
- Any claim of road legality. v3 explicitly assumes controlled-environment
  use only.

## Repository

`https://github.com/Flamchu/ECU-optimization-toolset` — v3 supersedes the
old `ecu-shenanigans` repo.

## Licence

MIT — see [LICENSE](LICENSE).
