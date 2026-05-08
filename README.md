# ecu-shenanigans (Rust, v4 audit-reconciled edition)

Single-platform ECU datalog analyzer for **Skoda Fabia Mk1 (6Y2) · 1.4 TDI PD ·
engine code AMF · Bosch EDC15P+ · Garrett GT1544S journal-bearing wastegated
turbo (OEM 045 145 701 J — commonly misidentified in older tunes; v4 corrects
this throughout). Ingests **VCDS** `.csv` exports plus an
optional `<base>.dtc.txt` DTC sidecar, runs the full v4 21-rule pack
(R01–R21), and emits Stage 1 tuning recommendations clamped to a hard
longevity envelope.

**v4 mandates a software-only EGR delete and a smoke-tolerant lambda floor.**
The car is targeted as a controlled-environment / off-road / closed-circuit
testbed, not a road vehicle.

The tool is **read-only against the ECU**. It never writes the `.bin`, never
talks to the OBD port, never flashes anything. Recommendations are emitted in
the symbolic EDC15P+ DAMOS vocabulary (`LDRXN`, `Smoke_IQ_by_MAP`, `SOI`,
`AGR_arwMEAB0KL`, `AGR_arwMEAB1KL`, `arwMLGRDKF`, …) so they can be pasted
into WinOLS / EDC15P Suite by hand.

> **`ecu-shenanigans` is an analysis and educational tool. It does NOT modify
> your ECU.** This v4 build assumes a vehicle that is not registered for road
> use, not subject to periodic emissions testing, and used only in a
> controlled environment. Emissions DTC suppression and software EGR delete
> are acceptable in that context only. The user is solely responsible for
> ensuring the vehicle is not driven on public roads. Mechanical safety caps
> (240 Nm clutch, 2150 mbar boost, 800 °C EGT, 1.05 λ floor, 26° BTDC SOI)
> are *physical* limits, not regulatory ones.

## Sane Stage 1 + EGR-delete envelope (hard caps, v4)

| Quantity | Value | Reason |
|---|---|---|
| Peak boost (abs) | **2150 mbar** | Right edge of Garrett GT1544S efficient compressor map |
| Peak boost above 4000 rpm | **2050 mbar** | GT1544S choke flow + shaft overspeed risk |
| Peak IQ | **54 mg/stroke** | PD75 nozzle duration headroom + LUK clutch ceiling |
| λ floor | **1.05** | Diesel combustion floor (incomplete combustion below this) |
| EGT (sustained) | **800 °C** | Cast-iron manifold creep + AMF has no piston-cooling oil jets |
| SOI advance | **26° BTDC at IQ ≥ 30 mg** | Beyond this, peak cylinder pressure migrates ahead of TDC |
| EOI | **≤ 10° ATDC** | Past this, heat dumps into the turbine |
| Modelled flywheel torque | **240 Nm** | LUK SMF clutch (engineering judgement, not LUK spec) |
| MAF mg/stroke ceiling | **1000** | ECU map quantisation (NOT a Bosch HFM5 sensor saturation) |
| Spec-MAF fill | **≥ 850 mg/stroke** | Strategy-B saturation; canonical Bosch HFM5 calibration target |
| EGR duty | **0 % in both banks** | Mandatory software EGR delete |
| Coolant pull-min (R11) | **80 °C** | Cold SOI map invalidates a pull |
| Warm cruise/idle min (R17/R18/R21) | **70 °C** | Engine off cold-start map |

See [`docs/platform_amf.md`](docs/platform_amf.md) for the platform deep-dive,
[`docs/rules.md`](docs/rules.md) for R01–R21 rationale, and
[`docs/damos_pointers.md`](docs/damos_pointers.md) for the open DAMOS
questions the operator is responsible for.

## Build

Requires Rust 1.75 (2021 edition).

```sh
cargo build --release
```

The binary lands at `target/release/ecu-shenanigans`.

## Usage

### `analyse` — produce a Markdown report

```sh
ecu-shenanigans analyse \
    --input path/to/vcds_log.csv \
    [--input path/to/second_bundle.csv ...] \
    [--dtc path/to/dtc_scan.txt] \
    [--validate] \
    [--out report.md] \
    --accept-disclaimer
```

`--accept-disclaimer` is **mandatory** — the tool exits 2 if it is missing
(forces every operator to re-read the §0 disclaimer at every invocation).

The report contains:

- the verbatim disclaimer,
- log metadata (groups present, median sample interval, pull count, DTCs
  if a sidecar was provided),
- **EGR Delete Strategy (v4)** — recommended map deltas (both EGR banks),
  full rationale, hard envelope summary,
- a findings table sorted by severity (R01–R21; global rules R16/R19/R21
  rendered with pull `G`),
- a per-pull breakdown,
- the full 22-row recommendation table (APPLY / SKIP / BLOCKED),
- (with `--validate`) the 15-item §10 post-EGR-delete validation checklist
  appended as a Markdown subsection,
- the list of rules SKIPPED because a required VCDS group is missing.

### `validate-egr-delete` — run the §10 post-flash checklist

```sh
ecu-shenanigans validate-egr-delete \
    [--pre path/to/pre_delete_log.csv] \
    --post path/to/post_delete_log.csv \
    [--dtc path/to/dtc_scan.txt] \
    [--out report.md] \
    --accept-disclaimer
```

When `--pre` is supplied, items 11 and 12 (pre/post idle MAF Δ ≥ 50 mg, pre
EGR duty > 5 %) are evaluated. Without `--pre` they are reported as
`SKIPPED`.

Exits **0 on PASS**, **2 on any FAIL**.

### DTC sidecar format

DTCs come from a separate VCDS DTC scan, exported as a plain text file:
one DTC per line. Lines beginning with `#` and blank lines are ignored;
descriptions after the code are tolerated.

```text
# DTC scan from VCDS
P0401   EGR insufficient flow
P0403   EGR solenoid circuit
```

The parser auto-loads the conventional `<base>.dtc.txt` file alongside
the CSV; pass `--dtc` to override the path.

## VCDS log requirements

The parser expects the standard VCDS group banner. Required minimum groups
for any per-pull rule to run: **003 (MAF + EGR), 008 (IQ + limiters),
011 (boost spec/actual + N75)**. Strongly recommended: **001** (idle
baseline + coolant), **004** (sanity), **005** (RPM + load + vehicle
speed), **010** (ambient + TPS — log key-on engine-off), **013**
(smooth-running, injector health), **020** (timing).

If a required group is missing, the affected rules emit
`SKIPPED — required VCDS group(s) [...] not present` rather than producing
a misleading partial result.

## Tests

```sh
cargo test                                    # full suite
cargo clippy --all-targets -- -D warnings     # zero warnings
```

Unit tests cover every clamp function (incl. v4 IQ-aware `clamp_soi`),
every rule (R01–R21), the canonicalizer, the resampler with LOCF for
`egr_duty`, the pull detector, the recommendation engine, the 15-item
EGR-delete validation checklist, and the DTC sidecar parser. Integration
tests parse the on-disk VCDS fixtures end-to-end. Property tests exercise
every envelope clamp with 1024 random inputs each — none ever escape the
envelope.

## Project structure

```
src/
├── main.rs                          # CLI: analyse + validate-egr-delete
├── lib.rs                           # crate root
├── disclaimer.rs                    # verbatim §0 text
├── error.rs                         # crate error type
├── ingest/                          # VCDS CSV parser + canonicalizer + DTC sidecar
│   ├── canonicalize.rs
│   ├── dtc.rs                       # NEW v4: <base>.dtc.txt parser
│   └── vcds.rs
├── platform/amf_edc15p/             # the only supported platform
│   ├── channels.rs                  # canonical channel registry
│   ├── stock_refs.rs                # stock IQ / boost / SOI baselines
│   ├── envelope.rs                  # hard caps + clamp_* (v4)
│   ├── maps.rs                      # EDC15P+ map registry (22 entries)
│   ├── default_deltas.rs            # sane Stage 1 default deltas (22 rows)
│   └── egr.rs                       # EGR-delete strategy module (both banks)
├── rules/
│   ├── base.rs                      # Rule, Finding, Severity, RuleScope
│   ├── pack.rs                      # R01..R21 + RuleId enum + dispatch
│   └── runner.rs                    # global vs per-pull dispatch
├── recommend/                       # engine + Markdown report
└── validate/                        # §10 EGR-delete checklist (15 items)

tests/
├── fixtures/                        # VCDS CSVs + .dtc.txt sidecars
├── integration_egr.rs               # spec §13 acceptance criteria
├── integration_engine.rs            # ingest → analyse → report
├── integration_envelope.rs          # property tests for clamp_*
├── integration_pulls.rs             # pull-detection invariants
├── integration_rules.rs             # one test per R01..R21
├── integration_vcds.rs              # parser end-to-end
├── integration_dispatch.rs          # NEW v4: every RuleId reachable
├── integration_dtc.rs               # NEW v4: sidecar ingest + R19 firing
└── integration_v4_invariants.rs     # NEW v4: cross-cutting acceptance
```

## Out of scope

- Reading or writing the ECU `.bin` (no WinOLS / EDC15P Suite / KESS /
  KTAG integration). The tool emits symbolic deltas only.
- Live OBD / KKL communication.
- Any platform other than AMF / EDC15P+ on Garrett GT1544S.
- DPF delete recommendations (AMF has no DPF anyway).
- Any claim of road legality. v4 explicitly assumes controlled-environment
  use only.

## Repository

`https://github.com/Flamchu/ECU-optimization-toolset`

## Licence

MIT — see [LICENSE](LICENSE).
