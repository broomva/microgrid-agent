"""
Tests for the knowledge graph.

Covers:
- Entity and relation CRUD operations
- Recursive CTE graph traversal
- Pattern learning from observations
"""

import json
import sqlite3
import os
import tempfile

import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

SCHEMA_PATH = os.path.join(
    os.path.dirname(os.path.dirname(__file__)),
    "schema",
    "knowledge-graph.sql",
)


@pytest.fixture
def db():
    """Create a temporary in-memory database with the schema loaded."""
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")

    # Load schema (without seed data)
    schema_sql = _load_schema_without_seeds()
    conn.executescript(schema_sql)

    yield conn
    conn.close()


@pytest.fixture
def seeded_db():
    """Create a temporary database with schema AND seed data."""
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")

    with open(SCHEMA_PATH, "r") as f:
        schema_sql = f.read()
    conn.executescript(schema_sql)

    yield conn
    conn.close()


def _load_schema_without_seeds():
    """Load only the CREATE TABLE/INDEX statements, skip INSERTs."""
    with open(SCHEMA_PATH, "r") as f:
        full_sql = f.read()

    # Split on the seed data marker and take only the schema part
    marker = "-- Example seed data"
    if marker in full_sql:
        return full_sql[: full_sql.index(marker)]
    return full_sql


# ===========================================================================
# Entity CRUD Tests
# ===========================================================================

class TestEntityCRUD:
    """Test basic entity create, read, update, delete."""

    def test_create_entity(self, db):
        """Create an entity and verify it exists."""
        db.execute(
            "INSERT INTO entities (type, name, properties) VALUES (?, ?, ?)",
            ("device", "test_inverter", json.dumps({"capacity_kw": 10.0})),
        )
        db.commit()

        row = db.execute("SELECT * FROM entities WHERE name = 'test_inverter'").fetchone()
        assert row is not None
        assert row["type"] == "device"
        assert json.loads(row["properties"])["capacity_kw"] == 10.0

    def test_create_multiple_entities(self, db):
        """Create several entities of different types."""
        entities = [
            ("device", "solar_panel", "{}"),
            ("device", "battery", "{}"),
            ("load", "hospital", '{"priority": 1}'),
            ("load", "school", '{"priority": 4}'),
            ("zone", "generation", "{}"),
        ]
        db.executemany(
            "INSERT INTO entities (type, name, properties) VALUES (?, ?, ?)",
            entities,
        )
        db.commit()

        count = db.execute("SELECT COUNT(*) FROM entities").fetchone()[0]
        assert count == 5

        devices = db.execute("SELECT COUNT(*) FROM entities WHERE type = 'device'").fetchone()[0]
        assert devices == 2

    def test_update_entity_properties(self, db):
        """Update an entity's properties."""
        db.execute(
            "INSERT INTO entities (type, name, properties) VALUES (?, ?, ?)",
            ("device", "inverter", json.dumps({"firmware": "1.0"})),
        )
        db.commit()

        db.execute(
            "UPDATE entities SET properties = ?, updated_at = datetime('now') WHERE name = ?",
            (json.dumps({"firmware": "2.0", "upgraded": True}), "inverter"),
        )
        db.commit()

        row = db.execute("SELECT properties FROM entities WHERE name = 'inverter'").fetchone()
        props = json.loads(row["properties"])
        assert props["firmware"] == "2.0"
        assert props["upgraded"] is True

    def test_delete_entity_cascades_relations(self, db):
        """Deleting an entity should cascade delete its relations."""
        db.execute("INSERT INTO entities (type, name) VALUES ('device', 'src')")
        db.execute("INSERT INTO entities (type, name) VALUES ('zone', 'tgt')")
        db.commit()

        src_id = db.execute("SELECT id FROM entities WHERE name = 'src'").fetchone()[0]
        tgt_id = db.execute("SELECT id FROM entities WHERE name = 'tgt'").fetchone()[0]

        db.execute(
            "INSERT INTO relations (source_id, relation, target_id) VALUES (?, 'located_in', ?)",
            (src_id, tgt_id),
        )
        db.commit()

        # Verify relation exists
        rel_count = db.execute("SELECT COUNT(*) FROM relations").fetchone()[0]
        assert rel_count == 1

        # Delete source entity
        db.execute("DELETE FROM entities WHERE id = ?", (src_id,))
        db.commit()

        # Relation should be cascade-deleted
        rel_count = db.execute("SELECT COUNT(*) FROM relations").fetchone()[0]
        assert rel_count == 0


# ===========================================================================
# Relation CRUD Tests
# ===========================================================================

class TestRelationCRUD:
    """Test relation create, read, and traversal."""

    def test_create_relation(self, db):
        """Create a relation between two entities."""
        db.execute("INSERT INTO entities (type, name) VALUES ('device', 'solar')")
        db.execute("INSERT INTO entities (type, name) VALUES ('zone', 'gen_zone')")
        db.commit()

        solar_id = db.execute("SELECT id FROM entities WHERE name = 'solar'").fetchone()[0]
        zone_id = db.execute("SELECT id FROM entities WHERE name = 'gen_zone'").fetchone()[0]

        db.execute(
            "INSERT INTO relations (source_id, relation, target_id, weight) VALUES (?, ?, ?, ?)",
            (solar_id, "located_in", zone_id, 1.0),
        )
        db.commit()

        rel = db.execute(
            "SELECT * FROM relations WHERE source_id = ? AND relation = 'located_in'",
            (solar_id,),
        ).fetchone()

        assert rel is not None
        assert rel["target_id"] == zone_id
        assert rel["weight"] == 1.0

    def test_bidirectional_query(self, db):
        """Query relations in both directions."""
        db.execute("INSERT INTO entities (type, name) VALUES ('device', 'a')")
        db.execute("INSERT INTO entities (type, name) VALUES ('device', 'b')")
        db.commit()

        id_a = db.execute("SELECT id FROM entities WHERE name = 'a'").fetchone()[0]
        id_b = db.execute("SELECT id FROM entities WHERE name = 'b'").fetchone()[0]

        db.execute(
            "INSERT INTO relations (source_id, relation, target_id) VALUES (?, 'powers', ?)",
            (id_a, id_b),
        )
        db.commit()

        # Forward: a powers b
        forward = db.execute(
            "SELECT target_id FROM relations WHERE source_id = ? AND relation = 'powers'",
            (id_a,),
        ).fetchall()
        assert len(forward) == 1
        assert forward[0]["target_id"] == id_b

        # Reverse: what powers b?
        reverse = db.execute(
            "SELECT source_id FROM relations WHERE target_id = ? AND relation = 'powers'",
            (id_b,),
        ).fetchall()
        assert len(reverse) == 1
        assert reverse[0]["source_id"] == id_a

    def test_weighted_relations(self, db):
        """Relations with different weights should be queryable."""
        db.execute("INSERT INTO entities (type, name) VALUES ('device', 'src')")
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'high_pri')")
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'low_pri')")
        db.commit()

        src_id = db.execute("SELECT id FROM entities WHERE name = 'src'").fetchone()[0]
        hi_id = db.execute("SELECT id FROM entities WHERE name = 'high_pri'").fetchone()[0]
        lo_id = db.execute("SELECT id FROM entities WHERE name = 'low_pri'").fetchone()[0]

        db.execute(
            "INSERT INTO relations (source_id, relation, target_id, weight) VALUES (?, 'powers', ?, ?)",
            (src_id, hi_id, 0.9),
        )
        db.execute(
            "INSERT INTO relations (source_id, relation, target_id, weight) VALUES (?, 'powers', ?, ?)",
            (src_id, lo_id, 0.3),
        )
        db.commit()

        # Query ordered by weight descending
        rows = db.execute(
            "SELECT e.name, r.weight FROM relations r JOIN entities e ON r.target_id = e.id "
            "WHERE r.source_id = ? ORDER BY r.weight DESC",
            (src_id,),
        ).fetchall()

        assert len(rows) == 2
        assert rows[0]["name"] == "high_pri"
        assert rows[1]["name"] == "low_pri"


# ===========================================================================
# Recursive CTE Graph Traversal
# ===========================================================================

class TestGraphTraversal:
    """Test recursive CTE queries for graph exploration."""

    def test_find_all_downstream_loads(self, seeded_db):
        """Find all loads reachable from the distribution zone via 'connected_to'."""
        # In the seed data, loads are connected_to the distribution_zone (id=4)
        rows = seeded_db.execute("""
            WITH RECURSIVE downstream AS (
                SELECT id, name, type FROM entities WHERE name = 'distribution_zone'
                UNION ALL
                SELECT e.id, e.name, e.type
                FROM entities e
                JOIN relations r ON r.source_id = e.id
                JOIN downstream d ON r.target_id = d.id
                WHERE r.relation = 'connected_to'
            )
            SELECT name, type FROM downstream WHERE type = 'load'
        """).fetchall()

        load_names = {row["name"] for row in rows}
        assert "health_post" in load_names
        assert "school" in load_names
        assert "community_center" in load_names
        assert len(load_names) >= 5, f"Expected at least 5 loads, got {len(load_names)}"

    def test_find_all_power_sources(self, seeded_db):
        """Find all devices that power the distribution zone."""
        rows = seeded_db.execute("""
            SELECT e.name, e.type
            FROM relations r
            JOIN entities e ON r.source_id = e.id
            WHERE r.relation = 'powers'
              AND r.target_id = (SELECT id FROM entities WHERE name = 'distribution_zone')
        """).fetchall()

        source_names = {row["name"] for row in rows}
        assert "solar_inverter" in source_names
        assert "battery_inverter" in source_names
        assert "diesel_genset" in source_names

    def test_transitive_reachability(self, db):
        """Test multi-hop graph traversal with recursive CTE."""
        # Build a chain: A -> B -> C -> D
        for name in ["A", "B", "C", "D"]:
            db.execute("INSERT INTO entities (type, name) VALUES ('node', ?)", (name,))
        db.commit()

        ids = {}
        for name in ["A", "B", "C", "D"]:
            ids[name] = db.execute("SELECT id FROM entities WHERE name = ?", (name,)).fetchone()[0]

        for src, tgt in [("A", "B"), ("B", "C"), ("C", "D")]:
            db.execute(
                "INSERT INTO relations (source_id, relation, target_id) VALUES (?, 'connects', ?)",
                (ids[src], ids[tgt]),
            )
        db.commit()

        # Find all nodes reachable from A
        rows = db.execute("""
            WITH RECURSIVE reachable AS (
                SELECT id, name, 0 AS depth FROM entities WHERE name = 'A'
                UNION ALL
                SELECT e.id, e.name, r2.depth + 1
                FROM entities e
                JOIN relations r ON r.source_id = r2.id AND r.target_id = e.id AND r.relation = 'connects'
                JOIN reachable r2 ON r.source_id = r2.id
            )
            SELECT DISTINCT name, depth FROM reachable ORDER BY depth
        """).fetchall()

        names = [row["name"] for row in rows]
        assert names == ["A", "B", "C", "D"], f"Expected [A,B,C,D] got {names}"

    def test_cycle_detection_with_limit(self, db):
        """Recursive CTE should handle cycles with LIMIT."""
        db.execute("INSERT INTO entities (type, name) VALUES ('node', 'X')")
        db.execute("INSERT INTO entities (type, name) VALUES ('node', 'Y')")
        db.commit()

        x_id = db.execute("SELECT id FROM entities WHERE name = 'X'").fetchone()[0]
        y_id = db.execute("SELECT id FROM entities WHERE name = 'Y'").fetchone()[0]

        # Create a cycle: X -> Y -> X
        db.execute("INSERT INTO relations (source_id, relation, target_id) VALUES (?, 'links', ?)", (x_id, y_id))
        db.execute("INSERT INTO relations (source_id, relation, target_id) VALUES (?, 'links', ?)", (y_id, x_id))
        db.commit()

        # Query with LIMIT to prevent infinite recursion
        rows = db.execute("""
            WITH RECURSIVE traverse AS (
                SELECT id, name, 0 AS depth FROM entities WHERE name = 'X'
                UNION ALL
                SELECT e.id, e.name, t.depth + 1
                FROM entities e
                JOIN relations r ON r.target_id = e.id
                JOIN traverse t ON r.source_id = t.id
                WHERE t.depth < 5
            )
            SELECT name, depth FROM traverse LIMIT 20
        """).fetchall()

        # Should not crash, and should have entries
        assert len(rows) > 0
        assert len(rows) <= 20, "LIMIT should prevent unbounded results"


# ===========================================================================
# Pattern Learning Tests
# ===========================================================================

class TestPatternLearning:
    """Test pattern learning from observation data."""

    def test_insert_pattern(self, db):
        """Insert a load pattern observation."""
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'test_load')")
        db.commit()
        entity_id = db.execute("SELECT id FROM entities WHERE name = 'test_load'").fetchone()[0]

        db.execute(
            "INSERT INTO patterns (entity_id, pattern_type, hour, avg_load, std_load, count) "
            "VALUES (?, 'load_profile', 12, 5.0, 0.8, 1)",
            (entity_id,),
        )
        db.commit()

        row = db.execute(
            "SELECT * FROM patterns WHERE entity_id = ? AND hour = 12",
            (entity_id,),
        ).fetchone()

        assert row is not None
        assert row["avg_load"] == 5.0
        assert row["count"] == 1

    def test_update_running_average(self, db):
        """Simulate incremental pattern learning with running average."""
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'sensor')")
        db.commit()
        entity_id = db.execute("SELECT id FROM entities WHERE name = 'sensor'").fetchone()[0]

        # Insert initial observation
        db.execute(
            "INSERT INTO patterns (entity_id, pattern_type, hour, avg_load, std_load, count) "
            "VALUES (?, 'load_profile', 14, 10.0, 0.0, 1)",
            (entity_id,),
        )
        db.commit()

        # Simulate a new observation of 12.0 kW at hour 14
        new_value = 12.0
        db.execute("""
            UPDATE patterns
            SET avg_load = (avg_load * count + ?) / (count + 1),
                count = count + 1,
                updated_at = datetime('now')
            WHERE entity_id = ? AND pattern_type = 'load_profile' AND hour = 14
        """, (new_value, entity_id))
        db.commit()

        row = db.execute(
            "SELECT avg_load, count FROM patterns WHERE entity_id = ? AND hour = 14",
            (entity_id,),
        ).fetchone()

        assert row["count"] == 2
        assert row["avg_load"] == pytest.approx(11.0, abs=0.01), \
            "Running average of (10.0 + 12.0) / 2 should be 11.0"

    def test_pattern_query_by_time(self, db):
        """Query patterns for a specific time window."""
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'pump')")
        db.commit()
        entity_id = db.execute("SELECT id FROM entities WHERE name = 'pump'").fetchone()[0]

        # Insert patterns for different hours
        for hour, load in [(6, 3.0), (7, 5.0), (8, 7.0), (12, 4.0), (18, 6.0)]:
            db.execute(
                "INSERT INTO patterns (entity_id, pattern_type, hour, avg_load, count) "
                "VALUES (?, 'load_profile', ?, ?, 10)",
                (entity_id, hour, load),
            )
        db.commit()

        # Query morning hours (6-9)
        rows = db.execute(
            "SELECT hour, avg_load FROM patterns "
            "WHERE entity_id = ? AND hour BETWEEN 6 AND 9 ORDER BY hour",
            (entity_id,),
        ).fetchall()

        assert len(rows) == 3
        assert rows[1]["avg_load"] == 5.0  # hour 7

    def test_seed_data_patterns(self, seeded_db):
        """Verify seed data patterns are loaded correctly."""
        # Check that the health_post has load patterns
        health_id = seeded_db.execute(
            "SELECT id FROM entities WHERE name = 'health_post'"
        ).fetchone()[0]

        patterns = seeded_db.execute(
            "SELECT COUNT(*) FROM patterns WHERE entity_id = ?",
            (health_id,),
        ).fetchone()[0]

        assert patterns > 0, "Health post should have load patterns from seed data"

    def test_aggregate_daily_pattern(self, db):
        """Test aggregating hourly patterns into a daily summary."""
        db.execute("INSERT INTO entities (type, name) VALUES ('load', 'agg_test')")
        db.commit()
        entity_id = db.execute("SELECT id FROM entities WHERE name = 'agg_test'").fetchone()[0]

        # Insert 24 hours of pattern data
        for hour in range(24):
            load = 5.0 + 10.0 * (1 if 7 <= hour <= 20 else 0)  # 5kW base, +10kW during day
            db.execute(
                "INSERT INTO patterns (entity_id, pattern_type, hour, avg_load, count) "
                "VALUES (?, 'load_profile', ?, ?, 30)",
                (entity_id, hour, load),
            )
        db.commit()

        # Aggregate: average load across all hours
        row = db.execute(
            "SELECT AVG(avg_load) as daily_avg, MAX(avg_load) as peak, MIN(avg_load) as min_load "
            "FROM patterns WHERE entity_id = ?",
            (entity_id,),
        ).fetchone()

        assert row["peak"] == 15.0, "Peak should be 15kW (5 base + 10 day)"
        assert row["min_load"] == 5.0, "Minimum should be 5kW (night base)"
        assert row["daily_avg"] > 5.0, "Daily average should be above night base"
