//! Knowledge graph — structured information about the microgrid site.
//!
//! Stores and queries relationships between community entities, loads,
//! schedules, and priorities. Uses SQLite with recursive CTEs for
//! graph traversal queries.
//!
//! Example queries:
//! - "What loads are affected if feeder-2 trips?"
//! - "Which loads are priority during a community health emergency?"
//! - "What is the expected load profile on market days?"

use std::path::Path;

use rusqlite::Connection;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Knowledge graph
// ---------------------------------------------------------------------------

/// An embedded knowledge graph backed by SQLite.
///
/// Stores entities (loads, feeders, buildings, services) and their
/// relationships as a directed graph. Supports recursive CTE queries
/// for impact analysis and priority resolution.
pub struct KnowledgeGraph {
    conn: tokio::sync::Mutex<Connection>,
}

impl KnowledgeGraph {
    /// Open (or create) the knowledge graph database at the given path.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Initialize schema
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS entities (
                id          TEXT PRIMARY KEY,
                kind        TEXT NOT NULL,  -- 'load', 'feeder', 'building', 'service'
                label       TEXT,
                priority    INTEGER DEFAULT 0,  -- 0=normal, 1=essential, 2=critical
                metadata    TEXT  -- JSON blob for extra attributes
            );

            CREATE TABLE IF NOT EXISTS edges (
                source_id   TEXT NOT NULL,
                target_id   TEXT NOT NULL,
                relation    TEXT NOT NULL,  -- 'feeds', 'contains', 'depends_on', 'serves'
                weight      REAL DEFAULT 1.0,
                PRIMARY KEY (source_id, target_id, relation),
                FOREIGN KEY (source_id) REFERENCES entities(id),
                FOREIGN KEY (target_id) REFERENCES entities(id)
            );

            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_entities_kind ON entities(kind);
            CREATE INDEX IF NOT EXISTS idx_entities_priority ON entities(priority);
            ",
        )?;

        info!(path = %path.display(), "Knowledge graph opened");
        Ok(Self {
            conn: tokio::sync::Mutex::new(conn),
        })
    }

    /// Get all load entity IDs marked as priority (priority >= 1).
    ///
    /// Returns entity IDs sorted by priority descending (critical first).
    pub async fn get_priority_loads(&self) -> Vec<String> {
        let conn = self.conn.lock().await;

        let result: Vec<String> = conn
            .prepare(
                "SELECT id FROM entities WHERE kind = 'load' AND priority >= 1 ORDER BY priority DESC",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        debug!(count = result.len(), "Queried priority loads");
        result
    }

    /// Query all entities affected by an event on the given entity.
    ///
    /// Uses a recursive CTE to walk the graph downstream from the
    /// event source, following 'feeds', 'contains', and 'depends_on'
    /// relationships.
    pub async fn query_affected(&self, event_entity_id: &str) -> Vec<String> {
        let conn = self.conn.lock().await;
        let entity_id = event_entity_id.to_string();

        let result: Vec<String> = conn
            .prepare(
                "
                WITH RECURSIVE affected(id, depth) AS (
                    -- Base case: the event source
                    SELECT ?, 0
                    UNION
                    -- Recursive case: follow downstream edges
                    SELECT e.target_id, a.depth + 1
                    FROM affected a
                    JOIN edges e ON e.source_id = a.id
                    WHERE e.relation IN ('feeds', 'contains', 'depends_on')
                      AND a.depth < 10  -- prevent infinite loops
                )
                SELECT DISTINCT id FROM affected WHERE depth > 0
                ORDER BY depth
                ",
            )
            .and_then(|mut stmt| {
                stmt.query_map([&entity_id], |row| row.get(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        debug!(
            event = event_entity_id,
            affected_count = result.len(),
            "Queried affected entities"
        );
        result
    }

    // TODO: Add methods for:
    // - `insert_entity(id, kind, label, priority)` — add/update an entity
    // - `insert_edge(source, target, relation, weight)` — add a relationship
    // - `get_load_profile(day_type: &str) -> HashMap<String, f64>` — expected load by entity
    // - `import_from_toml(path: &Path)` — bulk-import site topology from config
    // - `shortest_path(from, to) -> Vec<String>` — find path between entities
}
