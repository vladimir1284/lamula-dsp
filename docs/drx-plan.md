# **LAMULA DRx — Project Plan**

**Project:** LAMULA DRx — FPGA Digital Receiver firmware for the LAMULA™ system (the acquisition front end the DRX-DSP plan treats as its upstream source) **Goal:** Build a clean-sheet, open-source digital receiver on a Zynq UltraScale+ platform: ADC capture over JESD204B, digital down-conversion (DDC) in the FPGA fabric, ray assembly on the processor under FreeRTOS, and delivery of decimated complex I/Q to the DSP over 1GbE — with no legacy dependency. **Duration:** 8 months (34 weeks) **Team:** 3 — one principal developer (HDL \+ embedded firmware) plus two specialists acting as QA/domain. **Delivery model:** AI-agent-accelerated, spec-and-test-driven, Xilinx-IP-first. **Month-8 success criterion:** the receiver validated end-to-end on the bench (DAC→ADC loopback \+ injected digital vectors) and integrated with the DSP project against the frozen DRx↔DSP contract; connection to the real radar IF/antenna follows after month 8\.

---

## **1\. Executive Summary**

LAMULA DRx is the sixth-generation LADETEC digital receiver, the hardware/firmware front end that the LAMULA™ DRX-DSP plan consumes. It digitizes the radar IF, performs digital down-conversion and decimation in the FPGA fabric, assembles range-gated complex I/Q into rays on the processor, and streams those rays to the DSP. Everything from the gateware to the embedded firmware is open source.

This project sits one layer below the DSP. The DSP plan defined a "DSP↔FPGA acquisition" contract and simulated the FPGA with a signal simulator; this project is the real implementation of that contract and the thing the DSP eventually commissions against. Its **single external interface is DRx↔DSP**: decimated I/Q rays and status flow up to the DSP, configuration and control flow down (originating at the RCP and relayed by the DSP). The DRx never talks to the RCP or ORPG directly.

The hardware baseline is a **Zynq UltraScale+ ZU9 carrier with two VadaTech FMC213 mezzanines** (each a quad 16-bit, 250 MSPS ADC over JESD204B plus a wideband DAC), giving the eight ADC channels needed for four IF Rx channels and two Tx-burst channels, plus two DAC channels for the Tx reference. The hardest real-time work lives in the **programmable logic (PL)**: the JESD204B ADC interface, the DDC chain, the trigger/timing engine and the SSI encoder readers. The **processor (PS)** runs **FreeRTOS** on a real-time core and is deliberately thin — it assembles rays from DMA buffers, tags them with metadata, and ships them over GEM 1GbE.

The plan is structured around four milestones (M1–M4) over five phases. The two largest residual risks are **JESD204B link/clocking bring-up** (the classic hard part of these boards, and hardware-reality-bound rather than agent-accelerable) and the small team. Both are mitigated by leaning hard on mature Xilinx IP and by the on-board DAC→ADC loopback, which provides a self-contained correctness oracle from day one.

## **2\. Context & Objectives**

The analog radar front end (IF chain at 60 MHz, transmitter, antenna/servo, sensors) exists and is well understood. The objective is to replace the legacy acquisition hardware and software with an independent Zynq-based receiver that delivers calibrated, range-gated complex I/Q to the DSP.

Objectives, in priority order:

1. Bring up the JESD204B ADC link and clocking on the ZU9 \+ FMC213 platform with deterministic, multi-channel-aligned capture.  
2. Implement DDC (NCO \+ decimation) in the PL, producing complex baseband I/Q at the range-gate rate for 125 m / 250 m cells.  
3. Generate the radar timing in the PL: four configurable triggers, PRF 200–1200 Hz, pulse widths 0.8 / 1.66 / 3.3 µs.  
4. Read the two SSI position encoders and tag each ray with azimuth/elevation.  
5. Assemble rays on the PS under FreeRTOS and stream them to the DSP over the DRx↔DSP contract on GEM 1GbE.  
6. Synthesize the Tx reference via DAC and apply the AFC correction (NCO adjustment) estimated by the DSP.  
7. Be configured and controlled entirely through the DSP (no local UI); report status and BITE.

## **3\. Scope**

### **3.1 In scope (Stage 1 — this project)**

* **Platform bring-up:** ZU9 carrier \+ 2× FMC213; clocking tree, PLL and **SYSREF** for JESD204B; power/boot/provisioning.  
* **JESD204B ADC interface:** Xilinx JESD204B IP over GTH transceivers for the four ADS42JB69 channels per module (eight channels total); link establishment, lane alignment, deterministic latency, multi-device synchronization.  
* **DDC chain (PL):** per-channel NCO/mixer (DDS Compiler) \+ CIC (CIC Compiler) \+ FIR (FIR Compiler) decimation to the 125 m / 250 m gate rate; gain/scaling.  
* **Range gating:** bin assembly to max range per mode (reflectivity to 460 km — 1840 bins @250 m / 3680 @125 m; Doppler default 150 km, extendable to 230 km).  
* **Timing/trigger engine (PL):** four configurable trigger outputs, PRF 200–1200 Hz, pulse-width selection (0.8 / 1.66 / 3.3 µs), pulse-mode handling.  
* **SSI encoder readers (PL):** two channels (azimuth, elevation), timestamped and tagged per ray.  
* **PL↔PS data path:** AXI4-Stream from the DDC → **AXI DMA (scatter-gather, S2MM)** → PS DDR ring buffers.  
* **PS firmware (FreeRTOS):** ray assembly (decimated I/Q \+ metadata: az/el, PRF, pulse mode, pulse width, trigger count, timestamps), DRx↔DSP framing, network TX via lwIP over GEM 1GbE; control/config command handling; status & BITE.  
* **Tx-reference DAC path:** AD9129 interface from the PL; NCO synthesis of the Tx reference on the two DAC channels.  
* **AFC actuation:** apply the DSP-supplied frequency correction by adjusting the DDC NCO (and the Tx-reference NCO where applicable).  
* **Scan-mode acquisition framing:** Split Cut, Batch Cut, Doppler Cut.  
* **Calibration hooks:** application of gain/offset constants supplied via the DSP.  
* **Diagnostics:** windowed raw-ADC capture to DDR for bring-up and DDC validation (a debug feature, not a streaming output).  
* **Packaging:** reproducible offline build (Tcl/non-project flow), boot image and field-provisioning procedure.

### **3.2 Out of scope**

* **Analog front end / IF chain / transmitter / antenna-servo** — existing hardware; we interface to the IF and to the SSI encoders.  
* **Signal processing (I/Q → moments)** — the DSP project; we deliver decimated I/Q rays.  
* **RCP and ORPG** — separate projects; the DRx never talks to them directly.  
* **Continuous raw-ADC streaming** — link-limited (one channel of raw 250 MSPS ≈ 2 Gbps exceeds GEM 1GbE; the decimated I/Q is the product). Raw capture is windowed-to-DDR for diagnostics only.  
* **AFC estimation** — performed by the DSP; the DRx only actuates the correction.  
* **Connection to the real radar IF/antenna and on-radar commissioning** — after month 8\.

### **3.3 Stage 2 / deferred (documented, not built now)**

PetaLinux/AMP option on the PS; second carrier / module scaling; on-board long-term capture-to-SSD; advanced self-calibration; JESD204C migration; alternative carriers or non-FMC213 digitizers; redundant/hot-standby acquisition.

## **4\. System Architecture**

### **4.1 Overview**

A single ZU9 board with two FMC213 mezzanines is the receiver. The PL captures the four Rx IF channels and two Tx-burst channels over JESD204B, down-converts and decimates them, gates them into range bins, and streams the resulting complex I/Q over AXI4-Stream through AXI DMA into PS DDR. The PS, under FreeRTOS, assembles rays with their metadata and sends them to the DSP over GEM 1GbE. Configuration and the AFC correction arrive from the DSP over the same link; the PL also drives the Tx-reference DACs.

### **4.2 Layered architecture**


### **4.3 Key design principles**

* **Xilinx-IP-first** — JESD204B, DDS/CIC/FIR Compilers, AXI DMA, AXI Timer/GPIO. Custom HDL is reserved for the timing engine, encoder readers, ray framing glue and anything no IP covers. This is what makes the scope feasible for a three-person team.  
* **Hard real-time in the PL, thin PS** — the trigger/timing path, JESD capture and DDC are in fabric; the PS only moves decimated data and metadata. FreeRTOS runs on a real-time core; the PS is never on the trigger path.  
* **On-board oracle** — the FMC213's DAC→ADC loopback lets us synthesize a known IF and verify the DDC output without the radar, from the first weeks.  
* **Single external contract** — DRx↔DSP only, frozen in Phase 0 jointly with the DSP project; the DRx is decoupled from the RCP and ORPG entirely.  
* **Reproducible gateware** — Tcl/non-project Vivado flow under version control so CI rebuilds the bitstream deterministically and agents work against a scripted design.  
* **Decimated I/Q is the product** — the 250 MSPS real stream never leaves the chip; only complex baseband I/Q at the gate rate is exported, which is why GEM 1GbE suffices (\~20 MB/s for four Rx channels at 125 m cells).

### **4.4 Component summary**

| Component | Responsibility |
| ----- | ----- |
| Clocking & SYSREF | Reference/PLL tree and SYSREF distribution for JESD204B device sync |
| JESD204B RX | Xilinx IP \+ GTH PHY; lane alignment, deterministic latency, multi-channel sync |
| DDC chain | Per-channel NCO \+ CIC \+ FIR decimation to 125/250 m gate rate |
| Range Gating | Bin assembly to max range per mode |
| Timing/Trigger Engine | Four triggers, PRF, pulse-width and pulse-mode generation |
| SSI Encoder Readers | Azimuth/elevation acquisition and per-ray tagging |
| AXI DMA Path | AXI4-Stream → S2MM DMA → PS DDR ring buffers |
| Ray Assembler (PS) | Decimated I/Q \+ metadata → ray frames |
| DRx↔DSP Transport (PS) | Framing \+ lwIP TX/RX over GEM 1GbE |
| Control/Config Handler (PS) | Applies DSP/RCP configuration; AFC actuation |
| DAC / Tx-Reference Path | NCO synthesis of Tx reference on the two DAC channels |
| Status & BITE (PS) | Link/DMA/timing health, fault messages, history |
| Diagnostics | Windowed raw-ADC capture to DDR |

### **4.5 Hardware support for DSP processing algorithms**

Several DSP-side algorithms (see the LAMULA DSP plan, §4.5, and `docs/algorithms/`) place requirements on the DRx that are worth calling out explicitly here so they are not lost as a DSP-only concern:

* **[Reflectivity calibration](algorithms/reflectivity-calibration.md)** requires the DRx to support test-signal injection at the receiver input (DAC→ADC loopback, §4.3) with known, traceable power, so the DSP can verify receiver gain/linearity end to end — already covered by the on-board oracle design principle.
* **[GMAP clutter filtering](algorithms/gmap-clutter-filtering.md)** and **[pulse-pair moment estimation](algorithms/pulse-pair-moments.md)** are downstream consumers of the decimated I/Q the DRx exports; no additional DRx capability is required beyond timing/phase fidelity of the DDC chain already in scope.
* **[SZ(8/64) second-trip recovery](algorithms/sz-second-trip-recovery.md)** would require the Tx-reference DAC path to synthesize a pulse-to-pulse phase-coded waveform, not just a fixed-phase reference. This is **not** required for Stage 1; it is flagged here as a DAC/Tx-Reference Path constraint to revisit if/when that algorithm is pulled forward from Stage 2.

## **5\. Technology Stack**

| Layer | Choice | Rationale |
| ----- | ----- | ----- |
| SoC | Zynq UltraScale+ MPSoC (ZU9) | Quad A53 \+ dual R5 \+ UltraScale+ fabric \+ GTH transceivers for JESD204B |
| Digitizer | 2× VadaTech FMC213 | Quad 16-bit 250 MSPS ADC (ADS42JB69, JESD204B) \+ AD9129 DAC per module |
| Gateware (PL) | VHDL/Verilog \+ Xilinx IP | JESD204B, DDS/CIC/FIR Compilers, AXI DMA, AXI Timer/GPIO; minimal custom HDL |
| FPGA toolchain | Vivado (Tcl/non-project flow) | Reproducible, scriptable, CI- and agent-friendly bitstream builds |
| HDL verification | Vivado Simulator / Verilator \+ cocotb (Python testbenches); optional VUnit | Vector-driven DDC/timing verification; Python TBs pair well with AI-accelerated workflow |
| Constraints | XDC (timing, pinout, JESD/clocking) | Timing closure and board pinout |
| PS firmware | C/C++ on FreeRTOS, Vitis BSP | Real-time ray assembly on an R5 core; standard Xilinx embedded path (Rust is the DSP's language, not used here) |
| Embedded networking | lwIP over GEM 1GbE | Sufficient for decimated rays; raw/sockets API |
| PL↔PS | AXI4 (AXI4-Stream \+ AXI DMA SG; AXI-Lite control regs) | Simplest IP-supported path for streaming samples to DDR |
| Contract codec | Compact binary ray/control framing (shared schema with the DSP) | Single source of truth for DRx↔DSP, generated for both sides |
| Build/CI | Vivado batch builds, HDL lint, sim regression, firmware build, boot-image assembly | Deterministic offline builds; gates on every change |
| Dev workflow | AI coding agents \+ spec/test-first \+ IP reuse | Core to feasibility for a three-person team; see §7.2 |

## **6\. Interfaces & Contracts (defined in Phase 0\)**

* **DRx ↔ DSP** (the only external contract; owned jointly with the DSP project, frozen in Phase 0). Up: decimated complex I/Q rays plus per-ray metadata (azimuth/elevation from the SSI encoders, PRF, pulse width, pulse mode, trigger count, timestamps, channel mapping), and status/BITE. Down: configuration (NCO/DDC frequency, decimation/cell size, range extent, PRF, pulse width, trigger config, gain/calibration constants, scan mode) and the AFC correction estimate. This is the field-level realization of the DSP plan's "DSP↔FPGA acquisition" contract — now 1GbE rather than 10GbE, since decimated I/Q is the only product on the wire.

Internal interfaces frozen alongside it: the **JESD204B/clocking** parameters (lane rate, SYSREF, sync), the **AXI4-Stream ray format** PL→PS, and the **AXI-Lite register map** for PL control.

## **7\. Team, Roles & Delivery Model**

### **7.1 Roles**

| \# | Role | Primary ownership |
| ----- | ----- | ----- |
| 1 | Principal Developer (HDL \+ embedded) | PL gateware (JESD204B, DDC, timing engine, encoders, DMA), PS FreeRTOS firmware, DRx↔DSP transport, CI |
| 2 | Specialist / QA (domain) | Acceptance specs, DDC/decimation and timing correctness, calibration and AFC-actuation validation, test execution |
| 3 | Specialist / QA (domain) | Bench validation (loopback, vector injection), fault/BITE scenarios, DSP-integration testing, documentation |

The principal developer carries the build; the two specialists own correctness, bench validation and integration with the DSP. AI agents amplify the single developer across HDL, firmware and tests.

### **7.2 AI-accelerated delivery model**

* **Spec-and-test-first:** the specialists author precise acceptance scenarios (loopback signals with known frequency/amplitude → expected DDC output; timing/PRF matrices); they become executable testbenches (cocotb) and firmware tests before implementation.  
* **On-board loopback as deterministic oracle:** synthesize known IF on the DAC, capture on the ADC, assert the decimated I/Q — reproducible and self-contained.  
* **IP-first generation:** agents wire and configure Xilinx IP and write the glue/timing/framing logic against the testbenches, rather than reimplementing converters.  
* **Reproducible, scripted flow and CI gates:** the Tcl/non-project Vivado flow plus firmware build run in CI with HDL lint, simulation regression and contract tests on every change.

## **8\. Delivery Plan**

### **8.1 Cadence & methodology**

Two-week sprints (17 sprints across 34 weeks). Sprint demos to the specialists, who own acceptance. CI with mandatory lint, simulation-regression and contract-test gates; bitstream and boot image rebuilt deterministically. A living Stage-2 backlog absorbs out-of-scope requests through lightweight change control.

### **8.2 Phases**

| Phase | Weeks | Focus | Exit milestone |
| ----- | ----- | ----- | ----- |
| 0 — Inception & Architecture | 1–3 | Freeze DRx↔DSP, AXI4-Stream ray format and register map; Vivado/Vitis project \+ CI; **JESD204B \+ clocking/SYSREF spike on the real board**; DDC IP architecture; agent/Tcl-flow conventions | Architecture & contracts baselined |
| 1 — Platform & Single-Channel Capture | 4–10 | JESD204B link up \+ clocking on HW; windowed raw-ADC capture to DDR; single-channel DDC (sim \+ HW); AXI DMA to PS; FreeRTOS \+ lwIP skeleton; first decimated I/Q in the PS | M1 single-channel I/Q verified by loopback |
| 2 — Multi-Channel, Timing & Rays | 11–18 | All channels (4 Rx \+ 2 burst); timing/trigger engine (4 triggers, PRF, pulse widths); SSI encoder readers \+ per-ray az/el tagging; range gating 125/250 m; ray assembly \+ DRx↔DSP framing \+ 1GbE TX to the DSP | M2 full rays streamed to the DSP |
| 3 — Tx Reference, AFC, Scan Modes & Control | 19–27 | DAC Tx-reference NCO synthesis; AFC actuation from the DSP estimate; scan-mode framing (Split/Batch/Doppler Cut); full control/config plane; calibration application; status/BITE | M3 full DRx capability, AFC loop closed with DSP |
| 4 — Hardening & Acceptance | 28–34 | Timing closure; throughput/latency; endurance/soak; fault injection (JESD loss, DMA overrun, encoder glitch); validation vs spec \+ DSP-integration consistency; offline boot image \+ provisioning; docs; on-radar commissioning dry-run plan | M4 bench \+ DSP-integration acceptance — commissioning-ready |

### **8.3 Milestones & acceptance criteria**

* **M1 — Single-channel capture (end of W10).** The JESD204B link is up and aligned; a known IF injected via the DAC→ADC loopback is captured, down-converted and decimated; the decimated I/Q reaches the PS over AXI DMA and matches the expected frequency/amplitude. Proves the clocking, JESD, DDC and DMA spine.  
* **M2 — Full rays to the DSP (end of W18).** All four Rx and two Tx-burst channels capture concurrently; the timing engine produces the four triggers at the configured PRF and pulse widths; the encoders tag az/el; rays (range-gated I/Q \+ metadata) are assembled and streamed over GEM 1GbE to the DSP (real or its simulator) per the frozen contract.  
* **M3 — Full capability (end of W27).** The Tx reference is synthesized on the DACs; the DSP-supplied AFC correction is applied to the NCO and verified via loopback; Split/Batch/Doppler Cut framing works; the DSP configures every parameter through the DRx↔DSP contract; status and BITE report correctly.  
* **M4 — Acceptance (end of W34, month 8).** Timing closes; throughput and latency meet the worst-case PRF/range budget; endurance and fault-injection campaigns pass; the receiver is validated against spec on the bench and integrated consistently with the DSP; the offline boot image provisions a clean board; documentation and an on-radar commissioning dry-run plan are delivered. Connection to the real radar IF/antenna begins after month 8\.

## **9\. Work Breakdown by Workstream**

* **Platform & Clocking** — board bring-up, PLL/SYSREF, boot, provisioning, reproducible Vivado/Vitis flow, CI.  
* **JESD204B & Capture** — IP integration, GTH PHY, lane alignment, multi-device sync, raw-capture diagnostics.  
* **DDC & Range Gating** — DDS/CIC/FIR chain, decimation to 125/250 m, gain/scaling, bin assembly.  
* **Timing, Triggers & Encoders** — trigger/PRF/pulse-width engine, scan-mode framing, SSI readers and per-ray tagging.  
* **Data Path & Firmware** — AXI DMA, FreeRTOS ray assembly, DRx↔DSP transport, control/config, status/BITE.  
* **DAC & AFC** — Tx-reference NCO synthesis, AFC actuation, calibration application.  
* **Quality & Validation** — loopback/vector-injection harnesses, sim regression, fault campaigns, DSP-integration tests, documentation.

## **10\. Quality, Testing & Validation**

* **HDL simulation** (cocotb/Verilator/Vivado sim) on DDC, timing and framing with vector stimuli on every change.  
* **DAC→ADC loopback** as the primary on-board oracle: known IF in, asserted decimated I/Q out.  
* **Digital vector injection** at the ADC/JESD boundary for deterministic DDC/decimation tests independent of analog.  
* **Acceptance suite** authored by the specialists — DDC accuracy, decimation/cell-size correctness, a PRF×pulse-width timing matrix, encoder tagging, scan-mode framing.  
* **DSP-integration consistency** — rays consumed by the DSP (and cross-checked against its signal simulator) to confirm the contract end-to-end.  
* **Fault-injection / BITE** — JESD link loss, DMA overrun, encoder glitch, clock loss.  
* **Endurance/soak** — sustained capture at worst-case PRF/range in Phase 4\.  
* **Bench-vs-radar delta register** — catalogues what loopback/sim cannot exercise (real IF levels, noise, antenna dynamics) to feed the on-radar commissioning plan.

## **11\. Risk Register**

| Risk | L | I | Mitigation |
| ----- | ----- | ----- | ----- |
| JESD204B link/clocking bring-up (hardware-bound, not agent-accelerable) | Med | High | Phase-0 spike on the real board; Xilinx JESD IP \+ reference designs; SYSREF/clocking validated before anything depends on it |
| Small team (one developer carries HDL \+ firmware) | Med | High | IP-first to minimize custom logic; AI acceleration; two specialists own validation; phased milestones; Stage-2 backlog |
| DRx↔DSP contract dependency (external project, co-developed) | Med | High | Freeze the contract in Phase 0; DSP simulates the FPGA; contract tests both sides |
| Timing closure / multi-channel sync at 250 MSPS | Med | Med | Conservative floorplanning; IP-based DDC; early timing runs in CI; XDC discipline |
| Tx-reference DAC path (high-speed AD9129) complexity | Med | Med | Treat as its own subsystem; loopback validation; phase it in M3, not on the M1/M2 critical path |
| Raw diagnostics misread as a streaming product (link budget) | Low | Med | Decimated I/Q is the only product on the wire; raw is windowed-to-DDR diagnostics only |
| On-radar assumptions wrong at commissioning (deferred) | Med | High | Bench-vs-radar delta register; documented IF/encoder assumptions; commissioning dry-run plan in Phase 4 |
| Offline/reproducible gateware build friction | Low | Med | Tcl/non-project flow \+ boot image built and tested in Phase 1, not Phase 4 |

## **12\. Assumptions & Dependencies**

* Hardware baseline is a ZU9 carrier with **2× FMC213** (8 ADC channels: 4 Rx \+ 2 Tx-burst \+ 2 spare; 2 DAC channels for Tx reference). To be confirmed if the build uses a single module with multiplexing.  
* The analog IF chain presents 60 MHz IF compatible with 250 MSPS capture; the two SSI encoders are accessible and their protocol/timing are known.  
* Hard real-time (triggering, PRF, pulse timing, capture, DDC) is owned by this receiver's PL; the PS is not on the trigger path.  
* The DSP is the DRx's only network peer; configuration originates at the RCP and is relayed by the DSP; AFC is **estimated by the DSP and actuated by the DRx**.  
* The DRx↔DSP contract is agreed and frozen early, jointly with the DSP project.  
* Decimated complex I/Q is the only product on the link; raw 250 MSPS samples never leave the chip except as windowed diagnostic captures.  
* The dev environment has Xilinx tool and IP access; the deployment target is offline.  
* This project runs in parallel with the RCP and DSP projects over the same 8-month horizon, with the DRx↔DSP contract as the synchronization point.

## **13\. Definition of Done (Month 8\)**

The LAMULA DRx, running on a clean-provisioned ZU9 \+ 2× FMC213 board, brings up the JESD204B link and clocking, captures the four Rx and two Tx-burst channels at 250 MSPS, down-converts and decimates them in the PL to range-gated complex I/Q for 125 m / 250 m cells, generates the four triggers at the configured PRF and pulse widths, tags each ray with azimuth/elevation from the SSI encoders, assembles rays under FreeRTOS and streams them to the DSP over GEM 1GbE per the frozen DRx↔DSP contract, synthesizes the Tx reference on the DACs, and applies the DSP-supplied AFC correction — all configured through the DSP, with status and BITE reporting — verified on the bench via DAC→ADC loopback and digital vector injection, validated against spec and integrated consistently with the DSP, with timing closed, endurance and fault-injection campaigns passed, an offline boot image provisioning a clean board, documentation delivered, and an on-radar commissioning dry-run plan ready. Connection to the real radar IF and antenna begins after month 8\.

