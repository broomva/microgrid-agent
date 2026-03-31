//! Autonomic safety controller.
//!
//! The last line of defense before actuation. Enforces hard safety
//! constraints on every dispatch decision, overriding the optimizer
//! when necessary to protect equipment and maintain minimum service.
//!
//! Follows the Life Agent OS Autonomic pattern: observe → compare → act.

use tracing::{info, warn};

use crate::config::AutonomicSection;
use crate::dispatch::DispatchDecision;

// ---------------------------------------------------------------------------
// Autonomic controller
// ---------------------------------------------------------------------------

/// Safety controller that enforces operational constraints on dispatch decisions.
///
/// Constraints enforced:
/// - `min_soc_pct`: Never discharge battery below this threshold.
/// - `max_soc_pct`: Stop charging above this threshold.
/// - `diesel_start_soc_pct`: Auto-start diesel when SOC drops below this.
/// - `diesel_stop_soc_pct`: Auto-stop diesel when SOC rises above this.
/// - `max_diesel_hours_per_day`: Limit daily diesel runtime.
pub struct AutonomicController {
    min_soc_pct: f64,
    max_soc_pct: f64,
    diesel_start_soc_pct: f64,
    diesel_stop_soc_pct: f64,
    max_diesel_hours_per_day: f64,
    /// Accumulated diesel runtime today (hours). Reset at midnight.
    diesel_hours_today: std::sync::Mutex<f64>,
}

impl AutonomicController {
    /// Create a new autonomic controller from the site autonomic configuration.
    pub fn new(config: &AutonomicSection) -> Self {
        info!(
            min_soc = config.min_soc_pct,
            max_soc = config.max_soc_pct,
            diesel_start = config.diesel_start_soc_pct,
            diesel_stop = config.diesel_stop_soc_pct,
            max_diesel_hours = config.max_diesel_hours_per_day,
            "Autonomic controller initialized"
        );

        Self {
            min_soc_pct: config.min_soc_pct,
            max_soc_pct: config.max_soc_pct,
            diesel_start_soc_pct: config.diesel_start_soc_pct,
            diesel_stop_soc_pct: config.diesel_stop_soc_pct,
            max_diesel_hours_per_day: config.max_diesel_hours_per_day,
            diesel_hours_today: std::sync::Mutex::new(0.0),
        }
    }

    /// Enforce safety constraints on a dispatch decision.
    ///
    /// Takes the optimizer's decision and the current agent state,
    /// and returns a (possibly modified) decision with safety overrides applied.
    /// All overrides are logged via `tracing`.
    pub fn enforce(
        &self,
        mut decision: DispatchDecision,
        state: &crate::AgentState,
    ) -> DispatchDecision {
        let soc = state.latest_readings.battery_soc_pct;
        let mut was_overridden = false;

        // --- Shield 1: Minimum SOC protection ---
        // Prevent battery discharge when SOC is at or below minimum
        if soc <= self.min_soc_pct && decision.battery_kw < 0.0 {
            warn!(
                soc,
                min_soc = self.min_soc_pct,
                original_battery_kw = decision.battery_kw,
                "OVERRIDE: Blocking battery discharge — SOC at minimum"
            );
            decision.battery_kw = 0.0;
            was_overridden = true;
        }

        // --- Shield 2: Maximum SOC protection ---
        // Prevent overcharging
        if soc >= self.max_soc_pct && decision.battery_kw > 0.0 {
            warn!(
                soc,
                max_soc = self.max_soc_pct,
                original_battery_kw = decision.battery_kw,
                "OVERRIDE: Blocking battery charge — SOC at maximum"
            );
            decision.battery_kw = 0.0;
            was_overridden = true;
        }

        // --- Shield 3: Auto-start diesel on low SOC ---
        if soc <= self.diesel_start_soc_pct && !decision.diesel_start && decision.diesel_kw == 0.0 {
            let diesel_hours = self.diesel_hours_today.lock().unwrap();
            if *diesel_hours < self.max_diesel_hours_per_day {
                warn!(
                    soc,
                    threshold = self.diesel_start_soc_pct,
                    "OVERRIDE: Auto-starting diesel — SOC critically low"
                );
                decision.diesel_start = true;
                // TODO: Set diesel_kw to a reasonable default based on diesel capacity
                decision.diesel_kw = 5.0; // Conservative default
                was_overridden = true;
            } else {
                warn!(
                    diesel_hours = *diesel_hours,
                    max_hours = self.max_diesel_hours_per_day,
                    "Diesel auto-start blocked — daily runtime limit reached"
                );
            }
        }

        // --- Shield 4: Auto-stop diesel on high SOC ---
        if soc >= self.diesel_stop_soc_pct && decision.diesel_kw > 0.0 && !decision.diesel_stop {
            warn!(
                soc,
                threshold = self.diesel_stop_soc_pct,
                "OVERRIDE: Auto-stopping diesel — SOC recovered"
            );
            decision.diesel_kw = 0.0;
            decision.diesel_stop = true;
            decision.diesel_start = false;
            was_overridden = true;
        }

        // --- Shield 5: Daily diesel runtime limit ---
        {
            let mut diesel_hours = self.diesel_hours_today.lock().unwrap();
            if decision.diesel_kw > 0.0 {
                // Approximate: each dispatch cycle adds ~dispatch_interval_s / 3600 hours
                // TODO: Use actual elapsed time between dispatches
                let increment = 5.0 / 3600.0; // Assume 5-second dispatch interval
                *diesel_hours += increment;

                if *diesel_hours >= self.max_diesel_hours_per_day {
                    warn!(
                        diesel_hours = *diesel_hours,
                        max_hours = self.max_diesel_hours_per_day,
                        "OVERRIDE: Shutting down diesel — daily runtime limit reached"
                    );
                    decision.diesel_kw = 0.0;
                    decision.diesel_stop = true;
                    decision.diesel_start = false;
                    was_overridden = true;
                }
            }

            // TODO: Reset diesel_hours_today at midnight (requires a timer or
            //       checking the current date against a stored last-reset date).
        }

        if was_overridden {
            decision.overridden = true;
            decision.reasoning = format!(
                "{} [AUTONOMIC OVERRIDE: soc={:.0}%]",
                decision.reasoning, soc
            );
        }

        decision
    }
}
