# Agentic Architecture — The Agent IS the Controller

> The microgrid agent is not a deterministic system that occasionally calls an ML model.
> It is an autonomous AI agent — an instance of Life/Arcan — that reasons about its
> environment using an LLM and acts on the physical world through tools.
> The LSTM, the LP solver, the KG — these are tools the agent uses, not the agent itself.

---

## The Conceptual Shift

### What we had (ML-centric framing):
```
Rust control loop → calls LSTM for forecast → calls LP for dispatch → actuates
The LLM is an afterthought, bolted on for "smart features"
```

### What we actually mean (agentic-native framing):
```
Life/Arcan agent → reasons about the microgrid → uses tools to sense, predict,
optimize, and actuate → learns from outcomes → adapts its own behavior
The LLM IS the reasoning core. Everything else is a tool.
```

This is exactly how Life already works:
- **Arcan** = agent runtime (manages lifecycle, tools, memory)
- **Praxis** = tool execution (the agent's hands — Modbus, LP solver, KG queries)
- **Lago** = event journal (the agent's episodic memory)
- **Autonomic** = homeostasis (the agent's brainstem — safety reflexes that override reasoning)
- **The LLM** = the agent's cortex — reasons about what to do given context

The microgrid agent is a Life agent with energy-domain tools. The LLM doesn't "assist" the control loop — it IS the decision maker, constrained by Autonomic safety gates.

---

## Architecture: Agent as Controller

```
╔══════════════════════════════════════════════════════════════════════════╗
║                                                                          ║
║  LIFE/ARCAN AGENT INSTANCE                                              ║
║  (one per microgrid site, running on RPi)                                ║
║                                                                          ║
║  ┌────────────────────────────────────────────────────────────────────┐  ║
║  │                                                                    │  ║
║  │  ┌──────────────────────────────────────────────────────────────┐  │  ║
║  │  │  REASONING CORE (LLM — BitNet 2B or Qwen 2.5 3B)           │  │  ║
║  │  │                                                              │  │  ║
║  │  │  System prompt:                                              │  │  ║
║  │  │    "You are an autonomous energy agent managing a {solar_kwp}│  │  ║
║  │  │     kWp solar + {battery_kwh} kWh battery + {diesel_kw} kW   │  │  ║
║  │  │     diesel microgrid at {site_name}. Your goal is to          │  │  ║
║  │  │     maximize renewable energy use, minimize diesel, and       │  │  ║
║  │  │     ensure priority loads (health center, water pump) never   │  │  ║
║  │  │     lose power. You have these tools available..."            │  │  ║
║  │  │                                                              │  │  ║
║  │  │  Context window (4096 tokens):                               │  │  ║
║  │  │    • Current state: SOC, solar, load, diesel fuel, time      │  │  ║
║  │  │    • Recent history: last 4h of readings (compressed)        │  │  ║
║  │  │    • KG context: community calendar, equipment relations     │  │  ║
║  │  │    • Last 3 decisions + outcomes (EGRI feedback)             │  │  ║
║  │  │    • Active alerts and anomalies                             │  │  ║
║  │  │                                                              │  │  ║
║  │  │  Reasoning cycle (every 5 minutes):                          │  │  ║
║  │  │    1. read_sensors() → current state                         │  │  ║
║  │  │    2. get_forecast() → next 24h (LSTM tool)                  │  │  ║
║  │  │    3. query_kg("what affects operations now?")               │  │  ║
║  │  │    4. THINK about the situation                               │  │  ║
║  │  │    5. Call dispatch tool OR adjust setpoints OR flag anomaly  │  │  ║
║  │  │    6. Log reasoning to journal                               │  │  ║
║  │  └──────────────────────────────────────────────────────────────┘  │  ║
║  │                     │                                              │  ║
║  │                     │ tool calls                                   │  ║
║  │                     ▼                                              │  ║
║  │  ┌──────────────────────────────────────────────────────────────┐  │  ║
║  │  │  PRAXIS — Tool Execution Layer                               │  │  ║
║  │  │                                                              │  │  ║
║  │  │  SENSE tools (read-only):                                    │  │  ║
║  │  │    read_sensors()     → SensorReadings (Modbus/VE.Direct)    │  │  ║
║  │  │    get_forecast()     → 24h generation + demand (LSTM)       │  │  ║
║  │  │    query_kg(question) → graph traversal result               │  │  ║
║  │  │    get_battery_health()→ degradation estimate                │  │  ║
║  │  │    get_fuel_level()   → diesel tank status                   │  │  ║
║  │  │    get_weather()      → temperature, irradiance, rain        │  │  ║
║  │  │                                                              │  │  ║
║  │  │  ACT tools (write, validated by Autonomic):                  │  │  ║
║  │  │    set_dispatch(solar, battery, diesel, shed)                │  │  ║
║  │  │    adjust_setpoint(key, value)  → modify autonomic params    │  │  ║
║  │  │    start_diesel() / stop_diesel()                            │  │  ║
║  │  │    set_load_priority(ordered_list)                           │  │  ║
║  │  │                                                              │  │  ║
║  │  │  COMMUNICATE tools:                                          │  │  ║
║  │  │    alert(severity, message)     → fleet + local dashboard    │  │  ║
║  │  │    log_insight(text)            → reasoning journal          │  │  ║
║  │  │    request_maintenance(what)    → fleet maintenance queue    │  │  ║
║  │  │    answer_operator(question)    → local dashboard response   │  │  ║
║  │  │                                                              │  │  ║
║  │  │  FORBIDDEN (no tool exists):                                 │  │  ║
║  │  │    override_safety()  ← DOES NOT EXIST. Cannot be called.   │  │  ║
║  │  │    modify_autonomic_gates() ← DOES NOT EXIST.               │  │  ║
║  │  │    The agent literally cannot bypass safety.                 │  │  ║
║  │  └──────────────────────────────────────────────────────────────┘  │  ║
║  │                     │                                              │  ║
║  │                     │ every tool call validated                    │  ║
║  │                     ▼                                              │  ║
║  │  ┌──────────────────────────────────────────────────────────────┐  │  ║
║  │  │  AUTONOMIC — Safety Gates (Rust, deterministic, not LLM)     │  │  ║
║  │  │                                                              │  │  ║
║  │  │  G1: SOC ≥ min_soc_pct          → block discharge           │  │  ║
║  │  │  G2: SOC ≤ max_soc_pct          → block charge              │  │  ║
║  │  │  G3: diesel_hours < max/day     → block diesel start        │  │  ║
║  │  │  G4: priority_loads_served      → block non-essential shed  │  │  ║
║  │  │  G5: setpoint_in_safe_range     → reject unsafe adjustments │  │  ║
║  │  │                                                              │  │  ║
║  │  │  Autonomic is NOT an LLM. It is Rust code with hard limits. │  │  ║
║  │  │  The LLM reasons. Autonomic enforces. This is the harness.  │  │  ║
║  │  └──────────────────────────────────────────────────────────────┘  │  ║
║  │                                                                    │  ║
║  │  ┌──────────────────────────────────────────────────────────────┐  │  ║
║  │  │  LAGO — Event Journal (redb, crash-safe)                     │  │  ║
║  │  │                                                              │  │  ║
║  │  │  Every reasoning cycle logged:                               │  │  ║
║  │  │    { timestamp, state, tools_called, reasoning, decision,    │  │  ║
║  │  │      outcome_after_5min, autonomic_overrides }               │  │  ║
║  │  │                                                              │  │  ║
║  │  │  This is the agent's episodic memory. Fed back into the      │  │  ║
║  │  │  LLM context as "last 3 decisions + outcomes" for EGRI.      │  │  ║
║  │  └──────────────────────────────────────────────────────────────┘  │  ║
║  │                                                                    │  ║
║  └────────────────────────────────────────────────────────────────────┘  ║
║                                                                          ║
╚══════════════════════════════════════════════════════════════════════════╝
```

---

## The EGRI Loop — How the Agent Improves Itself

This is the control-theoretic governance framework (Idea G) applied to a real system:

```
                    ┌─────────────────────────────────┐
                    │         EGRI EVALUATOR           │
                    │   (runs daily, uses the LLM)     │
                    │                                   │
                    │   Reads last 24h from Lago:       │
                    │   • What did I predict?            │
                    │   • What actually happened?        │
                    │   • Where was I wrong?             │
                    │   • Did Autonomic override me?     │
                    │   • Did my setpoint changes help?  │
                    │                                   │
                    │   Produces:                        │
                    │   • Forecast bias correction       │
                    │   • Setpoint adjustment proposals  │
                    │   • Self-assessment score          │
                    └──────────┬──────────────┬─────────┘
                               │              │
                     ┌─────────▼───┐    ┌─────▼──────────┐
                     │ ADJUST      │    │ ESCALATE       │
                     │ (if safe)   │    │ (if uncertain) │
                     │             │    │                 │
                     │ Lower       │    │ "I've been      │
                     │ diesel_start│    │  wrong 3 days   │
                     │ from 25→22  │    │  in a row about │
                     │             │    │  afternoon      │
                     │ Validated   │    │  demand. Request │
                     │ by Autonomic│    │  fleet model    │
                     │ (22 > 20 ✓) │    │  update."       │
                     └─────────────┘    └────────────────┘
```

**This is homeostasis.** The agent maintains operational stability not through fixed rules but through continuous self-evaluation and adaptation — the same way biological organisms maintain temperature, blood sugar, pH. The setpoints are the "desired state." The EGRI loop is the feedback mechanism. Autonomic is the brainstem that prevents lethal excursions.

This IS Idea G (brain-inspired control for AI governance) applied to a real system — not as a paper about safety, but as a working agent that governs itself.

---

## Why LLM-as-Controller, Not ML-as-Feature

| Dimension | ML-as-Feature (old framing) | LLM-as-Controller (agentic) |
|-----------|---------------------------|---------------------------|
| **Decision maker** | LP solver (deterministic) | LLM (reasons about context) |
| **LSTM forecast** | The brain | A tool the brain uses |
| **KG query** | Feature extraction | The agent asking itself a question |
| **Anomaly response** | Threshold → alert | Agent reasons about cause + action |
| **Adaptation** | Retrain model daily | Agent adjusts own setpoints based on EGRI |
| **Novel situations** | Fails (not in training data) | Reasons from first principles |
| **Explanation** | None (black box) | Natural language: "I started diesel because..." |
| **Community interface** | Dashboard with numbers | Conversation in Spanish |

### The killer example: a novel failure

```
Scenario: Solar output drops 40% despite clear sky. This has never happened before.

ML-as-Feature response:
  LSTM: "Forecast was 350W/m², actual is 210W/m²" (observes, can't explain)
  LP solver: starts diesel (correct but doesn't know why solar dropped)
  System: logs anomaly, waits for technician

LLM-as-Controller response:
  Agent reasons:
    "Solar is at 60% of expected despite clear sky and 32°C.
     Possible causes I can check:
     1. Panel soiling — use get_weather() to check if dust storm recently
     2. Inverter derating — read_sensors() shows inverter at 85°C (high!)
     3. Partial shading — time is 16:00, tree shadow possible at this angle

     Inverter temperature is 85°C, threshold is 65°C for derating.
     This explains the 40% drop — thermal derating.

     Action: reduce load to lower inverter stress, alert maintenance
     to check ventilation, log finding for pattern detection."

  → set_dispatch(reduce_solar_draw=True)
  → alert("warning", "Inverter thermal derating — check cooling")
  → log_insight("Thermal derating at 85°C explains 40% solar loss")
```

No LSTM or LP solver can do this. It requires causal reasoning across domains (weather + hardware + physics + operations).

---

## BitNet: Making the Reasoning Core Fit on Edge

The core challenge: LLMs are large. RPi 5 has 8GB RAM. How do we fit a capable reasoning core?

### BitNet — 1.58-bit Ternary Models

Microsoft's BitNet uses weights of {-1, 0, +1} instead of floating point. This means:
- **No floating-point multiplication** — just addition/subtraction
- **1.58 bits per weight** vs 4 bits (Q4) or 16 bits (FP16)
- **Optimized kernels** for ARM NEON (RPi's instruction set)

### Performance on Edge Hardware

| Model | Weights | Memory | Decode Latency | Energy/token | Quality (ARC) |
|-------|---------|--------|---------------|-------------|---------------|
| **BitNet 2B** | 1.58-bit | **0.4 GB** | **29 ms** | **0.028 J** | 49.9 |
| Qwen 2.5 1.5B Q4 | 4-bit | 1.2 GB | ~80 ms | ~0.25 J | 46.3 |
| Llama 3.2 3B Q4 | 4-bit | 2.0 GB | ~150 ms | ~0.40 J | ~50 |
| Qwen 2.5 3B Q4 | 4-bit | 2.2 GB | ~170 ms | ~0.45 J | ~55 |

**BitNet 2B uses 3x less memory, is 3-5x faster, and uses 9x less energy per token than comparable quantized models.** On a solar-powered RPi where every watt matters, this is transformative.

### What BitNet 2B Can and Can't Do

| Task | Feasible at 2B? | Notes |
|------|-----------------|-------|
| Tool selection ("which tool do I call?") | YES | Routing/classification works at small scale |
| Structured output (JSON dispatch decisions) | YES | Format following is good |
| Anomaly classification ("is this normal?") | YES | Pattern matching works |
| Simple causal reasoning | PARTIAL | 1-2 hop reasoning OK, complex chains fail |
| Natural language operator Q&A | PARTIAL | Short answers OK, long explanations weak |
| EGRI self-evaluation | PARTIAL | Can compare predicted vs actual, weak at deep analysis |
| Complex novel situation reasoning | NO | Need 3B+ for multi-step causal chains |

### The Tiered Reasoning Architecture

```
┌───────────────────────────────────────────────────────────┐
│  TIER 1: REFLEX (Rust, <1ms, always available)            │
│                                                            │
│  Autonomic safety gates. No reasoning. Pure constraint     │
│  enforcement. SOC < 20% → diesel starts. Period.           │
│                                                            │
│  This is the brainstem.                                    │
├───────────────────────────────────────────────────────────┤
│  TIER 2: FAST REASONING (BitNet 2B, ~30ms, on-device)     │
│                                                            │
│  Every 5 minutes: "given current state and forecast,       │
│  what's the right dispatch?" Tool calling, structured      │
│  output, simple anomaly detection.                         │
│                                                            │
│  0.4 GB RAM. 0.028 J/token. Runs on solar power.           │
│  This is the fast, intuitive brain.                        │
├───────────────────────────────────────────────────────────┤
│  TIER 3: DEEP REASONING (Qwen 3B, ~150ms, on-device)      │
│                                                            │
│  Triggered by anomalies, daily EGRI evaluation, complex    │
│  situations. Multi-step causal reasoning. Can swap in      │
│  when BitNet flags uncertainty: "I don't understand this   │
│  situation, escalating to deeper model."                   │
│                                                            │
│  2.2 GB RAM. Only loaded when needed. Sleeps otherwise.    │
│  This is the deliberate, analytical brain.                 │
├───────────────────────────────────────────────────────────┤
│  TIER 4: STRATEGIC (Claude API, when connected)            │
│                                                            │
│  Fleet-level analysis. Cross-site patterns. Natural        │
│  language reports. Policy recommendations. Deep EGRI       │
│  evaluation across the entire fleet.                       │
│                                                            │
│  Unlimited capability. Only available when online.         │
│  This is the collective intelligence.                      │
└───────────────────────────────────────────────────────────┘
```

Each tier can operate independently. If Tier 4 is offline, Tier 3 handles complex reasoning. If Tier 3 isn't loaded, Tier 2 handles routine dispatch. If even Tier 2 crashes, Tier 1 (Autonomic) keeps the lights on with pure Rust reflex rules.

**This is the control-theoretic hierarchy applied to cognition:**
- Inner loop (fast, deterministic) → Autonomic
- Mid loop (adaptive, learned) → BitNet 2B
- Outer loop (deliberate, analytical) → Qwen 3B
- Meta loop (strategic, reflective) → Claude

Faster loops override slower ones on safety. Slower loops improve faster ones over time. This IS predictive coding. This IS hierarchical Bayesian inference. This IS homeostasis.

---

## Continuous Progress: Why the Idea Gets Better Over Time

The agentic-native architecture benefits from every improvement in the field without architectural changes:

### 2024 → 2026: What Already Happened

| Year | Advance | Impact on This Architecture |
|------|---------|---------------------------|
| 2024 | Qwen 2.5 small models with tool calling | Made agentic reasoning feasible at 1.5-3B |
| 2024 | BitNet 1.58-bit ternary quantization | 9x energy reduction, 3x memory reduction |
| 2025 | llama.cpp ARM NEON optimization | 1.37-5.07x speedup on RPi |
| 2025 | LoRA/QLoRA for edge fine-tuning | Site-specific adaptation without full retraining |
| 2025 | Flower federated learning on RPi | Proven multi-device FL |
| 2026 | Control-theoretic foundations for agentic systems | Formal framework validates our approach |
| 2026 | BitNet 2B (b1.58-2B-4T) | First research-quality 1-bit model |

### 2026 → 2028: What's Coming (conservative projection)

| Expected Advance | Impact |
|-----------------|--------|
| BitNet 7B+ models | Deep reasoning on RPi 8GB at 0.4 GB memory |
| Sub-1-bit quantization research | Even smaller, even faster |
| ARM v9 with matrix extensions (RPi 6?) | 2-4x inference speedup |
| Agentic tool-calling fine-tunes at 1-3B | Purpose-built models for IoT agent use |
| Federated fine-tuning with ternary weights | Fleet-wide learning at 1-bit efficiency |
| RISC-V with BitNet acceleration | Custom silicon for 1-bit inference |

**The architecture doesn't change.** The Arcan agent runtime, the Praxis tools, the Autonomic safety gates, the Lago journal — all stay the same. Only the model in the reasoning core gets swapped for a better one. This is the separation of concerns that Life provides: the agent runtime is independent of the model powering it.

### The Research Trajectory

```
CAPSTONE (2026-2028):
  BitNet 2B on RPi → prove agentic dispatch > rule-based
  MAPE comparison across 3 climate zones
  EGRI loop validation: does the agent improve over 30 days?

POST-CAPSTONE (2028+):
  BitNet 7B+ → deeper reasoning, fewer escalations to Tier 3/4
  Custom fine-tune on ZNI operational data → domain-specific agent
  Fleet of 100+ agents with federated learning
  Formal stability proofs connecting Autonomic gates to Lyapunov theory

LONG-TERM VISION:
  Every microgrid in the developing world runs a Life agent
  The model keeps improving. The architecture is already right.
  1-bit models make it economically viable at any scale.
```

---

## How Idea G Lives Inside Candidate E

The reviewer's concern was that Idea G (brain-inspired control for AI governance) scored 55/100 because "Ciencia para la Paz" is about armed conflict, not AI safety.

But Idea G was never meant to be a separate proposal. It's the **methodology** applied inside Candidate E:

| Idea G Concept | Where It Lives in the Microgrid Agent |
|---------------|--------------------------------------|
| Homeostasis | Autonomic controller with setpoints and feedback |
| Predictive coding | LLM anticipates future state, Lago records prediction error |
| Hierarchical Bayesian inference | Tiered reasoning (BitNet → Qwen → Claude) |
| Lyapunov stability | Autonomic gates ensure bounded state (SOC ∈ [min, max]) |
| Feedback loops | EGRI: predicted vs actual → parameter adjustment |
| Multi-rate control | Tier 1 (ms) → Tier 2 (min) → Tier 3 (hr) → Tier 4 (day) |
| Self-regulation | Agent adjusts own setpoints based on outcomes |

**The microgrid agent IS a brain-inspired control system governing an autonomous AI agent.** It just happens to be applied to energy management in Colombia's ZNI, which aligns with Transición Energética instead of Ciencia para la Paz.

The research contribution isn't "AI for microgrids" (engineering) or "AI safety theory" (pure theory). It's **demonstrating that an autonomous AI agent can safely and effectively manage critical physical infrastructure at the edge, governed by a control-theoretic framework inspired by biological homeostasis, using 1-bit LLMs that run on solar-powered hardware costing less than a boat trip.**

That's new. Nobody has done this.

---

## Implementation: What Changes in the Codebase

The current Rust kernel's `main.rs` dispatches on fixed time intervals. The agentic version dispatches based on the LLM's reasoning:

```rust
// Current: deterministic loop
loop {
    tokio::select! {
        _ = sensor_interval.tick() => { self.devices.read_all().await; }
        _ = dispatch_interval.tick() => { self.dispatcher.solve(&state).await; }
        _ = forecast_interval.tick() => { self.ml.request_forecast().await; }
    }
}

// Agentic: LLM reasoning loop
loop {
    tokio::select! {
        // Tier 1: Autonomic reflexes (always, deterministic)
        _ = sensor_interval.tick() => {
            let readings = self.devices.read_all().await;
            self.autonomic.check_reflexes(&readings).await;  // immediate safety
        }
        // Tier 2: Agent reasoning cycle (every 5 min)
        _ = reasoning_interval.tick() => {
            let context = self.build_context(&state).await;  // state + KG + history
            let response = self.llm.reason(context, &self.tools).await;
            // LLM returns tool calls → Praxis executes → Autonomic validates
            for tool_call in response.tool_calls {
                let validated = self.autonomic.validate(&tool_call);
                if validated.allowed {
                    self.praxis.execute(tool_call).await;
                } else {
                    self.journal.log_override(tool_call, validated.reason);
                }
            }
            self.journal.log_reasoning(context, response, outcomes);
        }
        // Tier 1: Watchdog (always)
        _ = watchdog_interval.tick() => {
            sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
        }
    }
}
```

The deterministic sensor reading stays at 1Hz (Tier 1). The LLM reasoning happens at 5-minute intervals (Tier 2). Safety enforcement happens on EVERY tool call (Tier 1). The agent reasons; the harness constrains.

---

## Bill of Materials — Updated for Agentic Architecture

| Component | Purpose | Cost |
|-----------|---------|------|
| RPi 5 8GB | Agent runtime + BitNet 2B (0.4GB) + kernel (0.1GB) + KG (0.1GB) = 7.4GB free | $80 |
| NVMe 256GB | Lago journal + models + sync queue | $30 |
| UPS HAT | Survive power outages | $50 |
| RS-485 HAT | Modbus to inverters/genset | $15 |
| Sensors | CTs, irradiance, temperature, SOC | $175 |
| Connectivity | 4G modem + satellite fallback | $150 |
| **Total** | **Full agentic node** | **~$650** |

BitNet 2B adds **0.4 GB RAM and 0.028 J per token** to the existing node. No hardware change needed. The agent literally runs on the same hardware — it just thinks now.
