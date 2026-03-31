//! Dispatch optimizer.
//!
//! Computes the optimal power dispatch decision for each control cycle.
//! Uses a rule-based priority strategy (solar -> battery -> diesel) with
//! a placeholder for LP optimization via `good_lp`.

use tracing::info;

use crate::config::SiteConfig;
use crate::knowledge::KnowledgeGraph;

// ---------------------------------------------------------------------------
// Dispatch decision — the canonical "action" type for the control loop
// ---------------------------------------------------------------------------

/// A dispatch decision produced by the optimizer and potentially modified
/// by the autonomic safety controller.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DispatchDecision {
    /// Solar power to deliver (kW).
    pub solar_kw: f64,
    /// Battery power (kW). Positive = charging, negative = discharging.
    pub battery_kw: f64,
    /// Diesel generator output (kW).
    pub diesel_kw: f64,
    /// Load shed (kW) — demand that cannot be met.
    pub load_shed_kw: f64,
    /// Whether to start the diesel generator.
    pub diesel_start: bool,
    /// Whether to stop the diesel generator.
    pub diesel_stop: bool,
    /// Human-readable reasoning for this decision.
    pub reasoning: String,
    /// Whether the autonomic controller overrode the optimizer's decision.
    pub overridden: bool,
}

impl Default for DispatchDecision {
    fn default() -> Self {
        Self {
            solar_kw: 0.0,
            battery_kw: 0.0,
            diesel_kw: 0.0,
            load_shed_kw: 0.0,
            diesel_start: false,
            diesel_stop: false,
            reasoning: String::new(),
            overridden: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// The dispatch optimizer. Computes power allocation each control cycle.
pub struct Dispatcher {
    solar_capacity_kwp: f64,
    battery_capacity_kwh: f64,
    diesel_capacity_kw: f64,
    max_dod: f64,
}

impl Dispatcher {
    /// Create a new dispatcher from the site configuration.
    pub fn new(config: &SiteConfig) -> Self {
        Self {
            solar_capacity_kwp: config.solar.capacity_kwp,
            battery_capacity_kwh: config.battery.capacity_kwh,
            diesel_capacity_kw: config.diesel.capacity_kw,
            max_dod: config.battery.max_dod,
        }
    }

    /// Compute the dispatch decision for the current state.
    ///
    /// Priority order: solar -> battery discharge -> diesel.
    /// Excess solar charges the battery.
    ///
    /// The `_kg` parameter is available for knowledge-graph-informed
    /// dispatch (e.g. priority loads, market days) but is not used yet.
    pub async fn solve(
        &self,
        state: &crate::AgentState,
        _kg: &KnowledgeGraph,
    ) -> DispatchDecision {
        let readings = &state.latest_readings;
        let load = readings.load_kw;
        let solar = readings.solar_kw;
        let soc = readings.battery_soc_pct;

        // --- Rule-based dispatch (solar -> battery -> diesel) ---

        let mut decision = DispatchDecision::default();
        let mut remaining_load = load;

        // 1. Use all available solar
        let solar_to_load = solar.min(remaining_load);
        decision.solar_kw = solar_to_load;
        remaining_load -= solar_to_load;

        // Excess solar charges battery
        let excess_solar = solar - solar_to_load;
        if excess_solar > 0.0 && soc < 95.0 {
            decision.battery_kw = excess_solar; // positive = charging
        }

        // 2. Discharge battery to cover remaining load
        let min_soc = (1.0 - self.max_dod) * 100.0;
        if remaining_load > 0.0 && soc > min_soc {
            let max_battery_discharge = self.battery_capacity_kwh * 0.5; // C/2 rate limit
            let battery_discharge = remaining_load.min(max_battery_discharge);
            decision.battery_kw = -battery_discharge; // negative = discharging
            remaining_load -= battery_discharge;
        }

        // 3. Start diesel if still unmet load
        if remaining_load > 0.5 {
            // >500W threshold to avoid hunting
            let diesel_output = remaining_load.min(self.diesel_capacity_kw);
            decision.diesel_kw = diesel_output;
            decision.diesel_start = true;
            remaining_load -= diesel_output;
        }

        // 4. Any remaining = load shed
        if remaining_load > 0.01 {
            decision.load_shed_kw = remaining_load;
        }

        // Build reasoning string
        decision.reasoning = format!(
            "Rule-based: solar={:.1}kW to load, battery={:.1}kW, diesel={:.1}kW, shed={:.1}kW (SOC={:.0}%)",
            decision.solar_kw, decision.battery_kw, decision.diesel_kw, decision.load_shed_kw, soc
        );

        if decision.load_shed_kw > 0.0 {
            tracing::warn!(
                shed_kw = decision.load_shed_kw,
                "Load shedding required — insufficient generation"
            );
        }

        info!(
            solar = decision.solar_kw,
            battery = decision.battery_kw,
            diesel = decision.diesel_kw,
            reasoning = %decision.reasoning,
            "Dispatch computed"
        );

        // TODO: Replace rule-based dispatch with LP optimization using good_lp.
        //       The LP formulation should minimize:
        //         cost = diesel_fuel_cost + battery_degradation_cost + load_shed_penalty
        //       Subject to:
        //         solar + battery_discharge + diesel - battery_charge = load - shed
        //         0 <= shed <= load
        //         0 <= diesel <= diesel_capacity
        //         min_soc <= soc_next <= max_soc
        //       Use KnowledgeGraph to query priority loads and adjust shed penalties.

        decision
    }
}
