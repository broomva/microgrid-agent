-- =============================================================================
-- Microgrid Agent — Knowledge Graph Schema (SQLite)
-- =============================================================================
-- This schema defines the knowledge graph used by the microgrid agent for
-- reasoning about site topology, device relationships, load patterns, and
-- dispatch decisions.
--
-- Usage:
--   sqlite3 knowledge.db < knowledge-graph.sql
-- =============================================================================

-- Enable WAL mode for better concurrent read/write performance
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- =============================================================================
-- Core tables
-- =============================================================================

-- Entities: nodes in the knowledge graph (devices, loads, zones, weather, etc.)
CREATE TABLE IF NOT EXISTS entities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    type        TEXT NOT NULL,           -- e.g. 'device', 'load', 'zone', 'weather_station'
    name        TEXT NOT NULL,           -- human-readable name
    properties  TEXT DEFAULT '{}',       -- JSON blob for flexible attributes
    created_at  TEXT DEFAULT (datetime('now')),
    updated_at  TEXT DEFAULT (datetime('now'))
);

-- Relations: edges in the knowledge graph (directed, weighted)
CREATE TABLE IF NOT EXISTS relations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation    TEXT NOT NULL,           -- e.g. 'powers', 'monitors', 'located_in', 'depends_on'
    target_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    weight      REAL DEFAULT 1.0,       -- edge weight (e.g. capacity fraction, priority)
    properties  TEXT DEFAULT '{}',       -- JSON blob for edge attributes
    created_at  TEXT DEFAULT (datetime('now'))
);

-- Patterns: learned statistical patterns from observations
CREATE TABLE IF NOT EXISTS patterns (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    pattern_type TEXT NOT NULL,          -- e.g. 'load_profile', 'solar_profile', 'demand_spike'
    hour        INTEGER,                -- hour of day (0-23), NULL for day/month patterns
    day         INTEGER,                -- day of week (0=Mon, 6=Sun), NULL for hour/month patterns
    month       INTEGER,                -- month (1-12), NULL for hour/day patterns
    avg_load    REAL NOT NULL,          -- average observed value (kW, W/m2, etc.)
    std_load    REAL DEFAULT 0.0,       -- standard deviation of observations
    count       INTEGER DEFAULT 1,      -- number of observations aggregated
    updated_at  TEXT DEFAULT (datetime('now'))
);

-- Readings: time-series storage for device telemetry
CREATE TABLE IF NOT EXISTS readings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL,           -- ISO 8601 timestamp
    device_id   TEXT NOT NULL,           -- matches entity name or external device ID
    metric      TEXT NOT NULL,           -- metric name (e.g. 'pv_power', 'soc', 'load_kw')
    value       REAL NOT NULL            -- metric value in engineering units
);

-- Decisions: audit log of all dispatch decisions made by the agent
CREATE TABLE IF NOT EXISTS decisions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT DEFAULT (datetime('now')),
    action      TEXT NOT NULL,           -- e.g. 'start_diesel', 'shed_load:community_center', 'curtail_solar'
    reasoning   TEXT DEFAULT '{}',       -- JSON: why the decision was made (inputs, constraints, scores)
    overridden  INTEGER DEFAULT 0        -- 1 if a human operator overrode this decision
);

-- =============================================================================
-- Performance indexes
-- =============================================================================

-- Entity lookups by type
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);

-- Relation traversal
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id, relation);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id, relation);

-- Pattern lookups by entity and time dimensions
CREATE INDEX IF NOT EXISTS idx_patterns_entity ON patterns(entity_id, pattern_type);
CREATE INDEX IF NOT EXISTS idx_patterns_hour ON patterns(entity_id, hour);
CREATE INDEX IF NOT EXISTS idx_patterns_day ON patterns(entity_id, day);

-- Time-series queries on readings (most common query pattern)
CREATE INDEX IF NOT EXISTS idx_readings_device_time ON readings(device_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_readings_metric_time ON readings(metric, timestamp);
CREATE INDEX IF NOT EXISTS idx_readings_time ON readings(timestamp);

-- Decision audit queries
CREATE INDEX IF NOT EXISTS idx_decisions_time ON decisions(timestamp);
CREATE INDEX IF NOT EXISTS idx_decisions_action ON decisions(action);

-- =============================================================================
-- Example seed data for a simulated site
-- =============================================================================

-- Site entities
INSERT INTO entities (type, name, properties) VALUES
    ('site', 'demo_site', '{"region": "Guainía", "population": 850}'),
    ('zone', 'generation_zone', '{"description": "Solar array and diesel generator area"}'),
    ('zone', 'storage_zone', '{"description": "Battery bank and inverters"}'),
    ('zone', 'distribution_zone', '{"description": "Load distribution and metering"}');

-- Device entities
INSERT INTO entities (type, name, properties) VALUES
    ('device', 'solar_inverter', '{"capacity_kwp": 60.0, "type": "pv_inverter", "manufacturer": "SMA"}'),
    ('device', 'battery_inverter', '{"capacity_kwh": 120.0, "chemistry": "lfp", "manufacturer": "Victron"}'),
    ('device', 'diesel_genset', '{"capacity_kw": 30.0, "fuel_tank_liters": 500}'),
    ('device', 'irradiance_sensor', '{"type": "pyranometer", "model": "BH1750"}');

-- Load entities
INSERT INTO entities (type, name, properties) VALUES
    ('load', 'health_post', '{"priority": 1, "peak_kw": 5.0, "critical": true}'),
    ('load', 'water_pump', '{"priority": 2, "peak_kw": 3.0, "critical": true}'),
    ('load', 'communications_tower', '{"priority": 3, "peak_kw": 2.0, "critical": true}'),
    ('load', 'school', '{"priority": 4, "peak_kw": 8.0, "critical": false}'),
    ('load', 'community_center', '{"priority": 5, "peak_kw": 10.0, "critical": false}'),
    ('load', 'residential_cluster_a', '{"priority": 6, "peak_kw": 12.0, "critical": false}');

-- Weather entity
INSERT INTO entities (type, name, properties) VALUES
    ('weather', 'local_weather', '{"source": "sensor", "forecast_hours": 24}');

-- Relations: topology
INSERT INTO relations (source_id, relation, target_id, weight, properties) VALUES
    -- Solar inverter powers the distribution zone
    (5, 'powers', 4, 1.0, '{"connection": "ac_bus"}'),
    -- Battery inverter powers the distribution zone
    (6, 'powers', 4, 1.0, '{"connection": "ac_bus", "bidirectional": true}'),
    -- Diesel powers the distribution zone
    (7, 'powers', 4, 1.0, '{"connection": "ac_bus"}'),
    -- Devices located in zones
    (5, 'located_in', 2, 1.0, '{}'),
    (6, 'located_in', 3, 1.0, '{}'),
    (7, 'located_in', 2, 1.0, '{}'),
    (8, 'located_in', 2, 1.0, '{}'),
    -- Loads connected to distribution
    (9, 'connected_to', 4, 1.0, '{"breaker": "CB-01"}'),
    (10, 'connected_to', 4, 1.0, '{"breaker": "CB-02"}'),
    (11, 'connected_to', 4, 1.0, '{"breaker": "CB-03"}'),
    (12, 'connected_to', 4, 1.0, '{"breaker": "CB-04"}'),
    (13, 'connected_to', 4, 1.0, '{"breaker": "CB-05"}'),
    (14, 'connected_to', 4, 1.0, '{"breaker": "CB-06"}'),
    -- Irradiance sensor monitors solar inverter
    (8, 'monitors', 5, 1.0, '{"metric": "irradiance_wm2"}'),
    -- Weather affects solar output
    (15, 'affects', 5, 0.8, '{"mechanism": "cloud_cover"}');

-- Example load patterns (hourly profile for a typical day)
INSERT INTO patterns (entity_id, pattern_type, hour, avg_load, std_load, count) VALUES
    -- Health post: fairly constant, slight peak in morning
    (9, 'load_profile', 0, 2.0, 0.3, 30),
    (9, 'load_profile', 6, 3.0, 0.5, 30),
    (9, 'load_profile', 8, 4.5, 0.8, 30),
    (9, 'load_profile', 12, 4.0, 0.6, 30),
    (9, 'load_profile', 18, 3.5, 0.5, 30),
    (9, 'load_profile', 22, 2.5, 0.3, 30),
    -- School: high during school hours
    (12, 'load_profile', 7, 6.0, 1.0, 30),
    (12, 'load_profile', 12, 7.5, 1.2, 30),
    (12, 'load_profile', 15, 4.0, 0.8, 30),
    (12, 'load_profile', 18, 1.0, 0.3, 30),
    -- Solar irradiance profile
    (8, 'solar_profile', 6, 50.0, 20.0, 30),
    (8, 'solar_profile', 9, 600.0, 80.0, 30),
    (8, 'solar_profile', 12, 900.0, 100.0, 30),
    (8, 'solar_profile', 15, 650.0, 90.0, 30),
    (8, 'solar_profile', 18, 80.0, 30.0, 30);

-- Example readings (a few sample data points)
INSERT INTO readings (timestamp, device_id, metric, value) VALUES
    ('2026-03-30T06:00:00', 'solar_inverter', 'pv_power', 0.0),
    ('2026-03-30T06:00:00', 'battery_inverter', 'soc', 72.0),
    ('2026-03-30T06:00:00', 'irradiance_sensor', 'irradiance_wm2', 0.0),
    ('2026-03-30T09:00:00', 'solar_inverter', 'pv_power', 35000.0),
    ('2026-03-30T09:00:00', 'battery_inverter', 'soc', 78.0),
    ('2026-03-30T09:00:00', 'irradiance_sensor', 'irradiance_wm2', 620.0),
    ('2026-03-30T12:00:00', 'solar_inverter', 'pv_power', 52000.0),
    ('2026-03-30T12:00:00', 'battery_inverter', 'soc', 89.0),
    ('2026-03-30T12:00:00', 'irradiance_sensor', 'irradiance_wm2', 880.0);

-- Example decisions
INSERT INTO decisions (timestamp, action, reasoning, overridden) VALUES
    ('2026-03-30T05:30:00', 'start_diesel', '{"trigger": "soc_below_threshold", "soc": 18.5, "threshold": 20, "load_kw": 15.2}', 0),
    ('2026-03-30T07:15:00', 'stop_diesel', '{"trigger": "soc_above_threshold", "soc": 62.0, "threshold": 60, "solar_kw": 22.0}', 0),
    ('2026-03-30T19:00:00', 'shed_load:community_center', '{"trigger": "soc_forecast_low", "predicted_soc_midnight": 12.0, "current_soc": 45.0}', 1);
