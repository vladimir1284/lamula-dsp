# **LAMULA DSP — Project Plan**

**Project:** LAMULA DSP — Digital Signal Processor for the LAMULA™ Digital Receiver (successor to the Vesta DRX signal-processing stack) **Goal:** Replace the entire Vesta DRX signal-processing software with a clean-sheet, headless, open-source DSP written in Rust, running on a Linux SBC, fed by the FPGA digital receiver (LAMULA DRx) over **1GbE**, and driven entirely by the RCP — with no Hunt / Borland / VCL dependency. **Duration:** 8 months (34 weeks) **Team:** 6 (4 software engineers \+ 2 product/domain experts acting as QA) — the same team that builds the LAMULA RCP, working both projects simultaneously. **Delivery model:** AI-agent-accelerated, spec-and-test-driven development. **Month-8 success criterion:** the complete DSP validated end-to-end against the in-house signal simulator (analytic ground-truth \+ legacy-Vesta I/Q regression); field commissioning against the real FPGA receiver follows after month 8\.

---

## **Revision note (this update)**

This revision aligns the DSP plan with the updated RCP plan and the LAMULA DRx plan. Two sets of changes:

1. **The ORPG feed leaves the DSP and moves to the RCP.** The DSP no longer connects to ORPG, no longer encodes NEXRAD/Level-II/MDV/TITAN, and no longer owns the `DSP↔ORPG` contract. Rationale: the RCP is the node that manages the archiving of observations, and the DSP is headless. The DSP now delivers the **full moment set (the volumetric observation)** to the RCP; the RCP archives it as NEXRAD Level-II and feeds ORPG via a complete WSR-88D RDA emulation. The DSP's signal-processing core (moment estimation, polarimetry, clutter filtering, dealiasing, spectrum analyzer) is unchanged — only the output layer moves.  
2. **The acquisition link is 1GbE, not 10GbE.** Confirmed against the LAMULA DRx plan: the 250 MSPS raw stream is down-converted and decimated *inside the FPGA fabric* and never leaves the chip; only decimated complex I/Q (\~20 MB/s for four Rx channels at 125 m cells) crosses the wire, which fits comfortably in 1GbE. The firm-real-time constraint is therefore **compute** (moment estimation throughput), not the link; the heavy 10GbE line-rate ingest machinery (AF\_XDP / io\_uring zero-copy at line rate, aggressive kernel-buffer tuning) is no longer required, though bounded-queue backpressure and drop detection stay.

*Consequence:* the `DSP↔FPGA` contract is the same artifact the DRx plan calls `DRx↔DSP`; it is co-owned and frozen once, named consistently across both plans.

---

## **1\. Executive Summary**

LAMULA DSP is a clean-sheet design and build of the LAMULA™ signal-processing software: the real-time pipeline that turns decimated complex I/Q from the FPGA digital receiver into calibrated radar moments and polarimetric variables, the encoder that delivers the full moment set to the RCP, a raw-I/Q research archive, and a hardware-faithful signal simulator that stands in for the FPGA during development and acceptance.

This is the sixth-generation LADETEC signal processor and the component the LAMULA RCP plan refers to as "the external DRX." This project is the authoritative source of the **moment stream** the RCP consumes. It does **not** feed ORPG: the RCP owns the ORPG/product side (archive-as-Level-II and the by-radial RDA feed).

We discard the legacy technology stack entirely. There are no Hunt acquisition cards, no FIFO interface, no Borland/Embarcadero C++Builder, no VCL, no Indy, no ModBus-to-card glue and no GUI of any kind. The hardest real-time work (ADC at 250 MHz, digital down-conversion, decimation, trigger generation, SSI encoder reading) lives in the FPGA digital receiver and is out of scope here; this software receives decimated complex I/Q, ray by ray, over **1GbE** and performs firm-real-time moment estimation. The DSP is a headless Linux service: every parameter, command and status flows through the RCP.

The architecture's central principle is an Acquisition Abstraction Layer (AAL) with two interchangeable implementations behind one interface: a real-hardware adapter (FPGA over 1GbE) and a signal-simulator adapter. The whole pipeline runs identically against either, which is exactly what makes "validate on the simulator now, commission on the FPGA later" a sound delivery strategy, and what lets the two projects (DSP and RCP) proceed in parallel without blocking each other: each side simulates its counterpart across a frozen contract.

The plan is structured around four milestones (M1–M4) over five phases. The single largest residual risk is simulator and reference fidelity — because the acceptance gate is simulator-based, the DSP is only as validated as its ground-truth is faithful. For a signal processor this has a specific shape: correctness is measured against synthetic I/Q with analytically known moments and against replayed legacy-Vesta I/Q recordings. That risk is owned by the two product experts and managed explicitly throughout.

## **2\. Context & Objectives**

The FPGA digital receiver (16-bit-capable ADC, 250 MHz sampling, four IF Rx channels at 60 MHz, two Tx-burst channels, two Tx-reference outputs, four configurable triggers, two SSI encoder channels) exists as a separate open-hardware project (LAMULA DRx) and is well understood by the team. The objective is to replace all of the proprietary Vesta DRX signal-processing software with an independent Rust stack, so the radar can be received and processed into moments without the original vendor and without the obsolete acquisition hardware.

Objectives, in priority order:

1. Ingest decimated complex I/Q from the FPGA receiver over **1GbE**, ray by ray, with full pulse/ray metadata, through a clean, simulator-swappable interface.  
2. Estimate Doppler moments (UZ/Zuncorr, CZ/Zcorr, V, W) and quality indices (SQI, CCOR, SIG) within the accuracy spec (≤ 2 dBZ, ≤ 1 m/s).  
3. Estimate the full polarimetric suite (ZDR, ΦDP, KDP, LDR, ρHV) for the target Gematronik Doppler-polarimetric radar.  
4. Filter clutter (fixed clutter maps, time-domain IIR, frequency-domain DFT) and dealias velocity and range in software.  
5. Stream the **full moment set (the volumetric observation)** and the IF spectrum-analyzer feed to the RCP, which archives it as Level-II and feeds ORPG; archive raw I/Q for research.  
6. Be configured and controlled entirely from the RCP — no local GUI.  
7. Provide a signal simulator faithful enough to serve as the validation and acceptance platform.

## **3\. Scope**

### **3.1 In scope (Stage 1 — this project)**

* Acquisition Abstraction Layer (AAL) with a real-hardware adapter (FPGA digital receiver over **1GbE**) and a signal-simulator adapter behind one interface.  
* Signal simulator: synthesizes decimated complex I/Q at the AAL boundary for scripted weather/clutter/noise scenarios with known ground-truth moments, emulates ray/pulse metadata and SSI encoder positions, and supports fault injection (malformed frames, dropped rays, encoder glitches, frequency drift) for BITE testing.  
* Ingest pipeline: **1GbE** reception, frame decoding, per-ray assembly, buffering with bounded-queue backpressure and drop detection.  
* Tx-burst processing: measurement of Tx frequency/phase from the burst channels, phase correction of received I/Q, and an AFC correction estimate (the physical control of STALO/NCO and triggers lives in the FPGA/RCP; the DSP only estimates and reports the correction).  
* Doppler moment estimation: pulse-pair and spectral (FFT) methods for UZ, CZ, V, W (moments of order 0/1/2); quality indices SQI, CCOR, SIG.  
* Clutter filtering: fixed clutter maps, adaptive time-domain IIR filters, frequency-domain DFT (GMAP-class) filtering, clutter correction (CCOR).  
* Polarimetry: ZDR, ΦDP, KDP, LDR, ρHV for the dual-pol channels.  
* Dealiasing (software): Doppler velocity dealiasing (dual-PRF / staggered-PRT) and range dealiasing (multiple-trip recovery).  
* Range processing: 125 m / 250 m cell sizes; reflectivity mode to 460 km (1840 bins @250 m / 3680 @125 m); Doppler mode to 150 km (extendable to 230 km).  
* Scan-mode handling: Split Cut, Batch Cut, Doppler Cut.  
* IF spectrum analyzer: the DSP computes IF spectra on demand; the RCP displays them.  
* RCP control/config plane: runtime configuration of filters, thresholds (SQI/SIG/CCOR), dealiasing, scan mode, calibration constants, plus status/health and event/BITE reporting.  
* **Output to the RCP:** the **full moment stream** — the authoritative volumetric observation (all moments, all bins, per radial with metadata, at full precision) — plus the IF spectrum feed, over **1GbE**. The RCP performs the NEXRAD/Level-II encoding and the ORPG feed; the DSP does **not** encode ORPG/Level-II/MDV/TITAN, and resolution selection (8-/16-bit) moves to the RCP's Level-II encoder.  
* Raw I/Q archive: complex 16-bit time-series dump with metadata sidecar, for research (distinct from the RCP's volumetric-observation Level-II archive).  
* Calibration (Stage 1): ingestion and application of single-point and TX-power calibration constants supplied via the RCP.  
* Packaging: offline, air-gapped installer for the Linux SBC (systemd service); reproducible offline build.

### **3.2 Out of scope**

* FPGA digital-receiver hardware and firmware (schematics, PCB, VHDL/Verilog) — the separate LAMULA DRx open-hardware project; we assume full knowledge of and access to the acquisition interface.  
* Hard real-time signal acquisition (ADC, digital down-conversion, decimation, trigger generation, SSI encoder reading) — owned by the FPGA; the 250 MSPS raw stream never reaches the DSP.  
* Physical oscillator/trigger control (STALO/NCO tuning, power, temperature; trigger timing) — lives in the FPGA/RCP; the DSP only estimates AFC correction.  
* The RCP MMI and all visualization — a separate project; this software has no GUI.  
* **ORPG and the feed to it** — product generation is the separate LAMULA ORPG project; the **by-radial RDA feed (WSR-88D ICD 2620002, Level-II) is now owned by the RCP**. The DSP delivers moments only to the RCP and never talks to ORPG.  
* **NEXRAD / Level-II / MDV / TITAN encoding** — moved to the RCP's data-management layer.  
* Field commissioning against the real FPGA receiver — occurs after month 8; this project delivers a system validated on the simulator and commissioning-ready.  
* Security hardening / authentication — air-gapped private network, single operator, control mediated by the RCP.

### **3.3 Stage 2 / deferred (documented, not built now)**

CPU-architecture-specific GPU/accelerator offload; multi-radar / heterogeneous radar support; advanced calibration (RX linearity, power monitor, signal-generator/ITSG-driven workflows); on-box long-term moment archiving (beyond raw-I/Q research dumps); adaptive clutter-map learning; a **remote raw-I/Q archive host** — decoupling archive storage from the DSP process itself (network export/import of time-series data to a separate box), rather than the Stage-1 assumption of a single-box, offline-friendly archive. *(Output/distribution formats beyond the RCP feed — NETCDF/HDF5, MDV-by-volume to TITAN, additional custom formats — now belong to the RCP's distribution layer, not the DSP.)*

## **4\. System Architecture**

### **4.1 Overview**

A single headless Rust service runs on one Linux SBC on the operational network. It receives decimated complex I/Q from the FPGA digital receiver over **1GbE**, runs the signal-processing pipeline, and emits two output streams: the **full moment set (+ IF spectrum) to the RCP over 1GbE**, and a **research raw-I/Q archive**. It is configured exclusively by the RCP. Every stage above the AAL is agnostic to whether the I/Q comes from real steel or the simulator. The FPGA is the upstream source the DSP ingests from; the simulator emulates it during development and validation. The **RCP** archives the moments as Level-II and feeds ORPG — the DSP is not involved in that path.

### **4.2 Layered architecture**


### **4.3 Key design principles**

* **AAL swappability** — one interface, two adapters (real FPGA / simulator). Nothing above the AAL changes between simulator validation and field operation.  
* **Simulator as the acceptance oracle** — the signal simulator is a first-class, early, critical-path deliverable, not a test fixture afterthought. Acceptance in month 8 is defined against it, with two truth sources: analytically known synthetic moments and replayed legacy-Vesta I/Q.  
* **Clean, modern contracts** — `DSP↔FPGA` (the shared `DRx↔DSP` contract) and `DSP↔RCP` are explicit typed contracts with a single schema source and generated bindings. There is **no** `DSP↔ORPG` contract — the RCP owns the ORPG feed. No legacy protocols.  
* **Headless, RCP-driven** — no GUI; all configuration, control, status, the full moment stream and the IF spectrum-analyzer feed travel over the `DSP↔RCP` contract.  
* **Firm real-time, bounded latency** — hard real-time stays in the FPGA. The firm-real-time constraint is **compute** (moment-estimation throughput at the worst-case ray rate, PRF up to 1200 Hz × pulses/ray), **not** the link: decimated I/Q over 1GbE is well within budget because the 250 MSPS raw stream is decimated inside the FPGA and never crosses the wire. Bounded queues, backpressure and drop detection guard ingest; the DSP is never on the trigger path.  
* **Portable across CPU architecture** — Linux is fixed; ARM-vs-x86 is decided in Phase 0, so the code stays architecture-agnostic with a SIMD/FFT abstraction and is benchmarked on candidate boards.  
* **Decoupled from the RCP project** — frozen contracts plus mutual interface simulators let the same team build both stacks at once; this is the explicit mechanism for lowering cross-project responsibility load.

### **4.4 Component summary**

| Component | Responsibility |
| ----- | ----- |
| AAL (Acquisition Abstraction Layer) | Single ingest interface with real (1GbE FPGA) and simulator adapters |
| Signal Simulator | Synthetic I/Q with known ground-truth, encoder/metadata emulation, fault/BITE injection, scriptable scenarios |
| Ingest Pipeline | 1GbE reception, frame decode, per-ray assembly, backpressure, drop detection |
| Tx-Burst & AFC Estimator | Measures Tx frequency/phase, drives phase correction, reports AFC correction estimate |
| Moment Estimator | Pulse-pair \+ spectral estimation of UZ, CZ, V, W; quality indices SQI, CCOR, SIG |
| Clutter Filter | Fixed clutter maps, time-domain IIR, frequency-domain DFT (GMAP-class), CCOR |
| Polarimetry Engine | ZDR, ΦDP, KDP, LDR, ρHV |
| Dealiasing | Velocity (dual-PRF/staggered-PRT) and range (multi-trip) recovery |
| Range Processor | 125/250 m cells, bin assembly to max range per mode, scan-mode handling |
| IF Spectrum Analyzer | On-demand IF spectra for RCP display |
| Output to RCP | Full moment stream \+ IF spectrum to the RCP over 1GbE; raw-I/Q research archive. *(NEXRAD/Level-II/MDV/TITAN/ORPG encoding moved to the RCP.)* |
| Control/Config Plane | RCP-driven runtime configuration, calibration ingestion |
| Status & BITE Manager | Aggregates pipeline health; manages fault messages, filtering and history |

### **4.5 Core processing algorithms**

The algorithm set below defines what "moment quality comparable to a market-leading processor" means concretely for LAMULA DSP. The selection of *which* algorithms a Stage-1 processor must cover was informed by studying the functional scope of an established, market-leading weather-radar signal processor (RVP900-class). The **implementation** of each algorithm follows exclusively public literature and open-source reference implementations — never the vendor's proprietary manual text. Each entry links to a dedicated design page under `docs/algorithms/`.

* **[Pulse-pair moment estimation](algorithms/pulse-pair-moments.md)** — primary UZ/CZ/V/W estimator (autocovariance, lag-1), per Doviak & Zrnić and Zrnić (1977); spectral (FFT) estimation retained as a higher-cost fallback mode.
* **[GMAP clutter filtering](algorithms/gmap-clutter-filtering.md)** — spectral, adaptive ground-clutter suppression (Siggia & Passarelli, 2004) as the primary filter, with a time-domain IIR filter and static clutter maps as lower-cost/auxiliary modes.
* **[Dual-PRF velocity dealiasing](algorithms/dual-prf-dealiasing.md)** — extended unambiguous-velocity mode using alternating PRFs and spatial-continuity unfolding, per Joe & May and Holleman & Beekhuis.
* **[SZ(8/64) second-trip recovery](algorithms/sz-second-trip-recovery.md)** — phase-coded range-overlay separation (Sachidananda & Zrnić, 1999); recorded here as Stage-2/deferred pending excitation-hardware support for pulse-to-pulse phase coding.
* **[Reflectivity calibration chain](algorithms/reflectivity-calibration.md)** — radar-constant, receiver-gain and noise-floor calibration exposed as RCP-configurable parameters, plus built-in test-signal injection for periodic verification.

These map directly onto the Moment Estimator, Clutter Filter, Dealiasing and Control/Config Plane components in §4.4.

## **5\. Technology Stack**

All choices are mature, actively maintained, open-source, and buildable offline (vendored crates) for the air-gapped target.

| Layer | Choice | Rationale |
| ----- | ----- | ----- |
| Language | Rust (stable) | Memory safety without GC → predictable latency for sustained DSP throughput; fearless concurrency for the pipeline; portable across ARM/x86; auditable open-source (GPLv3+) |
| Runtime/OS | Linux (CPU arch TBD in Phase 0\) | Linux fixed; ARM-vs-x86 benchmarked on candidate SBCs before commit |
| Async / I/O | tokio | 1GbE ingest, network streams, control plane |
| Pipeline concurrency | thread-per-stage \+ crossbeam bounded channels (and/or rayon for data-parallel stages) | Backpressure, deterministic stage isolation |
| Numerics | ndarray, num-complex, nalgebra, statrs | Moment math, matrix ops, statistics |
| FFT / spectral | rustfft / realfft | Spectral moment estimation and the IF spectrum analyzer |
| SIMD | std::simd (portable) or wide/pulp | Hot-loop acceleration kept architecture-agnostic (NEON/AVX) — the real firm-real-time constraint is here, not the link |
| Contracts / schema | Protocol Buffers or Cap'n Proto (decision needed) | One schema → generated Rust \+ Python/TS bindings for the RCP (and the shared `DRx↔DSP` contract); mirrors the RCP's "single source → codegen" approach across language boundaries |
| Control-plane transport | TCP/WebSocket with schema'd messages | RCP-driven config/status; pairs with the RCP gateway |
| Data-plane transport | Compact binary framing (typed arrays) over TCP/UDP, 1GbE | Full moment \+ spectrum delivery to the RCP; research I/Q to local archive |
| Config | TOML via serde | Replaces legacy .ini; typed, validated, version-controlled |
| Logging / telemetry | tracing \+ metrics | Structured logs feed the BITE/event history reported to the RCP |
| Archive | Binary 16-bit complex I/Q \+ metadata sidecar | Research dumps; offline-friendly on a single box |
| Testing | cargo test, proptest, criterion, insta, cargo-fuzz | DSP invariants, perf-regression gates, robust frame decoding |
| Build / packaging | cargo vendor (offline), static musl binary or container, systemd unit; cross/cargo-zigbuild if ARM | One-box offline deploy; cross-compile path kept open |
| Dev workflow | AI coding agents \+ spec/test-first | Core to the team's strategy; see §7.2 |

## **6\. Interfaces & Contracts (defined in Phase 0\)**

Two contracts are designed up front and frozen early, because every workstream — and the parallel RCP and DRx projects — depends on them:

* **DSP ↔ FPGA (acquisition).** This is the same artifact the LAMULA DRx plan calls `DRx↔DSP`; it is co-owned with the DRx project and frozen jointly in Phase 0\. A **1GbE** wire format for decimated complex I/Q rays plus per-ray metadata (SSI azimuth/elevation, PRF, pulse width, pulse mode, trigger/timing, channel mapping for the four Rx \+ two Tx-burst channels), with status/BITE up and configuration \+ AFC correction down. The 250 MSPS raw stream is decimated inside the FPGA and never reaches the DSP. Validated by contract tests and the signal simulator.  
* **DSP ↔ RCP.** Bidirectional and owned by this project: control/config (filters, thresholds, dealiasing, scan mode, calibration constants), status/health and BITE events, the **full moment stream** (the authoritative volumetric observation — all moments, all bins, per radial with metadata, at full precision), and the IF spectrum-analyzer feed, over **1GbE**. Typed from a single schema, with bindings mirrored to the RCP's Python/TypeScript side. The RCP archives this stream as NEXRAD Level-II and feeds ORPG; resolution decimation (8-/16-bit) is the RCP's concern. This is the contract the RCP plan calls `RCP ↔ DRX/DSP`.

*Removed:* the former `DSP ↔ ORPG` contract. The RCP now owns the WSR-88D RDA emulation (ICD 2620002\) and the by-radial Level-II feed to ORPG; the DSP plays no part in it.

### **6.1 DSP ↔ RCP control-plane capability checklist**

The bullet above names the *categories* of the `DSP↔RCP` contract; this checklist pins down capabilities the schema (Phase 0\) must account for, so they aren't discovered mid-implementation. It was compiled by studying the *functional* shape of a mature signal processor's host interface (RVP900-class, Chapter "Host Computer Commands" and its developer's-notes appendix) — capabilities only, never RVP900's actual command syntax or wire format, consistent with the sourcing rule in `docs/index.md`.

* **Setup phase vs. running phase are distinct.** Applying configuration (filters, thresholds, dealiasing mode, scan mode, calibration constants) is a separate step from starting/stopping acquisition; the DSP validates and acknowledges a setup before entering the running phase, rather than accepting config changes implicitly mid-stream.  
* **Mandatory link self-test at connect.** Every time the RCP (re)connects, an interface self-test precedes trusting the link for control — not just a TCP/WebSocket handshake.  
* **Config is readable, not just writable.** Every runtime parameter the RCP can set (filter thresholds, dealiasing mode, calibration constants) is also readable on demand, so the RCP can confirm applied state rather than assuming a write succeeded.  
* **Capability-flags reporting.** The DSP reports which optional processing modes it currently supports (e.g. which dealiasing methods are enabled, spectral-vs-pulse-pair fallback availability) as part of status — not folded into a single up/down health bit.  
* **Data-completeness and drift telemetry, not just link health.** Per-ray bin-acquisition success counts (a data-completeness metric distinct from "link is up"), per-channel noise-floor and DC-offset readback, and trigger-period drift (measured vs. commanded PRF) all feed the Status & BITE Manager alongside fault/config-error/mode-flag severity tiers.  
* **Narrowband interference (RFI) filtering, as a capability distinct from ground-clutter filtering.** Flagged for a Phase-0 decision against the target radar's actual RFI environment — not committed Stage-1 scope by itself; if confirmed necessary it extends the Clutter Filter component (§4.4), not a new component.

**Moment vocabulary (canonical):** UZ (uncorrected reflectivity, Zuncorr), CZ (corrected reflectivity, Zcorr), V, W, ZDR, ΦDP, KDP, LDR, ρHV; quality indices SQI, CCOR, SIG; raw I, Q. This reconciles the legacy Vesta naming (dBZ/dBT) and the RCP naming (UZ/CZ) into one set used by both contracts and consistent with the RCP's Level-II encoding.

## **7\. Team, Roles & Delivery Model**

### **7.1 Roles**

The same six people who build the LAMULA RCP build this, splitting time across both projects; the interface simulators are what make simultaneous delivery feasible. DSP-side ownership:

| \# | Role | Primary ownership (DSP side) |
| ----- | ----- | ----- |
| 1 | Tech Lead / DSP Architect (eng) | Architecture, the two contracts, AAL \+ ingest pipeline, CI |
| 2 | DSP Engineer (eng) | Moment estimation, clutter filtering, range processing, scan modes |
| 3 | DSP Engineer — Sim & Data (eng) | Signal simulator, the full moment stream to the RCP, raw-I/Q archive. *(ORPG/Level-II/MDV encoding moved to the RCP project.)* |
| 4 | DSP Engineer — Polarimetry & Numerics (eng) | Dual-pol estimators, dealiasing, FFT/spectrum analyzer, SIMD/perf (also the RCP visualization lead) |
| 5 | Product Expert / QA Lead (domain) | Accuracy specs, reference-truth definition, calibration correctness, validation strategy |
| 6 | Product Expert / QA (domain) | Fault/BITE scenarios, dealiasing & scan-mode validation, test execution, documentation |

### **7.2 AI-accelerated delivery model**

* **Spec-and-test-first:** the product experts author precise accuracy and behaviour scenarios; engineers turn them into executable specs (cargo test, proptest, criterion baselines) before implementation, then agents implement against the tests.  
* **Simulator as deterministic oracle:** scripted simulator scenarios (with analytic ground-truth and injected faults) give agents and CI a reproducible source of truth; legacy-Vesta I/Q recordings provide regression truth.  
* **Generated contracts:** a single schema generates Rust and Python/TypeScript bindings, eliminating drift between the DSP and the RCP (and matching the shared `DRx↔DSP` contract).  
* **Repository conventions ("skills") and CI gates:** documented Rust conventions keep agent output consistent; CI enforces clippy/rustfmt, tests, contract tests and benchmark-regression gates on every change.

## **8\. Delivery Plan**

### **8.1 Cadence & methodology**

Two-week sprints (17 sprints across 34 weeks). Sprint demos to the product experts, who own acceptance. Continuous integration with mandatory lint, test, contract-test and performance-regression gates. A living Stage-2 backlog absorbs out-of-scope requests through lightweight change control.

### **8.2 Phases**

| Phase | Weeks | Focus | Exit milestone |
| ----- | ----- | ----- | ----- |
| 0 — Inception & Architecture | 1–3 | Freeze the two contracts (jointly with RCP/DRx); repo/CI/agent-workflow setup; signal-simulator architecture; spikes: 1GbE ingest, FFT/SIMD throughput (the real firm-real-time constraint), CPU/SBC benchmark (ARM vs x86), schema/IDL choice | Architecture & contracts baselined |
| 1 — Foundations & Signal Simulator | 4–10 | AAL interface \+ signal simulator (synthetic I/Q with ground-truth \+ scenario scripting \+ fault injection); 1GbE ingest \+ real-adapter skeleton; pipeline skeleton; first end-to-end: sim I/Q → uncorrected reflectivity ray → moment stream → simulated RCP consumer | M1 vertical slice |
| 2 — Doppler Moments, Filtering & Scanning | 11–18 | Pulse-pair \+ spectral UZ/CZ/V/W; SQI/CCOR/SIG; clutter filtering (fixed maps \+ IIR \+ DFT/GMAP); phase correction; Tx-burst measurement \+ AFC estimate; range processing (125/250 m, multi-trip); velocity dealiasing; Split/Batch/Doppler Cut | M2 Doppler suite on sim |
| 3 — Polarimetry, Outputs, Control & Archive | 19–27 | Dual-pol ZDR/ΦDP/KDP/LDR/ρHV; range dealiasing; full RCP control/config \+ the **full moment stream \+ IF spectrum feed to the RCP**; raw-I/Q archive; calibration ingestion | M3 full processing on sim |
| 4 — Hardening & Simulator Acceptance | 28–34 | Throughput tuning to worst-case PRF/range (compute-bound); endurance/soak; full fault-injection/BITE coverage; accuracy validation vs spec \+ legacy-Vesta regression; offline installer; ops docs; FPGA-commissioning dry-run plan | M4 simulator acceptance — commissioning-ready |

### **8.3 Milestones & acceptance criteria**

* **M1 — Vertical slice (end of W10).** The simulator emits synthetic I/Q frames with ray metadata; the AAL ingests them; the pipeline produces an uncorrected reflectivity ray; the moment stream reaches a simulated RCP consumer. Proves the end-to-end path and the AAL/contract design.  
* **M2 — Doppler suite on sim (end of W18).** On synthetic scenarios with known truth, the DSP recovers UZ, CZ, V and W with SQI/CCOR/SIG within the accuracy spec (≤ 2 dBZ, ≤ 1 m/s), applies clutter filtering and velocity dealiasing, and handles Split/Batch/Doppler Cut. The Tx-burst path produces a phase correction and an AFC estimate.  
* **M3 — Full processing on sim (end of W27).** The full polarimetric suite (ZDR, ΦDP, KDP, LDR, ρHV) plus range dealiasing; the RCP drives all configuration and receives the **full moment stream \+ IF spectrum-analyzer feed** (and on its own side archives it as Level-II / feeds ORPG); raw I/Q is archived.  
* **M4 — Simulator acceptance (end of W34, month 8).** The product-expert acceptance suite passes; accuracy is validated against analytic truth and legacy-Vesta regression; throughput sustains worst-case load (compute-bound); endurance and fault-injection campaigns pass; the offline installer deploys cleanly on a clean air-gapped Linux SBC; ops documentation and an FPGA-commissioning dry-run plan are delivered. The system is commissioning-ready.

## **9\. Work Breakdown by Workstream**

* **Architecture & Platform** — contracts (two), schema/codegen, repo, CI, agent conventions, offline packaging, observability.  
* **AAL & Simulator** — abstract ingest interface, real 1GbE adapter, signal simulator (synthetic I/Q \+ ground-truth, encoder/metadata emulation, fault/BITE injection, scriptable scenarios).  
* **Core DSP** — Tx-burst/AFC, phase correction, clutter filtering, moment estimation, quality indices, range processing, scan modes.  
* **Polarimetry & Dealiasing** — dual-pol variable estimation, velocity and range dealiasing, spectrum analyzer.  
* **Outputs & Control** — full RCP moment/spectrum stream (1GbE), RCP control/config plane, raw-I/Q archive, status/BITE. *(No ORPG encoders — moved to the RCP.)*  
* **Quality & Validation** — accuracy specs, reference-truth definitions, test harnesses, endurance/fault campaigns, documentation.

## **10\. Quality, Testing & Validation**

* Unit & property tests (cargo test, proptest) on every change; contract tests guard the two interfaces (`DRx↔DSP` and `DSP↔RCP`); fuzz tests (cargo-fuzz) harden the wire-frame decoders.  
* Benchmark-regression gates (criterion) on hot paths — throughput is a first-class acceptance dimension, and (with a 1GbE link) the binding constraint is compute, so the gates focus on the moment/FFT/polarimetry hot loops.  
* Scenario/accuracy suite authored by the product experts — moment and polarimetric accuracy against analytic ground-truth, clutter-rejection cases, a dealiasing matrix (velocity and range), and scan-mode correctness.  
* Legacy-Vesta regression — recorded real I/Q replayed through the new pipeline, comparing outputs against the trusted legacy processor.  
* Fault-injection / BITE campaigns via the simulator's fault hooks (malformed frames, dropped rays, encoder glitches, drift).  
* Endurance/soak runs (long unattended processing at worst-case PRF/range) in Phase 4\.  
* Simulator-fidelity register — explicitly catalogues known simulator-vs-real and analytic-vs-recorded deltas so they convert directly into the FPGA commissioning test plan.

## **11\. Risk Register**

| Risk | L | I | Mitigation |
| ----- | ----- | ----- | ----- |
| Simulator / reference fidelity (acceptance is sim-only) | Med | High | Product-expert-owned fidelity criteria; analytic ground-truth and legacy-Vesta regression; explicit delta register feeding the commissioning test plan |
| DRx↔DSP acquisition-interface dependency (external, co-developed) | Med | High | Freeze the `DRx↔DSP` contract in Phase 0; signal simulator; contract tests; co-design with the DRx team |
| CPU/SBC target undecided → performance risk | Med | Med | Phase-0 benchmarking on candidate boards; architecture-agnostic Rust \+ SIMD/FFT abstraction; criterion gates; sized headroom |
| Sustained moment-estimation throughput at max PRF/range (compute-bound firm real-time) | Med | High | Early FFT/estimator spikes; bounded-queue backpressure; benchmark gates on hot loops; sized compute headroom |
| 1GbE ingest integrity | Low | Med | Decimated I/Q is well within 1GbE; bounded-queue backpressure; drop detection \+ telemetry. The heavy 10GbE line-rate machinery (AF\_XDP/io\_uring zero-copy, aggressive kernel tuning) is no longer required |
| Polarimetric estimator correctness (KDP/ΦDP/ρHV) | Med | High | Product-expert reference cases; literature-standard estimators; validation vs known-truth \+ legacy |
| Dealiasing correctness (velocity & range) | Med | Med | Scripted ambiguous scenarios with known truth; dual-PRF/staggered-PRT test matrix |
| Cross-project coupling with RCP (same team, simultaneous) | Med | Med | Frozen contracts \+ mutual interface simulators; shared contract work done once; the ORPG feed now lives in the RCP plan (single owner); phased milestones |
| Offline/air-gapped packaging (vendored crates, cross-compile) | Low | Med | cargo vendor \+ offline build tested in Phase 1, not Phase 4 |
| Team load (six people across RCP \+ DSP) | Med | Med | Interface simulators decouple the projects; removing ORPG encoding here offsets the heavier `DSP↔RCP` stream; AI-accelerated delivery; phased milestones; Stage-2 backlog |

## **12\. Assumptions & Dependencies**

* The FPGA digital receiver delivers correct decimated complex I/Q over **1GbE** (the `DRx↔DSP` contract), agreed early; the 250 MSPS raw stream is decimated inside the FPGA and never leaves the chip. The FPGA owns hard real-time (ADC, DDC, decimation, triggers, SSI encoder reading).  
* Hard real-time (triggering, PRF, pulse timing) is guaranteed by the FPGA, not by the Rust DSP.  
* Physical control of STALO/NCO and triggers lives in the FPGA/RCP; the DSP's responsibility is limited to measuring the Tx burst and reporting an AFC correction estimate.  
* The RCP is the sole client: it provides control/config, receives the full moment stream \+ IF spectrum feed, and owns the archive-as-Level-II and the ORPG feed. The DSP has no GUI and does **not** talk to ORPG.  
* Calibration constants are provided and managed via the RCP.  
* Legacy-Vesta I/Q recordings are available as the regression-truth source (to be confirmed and collected in Phase 0).  
* Linux is fixed; the CPU architecture (ARM vs x86) is decided in Phase 0; the codebase stays portable.  
* The target Stage-1 radar is a single Gematronik Doppler-polarimetric model, single air-gapped network, no security requirements.  
* The dev/CI environment has package access (or an internal mirror); the deployment target is offline.  
* The same team builds the RCP and the DSP simultaneously; the contracts are frozen early and mutual interface simulators decouple the two efforts.

## **13\. Definition of Done (Month 8\)**

The Rust DSP service, running headless on a clean air-gapped Linux SBC with the signal simulator behind the AAL, ingests synthetic I/Q over the acquisition interface (1GbE), computes the full Doppler and polarimetric suite (UZ, CZ, V, W, ZDR, ΦDP, KDP, LDR, ρHV) with clutter filtering, velocity and range dealiasing and quality indices within the accuracy spec (≤ 2 dBZ, ≤ 1 m/s), and **streams the full moment set (the volumetric observation) and IF spectra to the RCP over 1GbE — which the RCP archives as Level-II and feeds to ORPG** — and archives raw I/Q, all configured entirely from the RCP — with the product-expert acceptance suite green, accuracy validated against analytic truth and legacy-Vesta regression, endurance and fault-injection campaigns passed, an offline installer validated, ops documentation delivered, and an FPGA-commissioning dry-run plan ready. Real-hardware commissioning begins after month 8 by swapping the AAL's simulator adapter for the real FPGA-over-1GbE adapter (the `DRx↔DSP` link).

