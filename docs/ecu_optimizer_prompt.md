# ECU Map Optimizer — Build Specification

## 1. Project Context

Build a desktop application that analyzes ECU datalogs from real-world driving or dyno pulls and produces concrete, cell-level map-change recommendations. The tool does **not** read or write ECU binaries — that's handled by external tools (TunerPro, WinOLS, KTuner, Galletto). This tool's job is the analysis and recommendation layer that sits between "I have a log" and "I know what to change."

The user is a hobbyist tuner working on two specific platforms initially. The architecture must be platform-pluggable so additional ECUs can be added without rewriting the core.

## 2. Target Platforms (MVP)

### Platform A — Skoda Fabia 6Y2 1.4 TDI PD
- ECU: Bosch EDC15P+ (engines AMF/BNM 75hp, BAY 80hp)
- Fuel system: Pumpe Düse (unit injector), no common rail
- Turbo: KP35 / K03 variant
- Log source: VCDS measuring blocks exported as CSV, or ECUx logs
- Tuning levers (recommendations target these maps):
  - IQ limiter (mg/stroke vs RPM)
  - Boost target (LDRXN)
  - Boost limiter (LDRPLMX)
  - Smoke limiter (IQ vs MAF)
  - Injection timing / Start of Injection
  - Torque limiters (driver wish, MAF-based, RPM-based)

### Platform B — Honda Civic 8G R18 i-VTEC
- ECU: Honda PGM-FI, KTuner-compatible
- Naturally aspirated, port injection, economy i-VTEC
- Log source: KTuner CSV exports
- Tuning levers:
  - Base ignition table (RPM x Load)
  - VE / fuel table
  - VTEC engagement points
  - Cam timing (VTC) targets
  - Throttle map / pedal-to-throttle

## 3. Core Concept and Workflow

```
[CSV log file] -> [Platform adapter] -> [Normalized session]
   -> [Analysis engine: per-rule checks] -> [Issue list]
   -> [Recommendation engine: cell-level deltas] -> [GUI display]
   -> [Export: recommendations.json + annotated map images]
```

The user loads a log, the tool tells them what's wrong and what to change, they apply those changes externally, log again, iterate.

## 4. Functional Requirements

### 4.1 Log Ingestion
- Accept CSV files via drag-and-drop or file picker.
- Auto-detect platform by column header signature; allow manual override.
- Normalize to internal canonical channel names (e.g., `rpm`, `map_actual`, `map_target`, `iq_actual`, `iq_requested`, `maf`, `iat`, `ect`, `egt`, `afr_actual`, `afr_target`, `knock_retard`, `ignition_advance`, `tps`, `vehicle_speed`, `time`).
- Handle units conversion (kPa <-> bar, °C <-> °F, mg/str, etc.).
- Validate: warn if critical channels are missing for the selected analyses.
- Support loading multiple logs as a session (concatenate or compare).

### 4.2 Platform Adapters
Each platform is a folder under `platforms/` with:
- `adapter.py` — column mapping, unit conversion, channel synthesis (e.g., compute `load` from MAF and RPM if not directly logged).
- `maps.json` — definition of each tunable map: name, X axis (channel + breakpoints), Y axis, units, safe ranges, description.
- `rules.py` — platform-specific analysis rules.

Adding a new platform must require zero changes to core code.

### 4.3 Analysis Engine
Runs a set of rules against the normalized session. Each rule returns zero or more `Issue` objects:

```python
@dataclass
class Issue:
    severity: Literal["info", "warn", "critical"]
    rule_id: str
    title: str          # short, e.g. "Boost undershoot in mid-RPM"
    description: str    # detail, including measured values
    affected_cells: list[tuple[float, float]]  # (x, y) bin centers
    map_name: str       # which map to look at
    suggested_delta: float | None
    confidence: float   # 0.0–1.0 based on sample count and consistency
```

#### Rules — TDI (PD)
- **R-TDI-01 Boost tracking error**: For each (RPM, requested_boost) cell with ≥10 samples, if `mean(|actual − requested|) / requested > 0.15` and sustained ≥0.5s, flag.
- **R-TDI-02 Smoke limiter clipping**: Detect when requested IQ exceeds delivered IQ AND `MAF / IQ_actual` is at the smoke limit boundary. Compute max safe IQ from MAF assuming AFR floor of 17:1 for power, 18:1 for safety.
- **R-TDI-03 EGT margin**: If logged, flag any cell sustaining EGT >720°C; suggest IQ pull. Hard alarm at >750°C.
- **R-TDI-04 Boost overshoot**: Detect spikes >115% of target lasting >0.3s; recommend softening boost target ramp in affected RPM band.
- **R-TDI-05 IQ ceiling clipping**: Identify (RPM, pedal) cells where the IQ-actual curve flattens below pedal demand — torque limiter is engaging.
- **R-TDI-06 Injection timing drift**: Compare actual SOI vs requested; flag persistent deltas.

#### Rules — Honda R18
- **R-HON-01 Knock retard accumulation**: Sum knock-retard events per (RPM, Load) cell. If sum >5° across session OR any single event >3°, recommend pulling base ignition by ~half the observed retard.
- **R-HON-02 AFR deviation**: Per cell, compute `mean(AFR_actual − AFR_target)`. If `|delta| > 0.3 AFR` in closed-loop region or `|delta| > 0.5` in open-loop, recommend VE correction.
- **R-HON-03 Long-term fuel trim drift**: If LTFT outside ±5% across multiple cells in a region, suggest base fuel-table correction.
- **R-HON-04 IAT compensation check**: Look for power loss correlated with rising IAT — flag if IAT-comp pull seems excessive.
- **R-HON-05 VTEC engagement smoothness**: Detect torque dips at VTEC crossover; suggest engagement-point adjustment.

### 4.4 Recommendation Engine
Converts issues into per-cell map deltas:

```python
@dataclass
class MapRecommendation:
    map_name: str
    cell_x: float
    cell_y: float
    current_value: float | None  # if user provided base map
    suggested_delta: float
    units: str
    rationale: str         # one-line human explanation
    confidence: float
    source_issues: list[str]  # rule_ids that contributed
```

Conflict resolution: if two rules touch the same cell, take the more conservative delta (smaller absolute change in the "safer" direction). Always log conflicts in the rationale.

### 4.5 Optional: Base Map Import
Allow user to optionally import current map values (CSV grid). If present, recommendations show absolute target values, not just deltas. Without it, deltas only.

## 5. UI / UX Specification

### Design Principles
- **Synoptic**: everything important visible at once, no nested menus or hidden state.
- **Minimalistic**: monochrome base palette, single accent color for warnings, no decorative iconography.
- **Information-dense but legible**: serious-tool aesthetic — think Reaper, MoTeC i2, Bloomberg terminal — not consumer software.
- **Dark mode by default**, light mode available.
- **Keyboard navigable**: arrow keys move cell selection on heatmaps, number keys switch tabs.
- **No modal dialogs** except for file pickers and confirm-on-destroy.

### Layout — Single window, four regions

```
+------------------------------------------------------------------+
| [Top Bar] Logo · Platform: [Fabia 1.4 TDI ▾] · Log: filename.csv |
+----------------+--------------------------------+----------------+
|                |                                |                |
|  SESSION       |   MAP HEATMAP                  |  CELL DETAIL   |
|  (left, 280px) |   (center, flex)               |  (right, 340px)|
|                |                                |                |
|  - File list   |   Tabs: Boost | IQ | SOI |     |  Selected:     |
|  - Coverage %  |         Smoke | (per platform) |  RPM 2750      |
|  - Issue list  |                                |  Load 78%      |
|    (severity   |   [2D heatmap, X=RPM,          |                |
|     coded)     |    Y=Load/Pedal]               |  Samples: 142  |
|  - Filters     |                                |  Mean boost:   |
|                |   Click cell -> details right  |   1.52 bar     |
|                |   Hover -> tooltip values      |  Target: 1.65  |
|                |                                |  Δ: -0.13 bar  |
|                |   Toggle: Values | Coverage |  |                |
|                |           Issues | Suggested  |  Recommendation:|
|                |                                |  +8% boost     |
|                |                                |  target here   |
|                |                                |  (conf 0.82)   |
+----------------+--------------------------------+----------------+
|  TIME-SERIES VIEWER (bottom, 240px, full width)                  |
|                                                                  |
|  Channel multi-select on left, brushable timeline,               |
|  events marked (knock, overshoot, EGT) as vertical lines.        |
|  Selecting a region updates the heatmap to "this slice only."    |
+------------------------------------------------------------------+
```

### Heatmap behavior
- Default cell binning: 500 RPM × 10% load (configurable).
- Cells with <5 samples are visually de-emphasized (low opacity) — never show recommendations from undersampled cells.
- Color modes:
  - **Values**: actual mean of the channel in that cell.
  - **Coverage**: sample count, useful for "where do I need more data?"
  - **Issues**: severity of any rules triggered in that cell.
  - **Suggested**: the recommended delta for the active map.
- Diverging colormap centered at zero for delta views; sequential for value views.

### Issue list (left panel)
- Sorted by severity then confidence.
- Click an issue → heatmap centers and highlights affected cells; time-series filters to relevant samples.
- Each issue collapsible to show full description.

### Export panel (top-right button)
- "Export recommendations" → produces:
  - `recommendations.json` (machine-readable, full structure)
  - `recommendations.md` (human-readable summary)
  - `<map_name>_suggested.png` for each affected map (heatmap rendered)
  - Optional: `<map_name>_delta.csv` (cell grid)

## 6. Tech Stack

- **Python 3.11+**
- **GUI**: PySide6 (LGPL, modern Qt, good for synoptic dashboards)
- **Data**: pandas, numpy
- **Plotting**: pyqtgraph for heatmaps and time-series (fast, native Qt; matplotlib too slow for live interaction)
- **Validation**: pydantic v2
- **Packaging**: PyInstaller for one-file builds on Windows/macOS/Linux
- **Tests**: pytest, with sample logs in `tests/fixtures/`
- **Lint/format**: ruff, black

No web stack. No Electron. Native desktop.

## 7. Project Structure

```
ecu_optimizer/
├── pyproject.toml
├── README.md
├── ecu_optimizer/
│   ├── __init__.py
│   ├── __main__.py              # entry point
│   ├── core/
│   │   ├── session.py           # Session, normalized data container
│   │   ├── issue.py             # Issue, severity types
│   │   ├── recommendation.py    # MapRecommendation, conflict resolution
│   │   ├── binning.py           # cell binning logic
│   │   └── analysis.py          # rule runner
│   ├── platforms/
│   │   ├── __init__.py          # platform registry
│   │   ├── base.py              # PlatformAdapter ABC
│   │   ├── fabia_tdi/
│   │   │   ├── adapter.py
│   │   │   ├── maps.json
│   │   │   └── rules.py
│   │   └── civic_r18/
│   │       ├── adapter.py
│   │       ├── maps.json
│   │       └── rules.py
│   ├── io/
│   │   ├── csv_loader.py
│   │   ├── exporter.py
│   │   └── basemap_loader.py
│   ├── gui/
│   │   ├── main_window.py
│   │   ├── widgets/
│   │   │   ├── session_panel.py
│   │   │   ├── heatmap_view.py
│   │   │   ├── cell_detail_panel.py
│   │   │   ├── timeseries_view.py
│   │   │   └── issue_list.py
│   │   ├── theme.py             # palette, fonts, dark/light
│   │   └── icons/
│   └── utils/
│       ├── units.py
│       └── stats.py
├── tests/
│   ├── fixtures/
│   │   ├── fabia_tdi_sample.csv
│   │   └── civic_r18_sample.csv
│   ├── test_adapters.py
│   ├── test_rules.py
│   └── test_recommendations.py
└── docs/
    ├── adding_a_platform.md
    └── analysis_rules.md
```

## 8. Data Contracts

### Canonical channel names (all SI-ish where possible)
| Name | Unit | Description |
|---|---|---|
| `time` | s | seconds from start |
| `rpm` | rpm | engine speed |
| `map_actual` | kPa abs | manifold pressure |
| `map_target` | kPa abs | requested boost |
| `maf` | g/s | mass airflow |
| `iq_actual` | mg/str | injection quantity (diesel) |
| `iq_requested` | mg/str | requested IQ (diesel) |
| `iat` | °C | intake air temp |
| `ect` | °C | coolant temp |
| `egt` | °C | exhaust gas temp (optional) |
| `afr_actual` | AFR | gasoline lambda × 14.7 |
| `afr_target` | AFR | commanded AFR |
| `knock_retard` | ° | ignition pulled |
| `ignition_advance` | ° BTDC | base + correction |
| `tps` | % | throttle position |
| `pedal` | % | accelerator position |
| `injection_timing` | ° BTDC | SOI |
| `vehicle_speed` | km/h | |

### Issue / recommendation JSON
Use the dataclass shapes in section 4. All exports are stable JSON v1.

## 9. UI/UX Concrete Details

- Font: Inter or system-ui, 13px base.
- Palette (dark mode):
  - Background `#0e0e10`
  - Surface `#17171a`
  - Border `#2a2a2f`
  - Text primary `#e6e6e8`
  - Text secondary `#9a9aa0`
  - Accent (single) `#e8a33d` for warnings, `#d65151` for critical
  - Diverging heatmap: blue `#3a78c4` ← neutral `#2a2a2f` → orange `#e8a33d`
- Window minimum: 1280×800. Sensible defaults at 1440×900.
- All panels resizable via splitters; layout state persists across launches.
- Status bar at the very bottom: hover-cell coordinates, sample counts, log timestamp under cursor.

## 10. Out of Scope (Do Not Build)

- Reading or writing ECU binary files. Repeat: no ECU comms, no flashing, no checksum patching.
- Real-time OBD logging. Only post-hoc CSV analysis.
- Cloud sync, accounts, telemetry.
- Mobile or web versions.
- Auto-applying changes anywhere. Recommendations are always advisory; the user applies them in their tuning tool of choice.

## 11. Safety / Disclaimer Behavior

On first launch, show a one-time dialog:
> This tool produces advisory recommendations from datalogs. It does not modify your ECU. All tuning changes carry risk of engine damage. Verify every change against your own judgment, your tuner's advice, and dyno/road safety procedures. Use at your own risk.

Each exported `recommendations.md` should include this disclaimer in its header.

## 12. Milestones

**M1 — Core skeleton (build first, end-to-end thin slice)**
- Project structure, pyproject, basic main window with empty panels.
- CSV loader for the Fabia TDI sample with one platform adapter.
- One rule (R-TDI-01 boost tracking).
- Heatmap renders with one toggle (values).
- Issue list shows the one rule's findings.
- No export yet.

**M2 — Full TDI rule set + recommendations + export**

**M3 — Honda R18 platform adapter and rule set**

**M4 — Time-series viewer with brushing and event markers**

**M5 — Base map import and absolute-value recommendations**

**M6 — Polish pass: theme, keyboard nav, persisted layout, packaging**

Build M1 first as a complete vertical slice before expanding horizontally. Don't stub the GUI — it should actually display real data from a real log by the end of M1, even if only one rule is implemented.

## 13. Sample Data

Generate two synthetic but realistic sample CSV logs in `tests/fixtures/` based on the channel tables above. The Fabia sample should include at least one boost-undershoot region and one smoke-limited region. The Civic sample should include knock retard events at high load and an AFR-lean spot. These drive both tests and the M1 demo.

## 14. Coding Conventions

- Type hints everywhere; mypy strict where reasonable.
- Dataclasses or pydantic models for all structured data — never raw dicts crossing module boundaries.
- All numeric thresholds in rules live in a `rules.py`-level config dict at the top of the module, not buried in function bodies, so they're tunable without hunting.
- Each rule is one function, signature `def check(session: Session) -> list[Issue]`. Pure, no side effects, testable in isolation.
- Docstrings on every public function explaining *why* the threshold is what it is (e.g., "750°C EGT chosen as alarm because cast iron manifold creep onset; piston crown damage risk rises sharply above this").

## 15. Deliverables

A working PySide6 application that:
1. Launches to a clean dark-mode synoptic dashboard.
2. Loads a Fabia TDI CSV via drag-and-drop.
3. Auto-detects the platform.
4. Runs all TDI rules.
5. Displays issues, heatmaps, cell details, time-series, all live and interactive.
6. Exports recommendations as JSON + Markdown + map PNGs.
7. Same for the Honda R18 platform.
8. Tests pass: `pytest` green.
9. Packaged as a one-file binary via PyInstaller for the user's OS.

Build it. Ask for clarification only on genuine ambiguities; default to the conservative, safety-biased interpretation everywhere else.
