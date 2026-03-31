//! Event journal — crash-safe append-only storage.
//!
//! Follows the Lago (Life Agent OS) event-sourcing pattern:
//! every sensor reading and dispatch decision is persisted as an
//! immutable event, enabling replay, audit, and post-hoc analysis.
//!
//! Uses `redb` for transactional, crash-safe embedded storage.

use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use tracing::debug;

use crate::devices::SensorReadings;
use crate::dispatch::DispatchDecision;

/// Table for sensor readings, keyed by monotonic event ID.
const READINGS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("readings");

/// Table for dispatch decisions, keyed by monotonic event ID.
const DECISIONS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("decisions");

/// Table for metadata (counters, checkpoints).
const META_TABLE: TableDefinition<&str, u64> = TableDefinition::new("meta");

// ---------------------------------------------------------------------------
// Event journal
// ---------------------------------------------------------------------------

/// Append-only event journal backed by `redb`.
///
/// All events are serialized to JSON and stored with a monotonically
/// increasing sequence number. The journal is crash-safe — partial
/// writes are rolled back on recovery.
pub struct EventJournal {
    db: Database,
}

impl EventJournal {
    /// Open (or create) the event journal at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Database::create(path)?;

        // Initialize tables on first open
        let write_txn = db.begin_write()?;
        {
            let _readings = write_txn.open_table(READINGS_TABLE)?;
            let _decisions = write_txn.open_table(DECISIONS_TABLE)?;
            let _meta = write_txn.open_table(META_TABLE)?;
        }
        write_txn.commit()?;

        debug!(path = %path.display(), "Event journal opened");
        Ok(Self { db })
    }

    /// Append a sensor readings event to the journal.
    pub fn append_readings(&self, readings: &SensorReadings) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(readings)?;
        let seq = self.next_seq("readings_seq")?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(READINGS_TABLE)?;
            table.insert(seq, payload.as_slice())?;
        }
        write_txn.commit()?;

        debug!(seq, "Appended sensor readings");
        Ok(())
    }

    /// Append a dispatch decision event to the journal.
    pub fn append_decision(&self, decision: &DispatchDecision) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(decision)?;
        let seq = self.next_seq("decisions_seq")?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DECISIONS_TABLE)?;
            table.insert(seq, payload.as_slice())?;
        }
        write_txn.commit()?;

        debug!(seq, "Appended dispatch decision");
        Ok(())
    }

    /// Get the next sequence number for a given counter, incrementing it atomically.
    fn next_seq(&self, counter_key: &str) -> anyhow::Result<u64> {
        let write_txn = self.db.begin_write()?;
        let seq;
        {
            let mut meta = write_txn.open_table(META_TABLE)?;
            let current = meta
                .get(counter_key)?
                .map(|v| v.value())
                .unwrap_or(0);
            seq = current + 1;
            meta.insert(counter_key, seq)?;
        }
        write_txn.commit()?;
        Ok(seq)
    }

    // TODO: Add methods for:
    // - `replay_readings(since: DateTime<Utc>) -> impl Iterator<Item = SensorReadings>`
    // - `replay_decisions(since: DateTime<Utc>) -> impl Iterator<Item = DispatchDecision>`
    // - `compact(keep_last_n: usize)` — remove old events to reclaim disk space
    // - `export_csv(path: &Path)` — export to CSV for external analysis
}
