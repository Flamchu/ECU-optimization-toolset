# ecu-shenanigans

Single-platform ECU datalog analyzer for **Skoda Fabia Mk1 (6Y2) · 1.4 TDI PD ·
engine code AMF · Bosch EDC15P+**. Ingests **VCDS** `.csv`
exports, runs a fixed AMF-specific rule pack, and emits sane Stage 1 tuning
recommendations clamped to a hard longevity envelope.

The tool is **read-only against the ECU**. It never writes the `.bin`, never
talks to the OBD port, never flashes anything. Suggestions are emitted in the
symbolic EDC15P+ damos vocabulary (`LDRXN`, `Smoke_IQ_by_MAP`, `SOI`, …) so
you can paste them into WinOLS / VAGEDCSuite by hand.

> **`ecu-shenanigans` is an analysis and educational tool. It does NOT modify
> your ECU. Any tuning changes are performed at the user's sole risk, on
> private property only, on a vehicle the user owns. Modifying engine
> calibration may void your warranty, render the vehicle non-roadworthy,
> contravene type-approval / emissions regulations in your jurisdiction
> (e.g. EU Regulation 2018/858, UK MOT diesel smoke limits, US CAA §203),
> and may damage the engine, turbocharger, clutch, or particulate
> after-treatment. The "sane Stage 1" envelope encoded in this tool is a
> conservative community heuristic, not a manufacturer specification. The
> authors accept no liability. If the tool says BLOCKED — envelope cap, do
> not work around it.**

## Sane Stage 1 envelope (hard caps)

| Quantity | Cap | Reason |
|---|---|---|
| Peak boost (abs) | 2150 mbar | Right edge of KP35 efficient compressor map |
| Peak boost above 4000 rpm | 2050 mbar | KP35 chokes; sustained PR > 2.0 over-speeds the shaft |
| Peak IQ | 52 mg/stroke | Stock injector duration headroom + LUK clutch ceiling |
| λ floor | 1.20 | PD smokes below this; physics floor is 1.05 |
| EGT (sustained) | 800 °C | Cast-iron manifold creep + AMF has no oil-jet pistons |
| SOI advance | 26° BTDC | Beyond this, peak cylinder pressure migrates ahead of TDC |
| Modelled flywheel torque | 240 Nm | LUK SMF clutch ceiling (195 Nm × 1.23) |

See [`docs/platform_amf.md`](docs/platform_amf.md) for the full platform
deep-dive and [`docs/rules.md`](docs/rules.md) for every rule's rationale.
The original tool specification is preserved in
[`docs/specification.md`](docs/specification.md).

## Build

Requires Rust 1.75 (2021 edition).

```sh
cargo build --release
```

The binary lands at `target/release/ecu-shenanigans`.

## Usage

```sh
ecu-shenanigans analyse path/to/vcds_log.csv --out ./out
```

Produces `out/report_<utc_timestamp>.md` containing:

- the verbatim disclaimer,
- log metadata (groups present, median sample interval, pull count),
- a findings table sorted by severity,
- a per-pull breakdown,
- the full recommendation table (APPLY / SKIP / BLOCKED),
- the list of rules SKIPPED because a required VCDS group is missing.

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
cargo test            # 83 tests across 8 binaries
cargo clippy --all-targets -- -D warnings
```

Unit tests cover every clamp function, every rule, the canonicalizer, the
resampler, the pull detector, and the recommendation engine. Integration
tests parse all three on-disk VCDS fixtures end-to-end and verify the
generated report. Property tests exercise every envelope clamp with 1024
random inputs each — none ever escape the envelope.

## Project structure

```
src/
├── main.rs                       # CLI binary
├── lib.rs                        # crate root
├── disclaimer.rs                 # verbatim §11 text
├── error.rs                      # crate error type
├── ingest/                       # VCDS CSV parser + canonicalizer
│   ├── canonicalize.rs
│   └── vcds.rs
├── platform/amf_edc15p/          # the only supported platform
│   ├── channels.rs               # canonical channel registry
│   ├── stock_refs.rs             # stock IQ / boost / SOI baselines
│   ├── envelope.rs               # hard caps + clamp_to_envelope
│   ├── maps.rs                   # EDC15P+ map registry (symbolic only)
│   └── default_deltas.rs         # sane Stage 1 default deltas
├── rules/                        # base types + R01..R15 + runner
│   ├── base.rs
│   ├── pack.rs
│   └── runner.rs
├── recommend/                    # engine + Markdown report writer
│   ├── engine.rs
│   └── report.rs
└── util/                         # 5 Hz resample + WOT-pull detection
    ├── pulls.rs
    └── timebase.rs

tests/
├── fixtures/                     # three VCDS CSV fixtures
├── integration_engine.rs         # ingest → analyse → report pipeline
├── integration_envelope.rs       # property tests for clamp_*
├── integration_pulls.rs          # pull-detection invariants
├── integration_rules.rs          # one test per R01..R15
└── integration_vcds.rs           # parser end-to-end
```

## Out of scope

- Reading or writing the ECU `.bin` (no WinOLS / EDCSuite / KESS / KTAG
  integration).
- Live OBD / KKL communication.
- Any platform other than AMF / EDC15P+.
- DPF / EGR delete recommendations.

## Licence

MIT — see [LICENSE](LICENSE).
