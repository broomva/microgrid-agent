# microgrid-agent

## Project Context

Open-source edge AI agent for autonomous renewable energy microgrid management. Targets Raspberry Pi 4/5 deployment in Colombia's Zonas No Interconectadas (ZNI) -- disconnected communities with solar+battery+diesel hybrid microgrids.

- **Language**: Python 3.11+, asyncio throughout
- **Runtime**: Raspberry Pi OS Lite (64-bit), ARM64
- **Design principle**: Edge-first, offline-capable. No cloud dependency in the critical control path.
- **Research context**: MAIA capstone at Universidad de los Andes, TICSw research group (A1, Minciencias)

## Architecture

### Module Map

```
src/
+-- agent.py          Main async control loop & orchestrator
+-- devices.py        Hardware abstraction layer (Modbus RTU, VE.Direct, simulated)
+-- forecast.py       TFLite LSTM models for solar & demand forecasting
+-- dispatch.py       LP optimizer via scipy.optimize.linprog
+-- knowledge.py      SQLite knowledge graph for territorial context
+-- sync.py           MQTT fleet sync with store-and-forward queue
+-- autonomic.py      Safety constraints & homeostasis controller
+-- dashboard.py      Local web dashboard (FastAPI, lightweight)
+-- telemetry.py      Structured event logging & metrics

config/
+-- site.example.toml    Site identity, grid topology, autonomic setpoints
+-- devices.toml         Device registry (per-deployment, not committed)

data/
+-- models/              TFLite model files
+-- sync-queue/          Offline MQTT queue (SQLite WAL)

deploy/                  Systemd units, install scripts
scripts/                 Health checks, calibration, utilities
tests/                   pytest test suite
docs/                    Architecture, DIY guide, conversations
```

### Control Loop Hierarchy

The agent runs three nested control loops at different rates:

| Loop | Rate | Responsibility |
|------|------|----------------|
| Safety monitor | 100ms | SOC bounds, fault detection, emergency load shedding |
| Device polling | 1s | Read all sensors/devices, update state, log telemetry |
| Forecast + dispatch | 15min | ML inference, LP optimization, schedule next interval |

The safety monitor always runs fastest and can override any dispatch decision. This is the core invariant -- safety constraints are never relaxed by the optimizer.

### Data Flow

```
Sensors (Modbus/VE.Direct)
    |
    v
DeviceRegistry.read_all()  -- 1s poll
    |
    v
TelemetryLogger.record()   -- append to SQLite journal
    |
    +---> Forecaster.predict()  -- every 15 min
    |         |
    |         v
    |    Dispatcher.optimize()  -- LP solver
    |         |
    |         v
    |    DispatchPlan { solar_kw, battery_kw, diesel_kw, shed_loads }
    |         |
    +---> AutonomicController.validate(plan)  -- safety gate
              |
              v
         DeviceRegistry.apply(validated_plan)
              |
              v
         SyncClient.enqueue(telemetry)  -- store-and-forward to MQTT
```

## Conventions

### Code

- **Type hints** on all function signatures and return types
- **asyncio** for all I/O (device reads, network, file writes)
- **Structured JSON logging** via Python `logging` + JSON formatter. No `print()` statements.
- **TOML** for all configuration files
- **SQLite** for persistence (knowledge graph, telemetry journal, sync queue)
- **No cloud dependencies** in the core control path. Cloud features (fleet sync, remote dashboard) are optional modules that degrade gracefully.

### Naming

- Modules: `snake_case.py`
- Classes: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `UPPER_SNAKE_CASE`
- Config keys: `snake_case` (TOML convention)

### Error Handling

- Device read failures return a zero-power reading with `DeviceStatus.OFFLINE` -- never crash the control loop
- Network failures queue data locally -- never block the agent
- Safety constraint violations trigger immediate load shedding -- never wait for the optimizer

### Dependencies

Core (must run on RPi):
- `numpy` -- numerical operations
- `scipy` -- LP solver (`scipy.optimize.linprog`)
- `tflite-runtime` -- ML inference (not full TensorFlow)
- `pymodbus` -- Modbus RTU communication
- `paho-mqtt` -- MQTT client for fleet sync
- `fastapi` + `uvicorn` -- local dashboard
- `aiosqlite` -- async SQLite access

Optional:
- `serial-asyncio` -- VE.Direct serial protocol
- `RPi.GPIO` -- GPIO access on Raspberry Pi

Dev:
- `pytest` + `pytest-asyncio` -- testing
- `ruff` -- linting and formatting
- `mypy` -- type checking

## Bstack Primitives

Mapping the 7 bstack primitives to this project:

| # | Primitive | Implementation | Status |
|---|-----------|----------------|--------|
| P1 | Conversation Bridge | `docs/conversations/` -- session logs indexed here | Active |
| P2 | Control Gate | `autonomic.py` -- safety constraints as the control gate. SOC limits, diesel runtime caps, load shedding priority are hard gates that the ML optimizer cannot override. | Active |
| P3 | Spaces Integration | N/A -- standalone edge project, no SpacetimeDB dependency | N/A |
| P4 | Asset Delivery | N/A -- no web assets to deliver | N/A |
| P5 | Linear Tickets | GitHub Issues for task tracking | Active |
| P6 | PR Pipeline | GitHub Actions CI -- pytest + ruff on every PR | Active |
| P7 | Parallel Agents | Simulation mode supports multiple simulated sites running concurrently for fleet testing | Active |

## Control Kernel Integration

The `autonomic.py` module IS the control kernel for this project. It implements a homeostasis controller inspired by biological autonomic nervous systems.

### Setpoints

Defined in `site.toml` under `[autonomic]`:

```toml
[autonomic]
min_soc_pct = 15          # Hard floor -- load shedding below this
max_soc_pct = 95          # Hard ceiling -- curtail charging above this
diesel_start_soc = 20     # Diesel auto-start threshold
diesel_stop_soc = 60      # Diesel auto-stop threshold
renewable_target = 0.85   # Target renewable fraction
```

### Safety Gates

| Gate | Trigger | Action | Override |
|------|---------|--------|----------|
| G1: SOC Floor | SOC < `min_soc_pct` | Shed non-priority loads in order | NEVER |
| G2: SOC Ceiling | SOC > `max_soc_pct` | Curtail solar charging | NEVER |
| G3: Diesel Limit | Runtime > 8h/day | Force diesel stop, shed if needed | NEVER |
| G4: Fault Isolate | Device fault detected | Disconnect faulted device | NEVER |

### Feedback Loop

```
Predicted (forecast)  vs  Actual (telemetry)
         |                      |
         +-------> Error -------+
                     |
                     v
            Model Adaptation
            (retrain LSTM weights on-device, weekly)
```

### Invariant

**Safety constraints are NEVER overridden by ML predictions or optimizer outputs.** The autonomic controller has absolute veto power over any dispatch plan. This is the single most important design principle in the system.

## Testing

```bash
# Run all tests
make test

# Run with verbose output
pytest -v

# Run specific test module
pytest tests/test_devices.py

# Run in simulation mode for integration testing
make simulate
```

### Test Strategy

- **Unit tests**: Each module tested in isolation with simulated devices
- **Integration tests**: Full control loop running in simulation mode
- **No hardware required for CI**: All device interactions go through `SimulatedDevice` in test/CI environments
- **Deterministic randomness**: Tests seed the random number generator for reproducible simulated readings

## Commands

```bash
make test          # Run pytest suite
make simulate      # Run agent in simulation mode
make lint          # Ruff linter check
make format        # Ruff auto-format
make deploy-rpi    # Deploy to connected RPi via SSH (requires MICROGRID_HOST env var)
make docker-build  # Build test container
make docker-run    # Run test container
make health        # Run health-check.sh on local or remote RPi
```
