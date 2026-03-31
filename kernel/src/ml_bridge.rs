//! Python ML subprocess bridge.
//!
//! Spawns a Python subprocess to run forecasting models (solar generation,
//! load demand). Communicates via stdin/stdout JSON protocol.
//!
//! Falls back to a naive persistence forecast when the Python worker
//! is unavailable or fails.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::devices::SensorReadings;

// ---------------------------------------------------------------------------
// Forecast type
// ---------------------------------------------------------------------------

/// A forecast produced by the ML model (or the persistence fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    /// Predicted generation for each hour in the forecast horizon (kW).
    pub generation_kw: Vec<f64>,
    /// Predicted demand for each hour in the forecast horizon (kW).
    pub demand_kw: Vec<f64>,
    /// Forecast horizon in hours.
    pub horizon_hours: f64,
}

impl Default for Forecast {
    fn default() -> Self {
        Self {
            generation_kw: vec![0.0; 24],
            demand_kw: vec![0.0; 24],
            horizon_hours: 24.0,
        }
    }
}

/// Request sent to the Python ML worker via stdin.
#[derive(Debug, Serialize)]
struct ForecastRequest {
    /// Recent sensor history for the model to use.
    history: Vec<HistoryPoint>,
    /// Requested forecast horizon in hours.
    horizon_hours: f64,
}

/// Simplified history point sent to the Python worker.
#[derive(Debug, Serialize)]
struct HistoryPoint {
    timestamp: String,
    solar_kw: f64,
    load_kw: f64,
    irradiance_wm2: f64,
    temperature_c: f64,
}

// ---------------------------------------------------------------------------
// ML bridge
// ---------------------------------------------------------------------------

/// Bridge to a Python ML forecasting subprocess.
///
/// The Python worker is expected at `{model_dir}/forecast_worker.py`
/// and communicates via JSON over stdin/stdout:
///
/// ```text
/// Rust -> stdin:  { "history": [...], "horizon_hours": 24.0 }
/// Python -> stdout: { "generation_kw": [...], "demand_kw": [...], "horizon_hours": 24.0 }
/// ```
pub struct MlBridge {
    model_dir: PathBuf,
    python_bin: String,
}

impl MlBridge {
    /// Create a new ML bridge pointing to the given model directory.
    pub fn new(model_dir: &Path) -> Self {
        let python_bin =
            std::env::var("PYTHON_BIN").unwrap_or_else(|_| "python3".to_string());

        info!(
            model_dir = %model_dir.display(),
            python = %python_bin,
            "ML bridge initialized"
        );

        Self {
            model_dir: model_dir.to_path_buf(),
            python_bin,
        }
    }

    /// Request a forecast from the Python ML worker.
    ///
    /// Spawns the worker subprocess, sends history data via stdin,
    /// and reads the forecast from stdout.
    pub async fn request_forecast(
        &self,
        history: &[SensorReadings],
    ) -> anyhow::Result<Forecast> {
        let worker_script = self.model_dir.join("forecast_worker.py");

        if !worker_script.exists() {
            anyhow::bail!(
                "ML worker script not found at {}",
                worker_script.display()
            );
        }

        // Build the request
        let request = ForecastRequest {
            history: history
                .iter()
                .map(|r| HistoryPoint {
                    timestamp: r.timestamp.to_rfc3339(),
                    solar_kw: r.solar_kw,
                    load_kw: r.load_kw,
                    irradiance_wm2: r.irradiance_wm2,
                    temperature_c: r.temperature_c,
                })
                .collect(),
            horizon_hours: 24.0,
        };

        let request_json = serde_json::to_string(&request)?;

        // Spawn the Python subprocess
        debug!(script = %worker_script.display(), "Spawning ML worker");

        let mut child = tokio::process::Command::new(&self.python_bin)
            .arg(&worker_script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Write request to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        // Read response from stdout
        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ML worker failed (exit {}): {}", output.status, stderr);
        }

        let forecast: Forecast = serde_json::from_slice(&output.stdout)?;
        debug!(
            horizon = forecast.horizon_hours,
            gen_points = forecast.generation_kw.len(),
            "Forecast received from ML worker"
        );

        Ok(forecast)
    }

    /// Naive persistence fallback: assume the next 24 hours look like
    /// the most recent 24 hours of history.
    ///
    /// Used when the Python ML worker is unavailable.
    pub fn persistence_fallback(&self, history: &[SensorReadings]) -> Forecast {
        warn!("Using persistence fallback forecast");

        if history.is_empty() {
            return Forecast::default();
        }

        // Take the last 24 data points (or fewer if not enough history)
        let window = if history.len() >= 24 {
            &history[history.len() - 24..]
        } else {
            history
        };

        let generation_kw: Vec<f64> = window.iter().map(|r| r.solar_kw).collect();
        let demand_kw: Vec<f64> = window.iter().map(|r| r.load_kw).collect();
        let horizon_hours = generation_kw.len() as f64;

        Forecast {
            generation_kw,
            demand_kw,
            horizon_hours,
        }
    }
}
