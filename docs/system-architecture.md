# System Architecture — Full Map

> From a single photon hitting a solar panel to an IPSE operator asking
> "which sites need attention today?" in natural language.

> **See also**: [agentic-architecture.md](agentic-architecture.md) for the agentic-native
> design rationale, tiered reasoning hierarchy, BitNet edge reasoning, and EGRI
> self-improvement loop. This document covers the three-plane system map, fleet topology,
> and technology stack.

---

## 1. The Three Planes

The system operates across three planes, each with different time horizons,
failure modes, and language choices:

```
╔═══════════════════════════════════════════════════════════════════════════╗
║                                                                          ║
║  PLANE 3: INTELLIGENCE (cloud/IPSE server)         TypeScript + Python   ║
║  ┌────────────────────────────────────────────────────────────────────┐   ║
║  │  Fleet Dashboard (Next.js)  │  LLM Briefings (Claude API)        │   ║
║  │  Transfer Learning Engine   │  Diesel Logistics Optimizer         │   ║
║  │  Anomaly Detection (fleet)  │  Predictive Maintenance             │   ║
║  │  Time Series DB (DuckDB)    │  MQTT Broker (NanoMQ)               │   ║
║  └──────────────────────────────┬─────────────────────────────────────┘   ║
║                                 │                                         ║
║  ─ ─ ─ ─ ─ ─ ─ ─ MQTT/TLS ─ ─ ┼ ─ ─ ─ INTERMITTENT ─ ─ ─ ─ ─ ─ ─ ─   ║
║                                 │                                         ║
║  PLANE 2: ADAPTATION (on-device)                    Python subprocess     ║
║  ┌────────────────────────────────────────────────────────────────────┐   ║
║  │  ML Forecasting (TFLite)    │  Model Retraining (daily)           │   ║
║  │  Data Processing Pipeline   │  Knowledge Graph Learning           │   ║
║  │  Fleet Sync Daemon          │  Anomaly Flagging (local)           │   ║
║  └──────────────────────────────┬─────────────────────────────────────┘   ║
║                                 │ IPC (Unix socket / subprocess)          ║
║  PLANE 1: CONTROL (on-device, always running)       Rust kernel daemon    ║
║  ┌────────────────────────────────────────────────────────────────────┐   ║
║  │  Sensor Reading (Modbus/UART)│  LP Dispatch Optimization          │   ║
║  │  Autonomic Safety Gates      │  Event Journal (redb)              │   ║
║  │  Actuator Commands           │  Local Dashboard (axum + HTMX)     │   ║
║  │  Watchdog (systemd/HW)       │  KG Query (rusqlite)               │   ║
║  └────────────────────────────────────────────────────────────────────┘   ║
║                                 │                                         ║
║  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ RS-485 / UART / GPIO ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─   ║
║                                 │                                         ║
║  PLANE 0: PHYSICS                                   Wires and photons     ║
║  ┌────────────────────────────────────────────────────────────────────┐   ║
║  │  Solar Panels  │  Battery Bank  │  Diesel Generator  │  Loads     │   ║
║  │  Inverters     │  Charge Ctrl   │  Genset Controller  │  Meters   │   ║
║  └────────────────────────────────────────────────────────────────────┘   ║
║                                                                          ║
╚═══════════════════════════════════════════════════════════════════════════╝
```

### Failure Isolation Between Planes

```
If Plane 3 dies (cloud/fleet):
  → Plane 2 continues (local ML, local learning)
  → Plane 1 continues (dispatch, safety)
  → Lights stay on ✓

If Plane 2 dies (Python ML worker):
  → Plane 1 continues with last-known-good forecast
  → Falls back to persistence prediction (yesterday=today)
  → Falls back to rule-based dispatch
  → Lights stay on ✓

If Plane 1 dies (Rust kernel):
  → systemd restarts in 5s (WatchdogSec=30s)
  → If 4 crashes in 3 min → full RPi reboot
  → During restart: last dispatch commands hold (inverter keeps state)
  → Lights stay on ✓ (briefly degraded)

If Plane 0 dies (hardware failure):
  → Kernel detects via sensor timeout
  → Flags anomaly, notifies fleet
  → Requires physical maintenance
  → This is the only failure that affects power
```

---

## 2. Single Node — Internal Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                    RASPBERRY PI 5 (8GB ARM64)                        │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │              RUST KERNEL (single static binary)               │    │
│  │                                                                │    │
│  │   MAIN LOOP (tokio async runtime)                              │    │
│  │   ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐             │    │
│  │   │SENSORS │  │DISPATCH│  │ACTUATOR│  │WATCHDOG│             │    │
│  │   │ 1 Hz   │  │ 0.2 Hz │  │ 0.2 Hz │  │ 0.05Hz│             │    │
│  │   │        │  │        │  │        │  │        │             │    │
│  │   │Modbus  │  │LP solve│  │Modbus  │  │sd_note│             │    │
│  │   │VE.Dir  │  │or rules│  │write   │  │ping   │             │    │
│  │   │GPIO    │  │        │  │GPIO    │  │        │             │    │
│  │   └───┬────┘  └───┬────┘  └───┬────┘  └────────┘             │    │
│  │       │           │           │                                │    │
│  │   ┌───▼───────────▼───────────▼────────────────────────────┐  │    │
│  │   │                AUTONOMIC CONTROLLER                     │  │    │
│  │   │                                                         │  │    │
│  │   │  G1: SOC ≥ min_soc_pct        → force diesel start     │  │    │
│  │   │  G2: SOC ≤ max_soc_pct        → stop charging          │  │    │
│  │   │  G3: diesel_hours < max/day   → refuse diesel start     │  │    │
│  │   │  G4: priority_loads_served    → shed non-essential first│  │    │
│  │   │                                                         │  │    │
│  │   │  ALL GATES ARE HARD. ML CANNOT OVERRIDE.                │  │    │
│  │   └────────────────────────────┬────────────────────────────┘  │    │
│  │                                │                                │    │
│  │   ┌────────────────────────────▼────────────────────────────┐  │    │
│  │   │               EVENT JOURNAL (redb)                       │  │    │
│  │   │  Every sensor read, every dispatch, every override       │  │    │
│  │   │  Append-only, crash-safe, queryable                      │  │    │
│  │   │  Compressed daily, synced to fleet when connected        │  │    │
│  │   └─────────────────────────────────────────────────────────┘  │    │
│  │                                                                │    │
│  │   ┌───────────────────────┐  ┌──────────────────────────────┐  │    │
│  │   │  KNOWLEDGE GRAPH      │  │  LOCAL DASHBOARD             │  │    │
│  │   │  (rusqlite/SQLite)    │  │  (axum + HTMX)               │  │    │
│  │   │  entities, relations  │  │  :8080 on WiFi AP             │  │    │
│  │   │  recursive CTEs       │  │  /status /forecast /dispatch  │  │    │
│  │   └───────────────────────┘  └──────────────────────────────┘  │    │
│  │                                                                │    │
│  │   ┌─────────────────────────────────────────────────────────┐  │    │
│  │   │  ML BRIDGE (IPC to Python subprocess)                    │  │    │
│  │   │                                                          │  │    │
│  │   │  request_forecast(features) → ForecastResult             │  │    │
│  │   │  request_retrain(data_path) → ModelMetadata               │  │    │
│  │   │  request_ingest(file_path) → IngestResult                │  │    │
│  │   │                                                          │  │    │
│  │   │  Protocol: JSON over Unix socket or stdin/stdout pipe    │  │    │
│  │   │  Timeout: 30s → fallback to persistence forecast         │  │    │
│  │   └─────────────────────────────────────────────────────────┘  │    │
│  │                                                                │    │
│  │   ┌─────────────────────────────────────────────────────────┐  │    │
│  │   │  FLEET SYNC (rumqttc MQTT client)                        │  │    │
│  │   │                                                          │  │    │
│  │   │  Publishes:  site/{id}/metrics    (every 60s)            │  │    │
│  │   │              site/{id}/events     (on dispatch/override)  │  │    │
│  │   │              site/{id}/anomalies  (on detection)          │  │    │
│  │   │                                                          │  │    │
│  │   │  Subscribes: fleet/models/{zone}  (weight updates)       │  │    │
│  │   │              fleet/config/{id}    (config changes)        │  │    │
│  │   │              fleet/alerts         (fleet-wide notices)    │  │    │
│  │   │                                                          │  │    │
│  │   │  Offline: → queue to data/sync-queue/*.jsonl             │  │    │
│  │   │  Reconnect: → drain queue, deduplicate                    │  │    │
│  │   └─────────────────────────────────────────────────────────┘  │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │              PYTHON ML WORKER (separate process)               │    │
│  │                                                                │    │
│  │  forecast.py  → TFLite LSTM inference (24h gen + demand)       │    │
│  │  retrain.py   → fine-tune last 2 layers with recent data      │    │
│  │  ingest.py    → process CSV/JSON/XLSX → SQLite KG              │    │
│  │                                                                │    │
│  │  NOT always running — spawned by kernel on demand              │    │
│  │  If crash/timeout → kernel continues with fallback             │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
├──────────────────────────────────────────────────────────────────────┤
│  HARDWARE INTERFACES                                                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────┐ ┌──────────────┐  │
│  │RS-485 HAT│ │USB-UART  │ │GPIO/I2C  │ │WiFi  │ │4G/Satellite  │  │
│  │Modbus RTU│ │VE.Direct │ │Sensors   │ │AP    │ │Fleet Sync    │  │
│  │Inverters │ │Victron   │ │Temp/Irr  │ │Dash  │ │MQTT          │  │
│  │Genset    │ │SmartShunt│ │ADC       │ │:8080 │ │Starlink/LTE  │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Fleet Topology — N Nodes

```
                                    FLEET SERVER
                              (cloud / IPSE datacenter)
                    ┌─────────────────────────────────────┐
                    │                                      │
                    │  ┌──────────┐  ┌──────────────────┐  │
                    │  │ NanoMQ   │  │ DuckDB           │  │
                    │  │ MQTT     │  │ Time Series      │  │
                    │  │ Broker   │  │ + Fleet KG       │  │
                    │  └────┬─────┘  └────┬─────────────┘  │
                    │       │             │                  │
                    │  ┌────▼─────────────▼──────────────┐  │
                    │  │  FLEET ENGINE (Rust or Python)   │  │
                    │  │                                  │  │
                    │  │  Aggregator ─── collects metrics  │  │
                    │  │  Clusterer ─── groups by climate  │  │
                    │  │  Trainer ───── fleet-wide models  │  │
                    │  │  Detector ─── cross-site anomaly  │  │
                    │  │  Optimizer ── diesel logistics    │  │
                    │  │  Predictor ── maintenance needs   │  │
                    │  └────┬─────────────────────────────┘  │
                    │       │                                  │
                    │  ┌────▼─────────────────────────────┐  │
                    │  │  INTELLIGENCE (TypeScript + LLM)  │  │
                    │  │                                    │  │
                    │  │  Next.js Dashboard ── map + alerts │  │
                    │  │  Claude API ──────── NL briefings  │  │
                    │  │  Query Engine ────── "show me..."  │  │
                    │  │  Report Gen ──────── daily digest  │  │
                    │  └──────────────────────────────────┘  │
                    └───────────┬──────────┬─────────────────┘
                                │          │
           ┌────────────────────┘          └──────────────────┐
           │                                                   │
    MQTT/TLS (intermittent)                          MQTT/TLS (intermittent)
           │                                                   │
    ┌──────▼──────────────────────┐           ┌────────────────▼──────────┐
    │ CLIMATE ZONE: ORINOQUÍA     │           │ CLIMATE ZONE: PACÍFICO    │
    │                              │           │                            │
    │  ┌────────┐  ┌────────┐     │           │  ┌────────┐  ┌────────┐   │
    │  │Inirida │  │Mitú    │     │           │  │Coquí   │  │Quibdó  │   │
    │  │2.47 MW │  │200 kW  │     │           │  │101 kVA │  │500 kW  │   │
    │  │online ✓│  │online ✓│     │           │  │online ✓│  │offline │   │
    │  └────────┘  └────────┘     │           │  └────────┘  └────────┘   │
    │  ┌────────┐  ┌────────┐     │           │  ┌────────┐  ┌────────┐   │
    │  │Carurú  │  │P.Carreño│    │           │  │Tadó    │  │Istmina │   │
    │  │50 kW   │  │300 kW  │     │           │  │80 kW   │  │150 kW  │   │
    │  │offline │  │online ✓│     │           │  │online ✓│  │online ✓│   │
    │  └────────┘  └────────┘     │           │  └────────┘  └────────┘   │
    │                              │           │                            │
    │  Shared base model: v4.2     │           │  Shared base model: v3.7   │
    │  Irradiance: 5.0-5.5 kWh/m² │           │  Irradiance: 3.0-3.5 kWh/m²│
    └──────────────────────────────┘           └────────────────────────────┘

              ┌──────────────────────────────┐
              │ CLIMATE ZONE: INSULAR         │
              │                                │
              │  ┌────────────┐  ┌──────────┐ │
              │  │Providencia │  │San Andrés│  │
              │  │wind+solar  │  │diesel    │  │
              │  │online ✓    │  │online ✓  │  │
              │  └────────────┘  └──────────┘  │
              │                                │
              │  Shared base model: v2.1       │
              │  Wind: 7.0 m/s avg             │
              └──────────────────────────────┘
```

---

## 4. Data Flow — From Photon to Decision

```
TIME ──────────────────────────────────────────────────────────────────►

t=0s        t=0.1s      t=0.5s       t=1s         t=5s
PHOTON      SENSOR      KERNEL       JOURNAL      DISPATCH
  │           │           │            │             │
  ▼           ▼           ▼            ▼             ▼
┌─────┐   ┌──────┐   ┌───────┐   ┌────────┐   ┌──────────┐
│Solar│──►│Modbus│──►│Parse  │──►│Append  │──►│LP solve  │
│panel│   │read  │   │decode │   │to redb │   │min diesel│
│     │   │reg   │   │→State │   │journal │   │+ unserved│
└─────┘   │40071 │   │Vector │   │        │   │          │
          └──────┘   └───┬───┘   └────────┘   └────┬─────┘
                         │                          │
                         ▼                          ▼
                    ┌──────────┐              ┌──────────┐
                    │Ring      │              │Autonomic │
                    │buffer    │              │check:    │
                    │(last 24h)│              │SOC ok?   │
                    │for ML    │              │diesel ok?│
                    └──────────┘              │loads ok? │
                                              └────┬─────┘
                                                   │
                                                   ▼
t=5s         t=5.1s       t=60s          t=900s         t=86400s
ACTUATE      DASHBOARD    PUBLISH        FORECAST        RETRAIN
  │            │            │               │               │
  ▼            ▼            ▼               ▼               ▼
┌──────┐   ┌───────┐   ┌────────┐   ┌───────────┐   ┌──────────┐
│Modbus│   │Update │   │MQTT    │   │Spawn      │   │Spawn     │
│write │   │status │   │publish │   │Python:    │   │Python:   │
│setpts│   │page   │   │metrics │   │TFLite     │   │fine-tune │
│      │   │       │   │to fleet│   │LSTM       │   │last 2    │
│Start │   │SOC: 45│   │or queue│   │inference  │   │layers    │
│diesel│   │Gen: 12│   │if off- │   │24h ahead  │   │with      │
│at 3kW│   │Load: 8│   │line    │   │           │   │7d data   │
└──────┘   └───────┘   └────────┘   └───────────┘   └──────────┘
```

---

## 5. Control Loop Timing Diagram

```
         1s        5s        1min       15min      1hr       1day
         │         │          │           │          │          │
SAFETY   ■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■
         │ Always on. HW watchdog. Never stops.                │
         │                                                      │
SENSE    ■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─■─
         │ 1 Hz sensor polling. Ring buffer fills.              │
         │                                                      │
DISPATCH ──■────■────■────■────■────■────■────■────■────■────■──
         │   5s cycle. LP or rules. Uses latest forecast.       │
         │                                                      │
PUBLISH  ──────────■──────────■──────────■──────────■──────────■
         │         60s metric publish to fleet (or queue).      │
         │                                                      │
FORECAST ──────────────────────────■────────────────────────────■
         │                        15min. Spawn Python. TFLite.  │
         │                        Update forecast vectors.      │
         │                                                      │
LEARN    ─────────────────────────────────────────────────────■─
         │                                                  24h│
         │                        Fine-tune model. Update KG.  │
         │                        Sync fleet if connected.     │
         │                                                      │
FLEET    ─ ─ ─ ─ ─■─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─■─ ─ ─ ─ ─ ─ ─ ─
         │   Opportunistic. When connectivity exists.           │
         │   Download model updates. Upload compressed events.  │
```

---

## 6. Fleet Compounding — How Intelligence Grows

```
MONTH 1                    MONTH 3                    MONTH 12
(10 nodes)                 (50 nodes)                 (200+ nodes)

Each site learns           Sites grouped by           Fleet intelligence
independently.             climate zone.              is self-reinforcing.

┌─────┐                    ┌─────┐                    ┌─────┐
│  A  │ cold start         │  A  ├──┐                 │  A  ├──┐
└─────┘                    └─────┘  │                 └─────┘  │
┌─────┐                    ┌─────┐  │  Orinoquía      ┌─────┐  │  Orinoquía
│  B  │ cold start         │  B  ├──┼──base model     │  B  ├──┼──model v12
└─────┘                    └─────┘  │  v2             └─────┘  │  (mature)
┌─────┐                    ┌─────┐  │                 ┌─────┐  │
│  C  │ cold start         │  C  ├──┘                 │  C  ├──┘
└─────┘                    └─────┘                    └─────┘
                           ┌─────┐                    ┌─────┐
No transfer                │  D  ├──┐                 │  D  ├──┐
learning.                  └─────┘  │  Pacífico       └─────┘  │  Pacífico
                           ┌─────┐  │  base model     ┌─────┐  │  model v8
No fleet                   │  E  ├──┘  v1             │  E  ├──┘  (adapted
anomaly                    └─────┘                    └─────┘     to clouds)
detection.
                           Transfer: A helps B.       NEW site X
                           Anomaly: D+E correlated.   gets model v12
                                                      on DAY ONE.
Value per node:            Value per node:             No cold start.
  LOW                        MEDIUM
                                                      Value per node:
                                                        HIGH

                           ┌───────────────────────────────────┐
                           │  COMPOUNDING MECHANISMS            │
                           │                                    │
                           │  1. Transfer learning:             │
                           │     new site inherits from cluster │
                           │                                    │
                           │  2. Anomaly detection:             │
                           │     fleet sees what 1 site can't   │
                           │                                    │
                           │  3. Diesel logistics:              │
                           │     1 boat serves 3 river sites    │
                           │                                    │
                           │  4. Predictive maintenance:        │
                           │     500 battery curves → patterns  │
                           │                                    │
                           │  5. Failure prevention:            │
                           │     0 kWh detected in days, not    │
                           │     8 years (Puerto Nariño)        │
                           └───────────────────────────────────┘
```

---

## 7. Technology Stack Map

```
┌──────────────────────────────────────────────────────────────────┐
│                        TECHNOLOGY CHOICES                         │
├──────────┬──────────────┬───────────────────────────────────────┤
│ Layer    │ Language     │ Key Libraries / Tools                  │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ PHYSICS  │ Wires        │ Modbus RTU (RS-485), VE.Direct (UART) │
│          │              │ SunSpec register maps, DSE/ComAp       │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ KERNEL   │ Rust         │ tokio (async), tokio-modbus, rumqttc   │
│ (daemon) │              │ redb (journal), rusqlite (KG)          │
│          │              │ axum (dashboard), good_lp (dispatch)   │
│          │              │ sd-notify (watchdog), tracing (logs)   │
│          │              │ serde + toml (config)                  │
│          │              │                                        │
│          │              │ Binary: ~15MB static, no deps          │
│          │              │ RAM: ~50MB runtime                     │
│          │              │ Startup: <100ms                        │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ ML       │ Python       │ tflite-runtime (inference)             │
│ (worker) │              │ numpy, scipy (data processing)        │
│          │              │ openpyxl (data ingestion)              │
│          │              │                                        │
│          │              │ Called via subprocess, not always-on   │
│          │              │ Model: <1MB TFLite, <20MB RAM          │
│          │              │ Inference: <0.5ms per forecast         │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ REASONING│ BitNet /     │ BitNet 2B (1.58-bit ternary, 0.4 GB)  │
│ CORE     │ llama.cpp    │ ARM NEON optimized kernels             │
│          │              │ Qwen 2.5 3B Q4 (fallback, 2.2 GB)     │
│          │              │                                        │
│          │              │ LLM-as-controller: reasons about state │
│          │              │ Uses LSTM/LP/KG as tools, not hardcoded│
│          │              │ See docs/agentic-architecture.md        │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ FLEET    │ Rust/Python  │ NanoMQ (MQTT broker)                   │
│ ENGINE   │              │ DuckDB (time series analytics)         │
│          │              │ Flower (federated learning)            │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ FLEET    │ TypeScript   │ Next.js (dashboard UI)                 │
│ DASHBOARD│              │ Claude API (NL briefings)              │
│          │              │ Mapbox/Leaflet (site map)              │
│          │              │ shadcn/ui (component library)          │
│          │              │                                        │
├──────────┼──────────────┼───────────────────────────────────────┤
│          │              │                                        │
│ DEPLOY   │ Shell/Docker │ systemd (process management)           │
│          │              │ OverlayFS (read-only root)             │
│          │              │ RAUC/Mender (A/B updates)              │
│          │              │ Docker (CI/simulation)                 │
│          │              │                                        │
└──────────┴──────────────┴───────────────────────────────────────┘
```

---

## 8. Standards Alignment

```
IEEE 2030.7 Microgrid Controller Layers
────────────────────────────────────────

┌──────────────────────────────────┐
│ Layer 4: Grid Interactive        │  ← NOT APPLICABLE (ZNI = islanded)
│ (utility coordination)          │
├──────────────────────────────────┤
│ Layer 3: Supervisory             │  ← FLEET ENGINE
│ (fleet optimization, scheduling) │     Transfer learning, diesel logistics,
│                                  │     anomaly detection, LLM briefings
├──────────────────────────────────┤
│ Layer 2: Local Area              │  ← RUST KERNEL
│ (microgrid-level control)        │     LP dispatch, autonomic safety,
│                                  │     ML forecasting, KG queries
├──────────────────────────────────┤
│ Layer 1: Device                  │  ← HARDWARE ABSTRACTION
│ (individual asset control)       │     Modbus/VE.Direct adapters,
│                                  │     inverter setpoints, genset start/stop
└──────────────────────────────────┘
```

---

## 9. Security Boundaries

```
┌──────────────────────────────────────────────────────────────┐
│ TRUST ZONE 1: Physical (highest trust)                        │
│ Hardware interfaces, sensor readings, actuator commands        │
│ Attack surface: physical access to RS-485 bus                 │
│ Mitigation: IP65 enclosure, tamper detection                  │
├──────────────────────────────────────────────────────────────┤
│ TRUST ZONE 2: Local (high trust)                              │
│ Kernel daemon, event journal, knowledge graph                 │
│ Attack surface: local dashboard (WiFi AP), SSH                │
│ Mitigation: WPA2 on AP, SSH key-only, no root login           │
├──────────────────────────────────────────────────────────────┤
│ TRUST ZONE 3: Network (medium trust)                          │
│ MQTT fleet sync, model downloads                              │
│ Attack surface: cellular/satellite uplink                     │
│ Mitigation: MQTT over TLS, client certificates                │
│ Model integrity: SHA-256 checksum verification                │
├──────────────────────────────────────────────────────────────┤
│ TRUST ZONE 4: Fleet (lowest trust from node perspective)      │
│ Cloud infrastructure, dashboard, LLM API                      │
│ Attack surface: internet-facing services                      │
│ Mitigation: standard cloud security                           │
│ Key principle: node NEVER trusts fleet for safety decisions    │
│ Fleet can suggest, kernel decides, autonomic overrides all     │
└──────────────────────────────────────────────────────────────┘
```
