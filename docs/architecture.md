# Architecture Reference

Technical architecture for the microgrid-agent system.

> **Note**: This document covers protocol details, hardware abstraction, forecasting,
> dispatch, and fleet sync in depth. For the broader system design, see also:
> - [agentic-architecture.md](agentic-architecture.md) -- agentic-native framing, tiered
>   reasoning hierarchy, BitNet edge reasoning, EGRI self-improvement (authoritative design doc)
> - [system-architecture.md](system-architecture.md) -- three-plane architecture diagram,
>   fleet topology, data flow, technology stack map

---

## Design Principles

1. **Edge-first**: Every feature must work on a Raspberry Pi with no internet connection. Cloud is optional.
2. **Safety-absolute**: The autonomic safety layer has unconditional veto over all ML/optimizer decisions.
3. **Fail-open-safe**: Any component failure degrades to a safe state (diesel backup or load shedding), never to uncontrolled operation.
4. **Store-and-forward**: All telemetry is persisted locally first, synced to fleet when connectivity allows. No data loss.
5. **Minimal footprint**: Total memory <512MB, disk <1GB, CPU <5% during normal operation on RPi 5.

---

## Multi-Rate Control Loop

The agent operates three nested control loops at different frequencies, inspired by biological autonomic nervous systems:

```
+------------------------------------------------------------------+
|                                                                  |
|   +------------------+                                           |
|   | 100ms Loop       |  SAFETY MONITOR                          |
|   |                  |  - Check SOC bounds                       |
|   |  autonomic.py    |  - Detect device faults                   |
|   |                  |  - Emergency load shedding                |
|   |                  |  - Diesel runtime limits                  |
|   +--------+---------+                                           |
|            | safety_ok?                                           |
|            v                                                     |
|   +------------------+                                           |
|   | 1s Loop          |  DEVICE POLLING                           |
|   |                  |  - Read all Modbus/VE.Direct devices      |
|   |  agent.py        |  - Update in-memory state                 |
|   |  devices.py      |  - Append to telemetry journal            |
|   |  telemetry.py    |  - Push to local dashboard                |
|   +--------+---------+                                           |
|            | state_snapshot                                       |
|            v                                                     |
|   +------------------+                                           |
|   | 15min Loop       |  FORECAST + DISPATCH                      |
|   |                  |  - Run LSTM solar/demand forecast          |
|   |  forecast.py     |  - Solve LP dispatch optimization         |
|   |  dispatch.py     |  - Generate DispatchPlan                  |
|   |  knowledge.py    |  - Validate plan against safety gates     |
|   |                  |  - Apply validated commands to devices     |
|   +------------------+  - Enqueue telemetry for fleet sync       |
|                                                                  |
+------------------------------------------------------------------+
```

### Loop Timing Guarantees

| Loop | Period | Max Jitter | Deadline Miss Action |
|------|--------|-----------|---------------------|
| Safety | 100ms | 50ms | Log warning, continue next tick |
| Polling | 1s | 500ms | Skip failed devices, mark OFFLINE |
| Dispatch | 15min | 30s | Use previous dispatch plan |

The safety loop runs as a high-priority asyncio task. If a device read blocks (e.g., Modbus timeout), the safety loop continues independently because it reads from in-memory state, not directly from hardware.

---

## Hardware Abstraction Layer

All device interaction goes through the `EnergyDevice` abstract base class defined in `devices.py`. This provides a uniform interface regardless of communication protocol.

```
                        EnergyDevice (ABC)
                        +------------------+
                        | read_power_kw()  |
                        | read_energy_kwh()|
                        | read_status()    |
                        | set_power_limit()|
                        | start() / stop() |
                        +--------+---------+
                                 |
                +----------------+----------------+
                |                |                |
        ModbusRtuDevice   VeDirectDevice   SimulatedDevice
        (RS-485)          (Serial TTL)     (Software)
```

### Protocol Details

**Modbus RTU:**
- Half-duplex RS-485 bus, 9600 baud default
- 32-bit values read as two consecutive 16-bit registers (big-endian)
- Configurable register addresses per device (power, energy, status, control)
- Async client via `pymodbus.client.AsyncModbusSerialClient`
- 3-second timeout per read, retry once on failure

**VE.Direct:**
- Victron proprietary text protocol over 19200 baud serial
- Continuous streaming of key-value frames (`KEY\tVALUE\r\n`)
- Parsed in a background asyncio task, latest values stored in-memory
- Read-only protocol: no control commands available via VE.Direct text mode
- Key registers: `PPV` (panel power), `V` (voltage), `I` (current), `CS` (state), `H19` (yield)

**Simulated:**
- Software-only device for testing and development
- Solar: sinusoidal curve peaking at noon, proportional to `base_power_kw`
- Load: diurnal pattern (40% baseline, 100% during 07:00-22:00)
- Battery: tracks energy integral over time
- Gaussian noise added at configurable `noise_pct` level

### Device Registry

The `DeviceRegistry` class loads `config/devices.toml` and instantiates the correct device class for each entry. It provides:
- `read_all()`: Concurrent async reads of all devices, returns `dict[str, DeviceReading]`
- `by_type(type)`: Filter devices by type (solar, battery, diesel, load)
- `get(id)`: Retrieve a specific device by ID

---

## Forecasting Engine

### Model Architecture

Two TFLite LSTM models run on-device:

**Solar Irradiance Forecast:**
- Input: 72 hours of historical irradiance + time features (hour, day-of-year, cloud-proxy)
- Output: 24-hour irradiance prediction at 15-minute resolution (96 values)
- Size: ~200KB quantized INT8
- Inference time: <0.5ms on RPi 5

**Demand Forecast:**
- Input: 168 hours (1 week) of historical load + calendar features (day-of-week, market-day, festival)
- Output: 24-hour demand prediction at 15-minute resolution (96 values)
- Size: ~150KB quantized INT8
- Inference time: <0.3ms on RPi 5

### Feature Engineering

The knowledge graph (see below) enriches forecasting features with territorial context:

| Feature | Source | Impact |
|---------|--------|--------|
| Market days | `community.market_days` | +15-30% demand spike |
| Festival months | `community.festivals` | +20-40% demand spike |
| Rainy season | `community.rainy_season_months` | -20-40% solar output |
| Economic activity | `community.primary_activity` | Shapes daily load profile |
| Population | `community.population` | Scales absolute demand |

### On-Device Retraining

Every 7 days, the agent retrains the LSTM models on accumulated local data:

1. Export last 30 days of telemetry from SQLite journal
2. Run incremental training (5 epochs, frozen early layers)
3. Evaluate on held-out last 3 days
4. Deploy new model only if MAPE improves by >2%
5. Keep previous model as fallback

Training runs during low-activity hours (02:00-05:00) to avoid CPU contention with the control loop.

---

## Dispatch Optimizer

### Problem Formulation

At each 15-minute interval, the dispatcher solves a linear program:

```
Minimize:
    w_diesel * P_diesel + w_battery_wear * |P_battery| + w_shed * sum(P_shed)

Subject to:
    P_solar + P_battery_discharge + P_diesel = P_demand - P_shed    (power balance)
    0 <= P_solar <= forecast_solar                                   (solar availability)
    -C_charge <= P_battery <= C_discharge                            (battery C-rates)
    0 <= P_diesel <= diesel_capacity                                 (diesel limits)
    SOC_min <= SOC + integral(P_battery) <= SOC_max                  (SOC bounds)
    P_shed >= 0                                                      (non-negative shedding)
    shed respects priority order                                     (priority constraint)
```

### Priority Stack

The dispatcher follows a strict priority order:

1. **Solar** (zero marginal cost, always preferred)
2. **Battery discharge** (finite cycles, small wear cost)
3. **Diesel** (highest cost: fuel + maintenance + emissions)
4. **Load shedding** (last resort, follows `priority_loads` order from `site.toml`)

### Solver

Uses `scipy.optimize.linprog` with the HiGHS solver backend. Typical solve time: <10ms for a single-interval problem, <100ms for a 24-hour rolling horizon (96 intervals).

---

## Knowledge Graph

### Purpose

The knowledge graph stores territorial context that improves forecasting accuracy. Unlike generic weather APIs, it encodes local patterns specific to each community: when the market operates, which months bring festivals, how fishing seasons affect electricity demand.

### Schema

Stored in SQLite (`data/knowledge.db`), total size <100MB even after years of operation.

```sql
-- Community calendar events that affect demand
CREATE TABLE calendar_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT NOT NULL,  -- 'market', 'festival', 'holiday', 'custom'
    name TEXT,
    recurrence TEXT,           -- cron-like pattern or 'once'
    demand_factor REAL,        -- multiplier on baseline demand (e.g., 1.3 = +30%)
    start_hour INTEGER,
    end_hour INTEGER
);

-- Weather patterns learned from local observations
CREATE TABLE weather_patterns (
    id INTEGER PRIMARY KEY,
    month INTEGER NOT NULL,
    avg_irradiance_kwh_m2 REAL,
    avg_cloud_cover_pct REAL,
    avg_temperature_c REAL,
    rain_probability REAL
);

-- Device performance degradation tracking
CREATE TABLE device_performance (
    device_id TEXT NOT NULL,
    date TEXT NOT NULL,
    expected_output_kwh REAL,
    actual_output_kwh REAL,
    efficiency_ratio REAL,
    PRIMARY KEY (device_id, date)
);

-- Operational decisions and outcomes (for model training)
CREATE TABLE dispatch_log (
    timestamp REAL PRIMARY KEY,
    solar_kw REAL,
    battery_kw REAL,
    diesel_kw REAL,
    demand_kw REAL,
    shed_kw REAL,
    soc_pct REAL,
    forecast_error_pct REAL,
    decision_source TEXT  -- 'optimizer', 'safety_override', 'manual'
);
```

---

## Fleet Sync Protocol

### Design Goals

- **No data loss**: All telemetry is persisted locally before any sync attempt
- **Bandwidth efficient**: Delta compression + aggregation for constrained links
- **Secure**: TLS + per-node API keys
- **Resilient**: Operates correctly with minutes, hours, or days between sync windows

### Protocol

```
Site Agent                          Fleet Broker (MQTT)
    |                                     |
    |-- [1s] Write to local journal ---+  |
    |                                  |  |
    |-- [5min] Check connectivity -----|  |
    |                                  |  |
    |   if online:                     |  |
    |-- MQTT CONNECT (TLS) -----------|->|
    |-- PUBLISH telemetry/site-id ----|->|  (compressed JSONL batch)
    |<- PUBACK ------------------------|--|
    |-- Mark batch as synced ----------|  |
    |                                  |  |
    |   if offline:                    |  |
    |-- Append to sync queue ----------|  |
    |   (SQLite WAL in data/sync-queue)|  |
    |                                     |
    |-- [next window] Drain queue ------->|
    |                                     |
```

### Topics

| Topic | Direction | Payload |
|-------|-----------|---------|
| `telemetry/{site_id}` | Agent -> Broker | Compressed JSONL batch of readings |
| `commands/{site_id}` | Broker -> Agent | Remote configuration updates |
| `alerts/{site_id}` | Agent -> Broker | Critical alerts (fault, low SOC) |
| `status/{site_id}` | Agent -> Broker | Heartbeat every sync interval |

### Queue Management

The sync queue in `data/sync-queue/` uses SQLite in WAL mode for concurrent read/write. Records are batched by 5-minute windows and compressed with gzip before transmission. After successful MQTT PUBACK, the batch is marked as synced and eligible for cleanup. The queue retains up to 30 days of unsynced data (~50MB).

---

## Security Considerations

### Threat Model

The primary deployment environment is a physically accessible device in a remote community. The threat model focuses on:

1. **Accidental misconfiguration** (most likely)
2. **Physical access by untrained operators**
3. **Network-based attacks via MQTT** (if connectivity exists)
4. **Supply chain** (compromised dependencies)

### Mitigations

| Threat | Mitigation |
|--------|------------|
| Accidental unsafe dispatch | Autonomic safety gates with hard limits that cannot be configured away |
| Physical SD card removal | Read-only rootfs; data on separate partition with encryption at rest |
| MQTT man-in-the-middle | TLS required for all MQTT connections; broker certificate pinning |
| Unauthorized remote commands | Per-node API keys; command signing with HMAC-SHA256 |
| Dependency supply chain | Pinned dependencies with hash verification; minimal dependency set |
| Diesel runaway (stuck relay) | Hardware watchdog timer; independent diesel runtime counter |

### Safety Invariants

These are enforced in code and cannot be overridden by configuration:

1. Battery SOC never commanded below physical minimum (protects cells from deep discharge)
2. Diesel generator never runs continuously for more than 8 hours without a mandatory cooldown
3. Load shedding always respects the priority order -- critical loads (health post, water pump) are shed last
4. Any device fault triggers immediate isolation of the faulted device
5. The safety monitor loop runs independently of the optimizer -- a stuck optimizer does not block safety checks

---

## Deployment Architecture

### Single Node

```
+---[ Raspberry Pi ]-------------------------------+
|                                                  |
|  systemd service: microgrid-agent.service         |
|  +--------------------------------------------+ |
|  | Rust kernel (single static binary, ~15MB)   | |
|  |                                             | |
|  |  main.rs (tokio async control loop)         | |
|  |    +-- devices.rs (HAL: Modbus, VE.Direct)  | |
|  |    +-- dispatch.rs (LP solver: good_lp)     | |
|  |    +-- autonomic.rs (safety gates)          | |
|  |    +-- knowledge.rs (SQLite KG: rusqlite)   | |
|  |    +-- journal.rs (event journal: redb)     | |
|  |    +-- ml_bridge.rs (IPC to Python ML)      | |
|  |    +-- dashboard.rs (axum + HTMX :8080)     | |
|  |    +-- sync.rs (MQTT: rumqttc)              | |
|  +--------------------------------------------+ |
|  | Python ML worker (spawned on demand)        | |
|  |    +-- forecast.py (TFLite LSTM)            | |
|  |    +-- worker.py (IPC process)              | |
|  +--------------------------------------------+ |
|                                                  |
|  /var/lib/microgrid-agent/                       |
|    +-- journal.redb    (event journal)           |
|    +-- knowledge.db    (community context)       |
|    +-- sync-queue.db   (outbound queue)          |
|    +-- models/         (TFLite + BitNet weights)  |
|                                                  |
+--------------------------------------------------+
    |           |           |           |
    | RS-485    | VE.Direct | I2C       | USB
    |           |           |           |
  Inverter   Victron     Sensors     USB-serial
  BMS        MPPT        BH1750     adapters
  Genset                 DS18B20
```

> **Note**: The Python prototype (`prototype/`) provides an equivalent architecture in
> Python for rapid experimentation. For production deployments, use the Rust kernel.

### Fleet (Multiple Nodes)

```
+----------+    +----------+    +----------+
| Site A   |    | Site B   |    | Site C   |
| (RPi)    |    | (RPi)    |    | (RPi)    |
+----+-----+    +----+-----+    +----+-----+
     |               |               |
     | cellular      | satellite     | cellular
     |               |               |
+----+---------------+---------------+-----+
|              MQTT Broker                  |
|         (Mosquitto / EMQX)                |
+----+----------------------------------+---+
     |                                  |
+----+--------+               +---------+---+
| Fleet       |               | Grafana     |
| Dashboard   |               | Monitoring  |
| (optional)  |               | (optional)  |
+-------------+               +-------------+
```

Each site operates fully autonomously. The fleet broker and dashboards are optional infrastructure that provide aggregate visibility but are never required for site-level operation.
