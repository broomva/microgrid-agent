# microgrid-agent

**Open-source edge AI agent for autonomous renewable energy microgrid management.**

![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)
![Rust](https://img.shields.io/badge/Kernel-Rust-orange.svg)
![Python 3.11+](https://img.shields.io/badge/ML-Python%203.11%2B-blue.svg)
![Platform: RPi 4/5](https://img.shields.io/badge/Platform-RPi%204%2F5-red.svg)
![Status: Active Development](https://img.shields.io/badge/Status-Active%20Development-yellow.svg)

## Architecture

```
kernel/    — Rust daemon (always-on, no GC, ~15MB binary)
             Sensors, dispatch, safety, journal, dashboard, fleet sync
ml/        — Python ML worker (spawned on demand by kernel)
             TFLite LSTM forecasting, model retraining
prototype/ — Python prototype (reference implementation, hackable)
             Full agent in Python for rapid experimentation
```

---

## The Problem

An estimated 1.9 million people in Colombia's Zonas No Interconectadas (ZNI) lack access to reliable electricity. These communities -- scattered across the Pacific coast, the Amazon basin, the Orinoco plains, and island territories -- depend on isolated microgrids that run predominantly on imported diesel fuel. Roughly 78% of ZNI energy comes from diesel generators, driving electricity costs 3-5x higher than the national interconnected grid. The fuel itself must travel by river or air, arriving weeks late during rainy season, leaving entire communities in the dark.

Existing solutions fall into two camps, and neither works. Commercial microgrid controllers (Schneider, ABB, Siemens) cost $15,000-50,000 per node, require proprietary software licenses, and assume reliable internet connectivity for cloud-based optimization -- assumptions that collapse in a community reachable only by a six-hour boat ride from the nearest cellular tower. On the academic side, dozens of simulation papers propose sophisticated multi-agent optimization algorithms for microgrids, but they run on MATLAB or cloud GPUs, never touching real hardware or surviving a week of actual operation in the field.

No system exists today that provides autonomous, ML-based energy management optimized for disconnected, resource-constrained environments -- a system that can run on a $35 single-board computer, make intelligent dispatch decisions without any internet connection, and cost less than a single month of diesel savings to deploy.

## The Solution

`microgrid-agent` is an edge-first AI agent that runs entirely on a Raspberry Pi. It reads power from solar panels, batteries, and diesel generators via standard industrial protocols (Modbus RTU, Victron VE.Direct), forecasts demand and solar generation using lightweight ML models, and dispatches energy to minimize diesel consumption while protecting critical community loads.

**Total node cost: ~$650 USD** (Raspberry Pi + RS-485 HAT + sensors + enclosure).

```
Architecture Overview
=====================

                          +------------------+
                          |   Fleet Broker   |
                          |  (MQTT, cloud)   |
                          +--------+---------+
                                   |  store-and-forward
                                   |  (works offline)
            +----------------------+----------------------+
            |                      |                      |
    +-------+-------+     +-------+-------+      +-------+-------+
    | Site: Guainia  |     | Site: Choco   |      | Site: Vaupes  |
    | RPi 5 (8GB)   |     | RPi 4 (4GB)   |      | RPi 5 (4GB)   |
    +-------+-------+     +-------+-------+      +-------+-------+
            |                      |                      |
    +-------+-------+     +-------+-------+      +-------+-------+
    | Agent Core    |     | Agent Core    |      | Agent Core    |
    | +----------+  |     | +----------+  |      | +----------+  |
    | | Forecast |  |     | | Forecast |  |      | | Forecast |  |
    | | (TFLite) |  |     | | (TFLite) |  |      | | (TFLite) |  |
    | +----------+  |     | +----------+  |      | +----------+  |
    | | Dispatch |  |     | | Dispatch |  |      | | Dispatch |  |
    | | (LP opt) |  |     | | (LP opt) |  |      | | (LP opt) |  |
    | +----------+  |     | +----------+  |      | +----------+  |
    | | Autonomic|  |     | | Autonomic|  |      | | Autonomic|  |
    | | (safety) |  |     | | (safety) |  |      | | (safety) |  |
    | +----------+  |     | +----------+  |      | +----------+  |
    +-------+-------+     +-------+-------+      +-------+-------+
            |                      |                      |
    +-------+-------+     +-------+-------+      +-------+-------+
    | Modbus / VE.D |     | Modbus / VE.D |      | Modbus / VE.D |
    | Solar  Batt   |     | Solar  Batt   |      | Solar  Batt   |
    | Diesel Loads  |     | Diesel Loads  |      | Diesel Loads  |
    +---------------+     +---------------+      +---------------+
```

### Key Capabilities

- **ML Forecasting**: TensorFlow Lite LSTM models for solar irradiance and demand prediction. Inference in <0.5ms on RPi 5.
- **LP Dispatch Optimization**: Linear programming solver prioritizes solar, then battery, then diesel -- minimizing fuel consumption while meeting all loads.
- **Knowledge Graph**: SQLite-backed territorial context (<100MB) encodes community patterns -- market days, festivals, rainy seasons -- improving forecast accuracy.
- **Autonomic Safety Layer**: Hard safety constraints (SOC limits, diesel runtime caps, load shedding priority) that the ML layer can never override.
- **Fleet Sync**: MQTT-based store-and-forward telemetry. Queues data locally during connectivity outages, syncs when a link is available.
- **100% Offline Operation**: Every feature works without internet. Connectivity is optional and used only for fleet coordination.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/broomva/microgrid-agent.git
cd microgrid-agent

# Install in development mode
pip install -e ".[dev]"

# Run in simulation mode (no hardware needed)
python -m microgrid_agent --config config/site.example.toml --simulate

# Or use make
make simulate
```

Simulation mode creates virtual solar panels, batteries, a diesel generator, and community loads with realistic diurnal patterns. You can explore the full control loop without any physical hardware.

## Hardware Setup

For deploying on a real microgrid, see the [DIY Guide](docs/diy-guide.md) for step-by-step instructions.

### Bill of Materials (Minimum Viable Node)

| Component | Model | Approx. Cost |
|-----------|-------|-------------|
| Single-board computer | Raspberry Pi 5 (8GB) | $80 |
| RS-485 HAT | Waveshare RS485 CAN HAT | $15 |
| Current transformers (x4) | SCT-013-030 (30A) | $40 |
| Irradiance sensor | BH1750 I2C lux sensor | $5 |
| Temperature sensor | DS18B20 waterproof | $5 |
| MicroSD card | 64GB A2 rated | $12 |
| Enclosure | IP65 junction box | $20 |
| Power supply | 5V 5A USB-C (solar-powered) | $15 |
| Wiring and connectors | Misc terminals, cable | $30 |
| **Total** | | **~$220** |

Add the cost of a Victron MPPT controller (~$200) and a small inverter (~$230) if not already installed, bringing a complete new-install node to ~$650.

### Wiring Overview

- **RS-485**: Connects to inverters and charge controllers via Modbus RTU (2-wire, half-duplex)
- **VE.Direct**: Connects to Victron MPPT controllers via serial TTL cable
- **I2C**: Irradiance sensor (BH1750) and temperature sensor (DS18B20)
- **Current Transformers**: Clamp-on CTs on AC distribution lines for load measurement

See [docs/architecture.md](docs/architecture.md) for detailed wiring diagrams and pinout tables.

## Configuration

The agent is configured via two TOML files:

### `config/site.toml`

Defines the site identity, grid topology, equipment specifications, autonomic controller setpoints, and community context. Copy `config/site.example.toml` to get started:

```toml
[site]
id = "site-guainia-001"
name = "Demo Microgrid -- Guainia"
latitude = 3.8653
longitude = -67.9239

[grid]
type = "hybrid"           # solar + battery + diesel
peak_load_kw = 45.0

[autonomic]
min_soc_pct = 15          # load shedding threshold
diesel_start_soc = 20     # diesel auto-start
renewable_target = 0.85   # 85% renewable fraction goal
```

### `config/devices.toml`

Defines each physical (or simulated) device on the microgrid bus. Supports `modbus_rtu`, `vedirect`, and `simulated` protocols:

```toml
[[device]]
id = "solar-array-1"
name = "PV Array North"
type = "solar"
protocol = "modbus_rtu"
port = "/dev/ttyUSB0"
slave_id = 1
```

## Architecture

```
Multi-Rate Control Loop
=======================

    +----------+     +----------+     +----------+
    | 100ms    |     | 1s       |     | 15min    |
    | Safety   |---->| Device   |---->| Forecast |
    | Monitor  |     | Polling  |     | + Optim  |
    +----------+     +----------+     +----------+
         |                |                |
         v                v                v
    Hard limits      Read sensors     ML inference
    SOC bounds       Update state     LP dispatch
    Fault detect     Log telemetry    Schedule plan
    Emergency shed   Dashboard push   Sync fleet
```

```
Module Map
==========

    src/
    +-- agent.py          Main control loop & orchestrator
    +-- devices.py        Hardware abstraction (Modbus, VE.Direct, simulated)
    +-- forecast.py       TFLite LSTM solar & demand forecasting
    +-- dispatch.py       LP optimizer (scipy.optimize.linprog)
    +-- knowledge.py      SQLite knowledge graph (community context)
    +-- sync.py           MQTT fleet sync (store-and-forward)
    +-- autonomic.py      Safety constraints & homeostasis controller
    +-- dashboard.py      Local web dashboard (FastAPI, lightweight)
    +-- telemetry.py      Structured event logging & metrics
```

For the full technical architecture reference, see [docs/architecture.md](docs/architecture.md).

## DIY Guide

Want to build your own microgrid agent node? The [DIY Guide](docs/diy-guide.md) walks you through the entire process:

1. **Hardware Assembly** -- component selection, wiring, enclosure
2. **OS Setup** -- Raspberry Pi OS configuration, read-only rootfs
3. **Software Installation** -- clone, install, configure
4. **Equipment Connection** -- RS-485, VE.Direct, sensor wiring
5. **Calibration** -- device health checks, SOC baseline
6. **Shadow Mode** -- observe AI decisions before giving it control
7. **Go Live** -- switch to active mode with monitoring

No prior experience with energy systems required. The simulation mode lets you learn the system before touching any hardware.

## Development

```bash
# Install with dev dependencies
pip install -e ".[dev]"

# Run tests
make test

# Lint
make lint

# Format
make format

# Run simulation
make simulate
```

## Contributing

Contributions are welcome. This project exists to make autonomous energy management accessible to communities that need it most.

### How to Contribute

1. **Fork** the repository
2. **Create a branch** for your feature (`git checkout -b feature/my-feature`)
3. **Write tests** for any new functionality
4. **Run the linter** (`make lint`) and **tests** (`make test`) before submitting
5. **Open a Pull Request** with a clear description of the change

### Areas Where Help is Needed

- **Device drivers**: Support for additional inverters, charge controllers, and meters
- **Forecasting models**: Better solar irradiance and demand prediction for tropical climates
- **Fleet protocols**: Satellite-based sync for sites with no cellular coverage
- **Documentation**: Translations to Spanish for ZNI community deployment
- **Field testing**: Real-world validation data from microgrid deployments

### Code Standards

- Python 3.11+ with type hints on all functions
- asyncio for all I/O operations
- Ruff for linting and formatting
- pytest for testing
- Structured JSON logging (no print statements)

## License

MIT License. See [LICENSE](LICENSE) for details.

This is open-source software. Use it, modify it, deploy it. If it saves a community from burning diesel, that is the point.

## Acknowledgments

- Research conducted as part of the MAIA (Maestria en Inteligencia Artificial) capstone at **Universidad de los Andes**
- Supported by the **TICSw research group** (A1 classification, Minciencias)
- Inspired by the **Husk Power Systems** fleet intelligence model for distributed mini-grid management
- Built on Colombia's ZNI electrification data from **IPSE** (Instituto de Planificacion y Promocion de Soluciones Energeticas)
- Solar irradiance data from **IDEAM** (Instituto de Hidrologia, Meteorologia y Estudios Ambientales)
