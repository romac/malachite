//! JavaScript bindings, exchanging data as JSON strings.

use std::time::Duration;

use malachitebft_core_types::LinearTimeouts;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::{SimConfig, Simulation};

/// Simulation configuration, as passed from JavaScript. Missing fields take
/// their default value. Durations are in milliseconds.
#[derive(Deserialize)]
#[serde(default)]
struct JsConfig {
    num_nodes: u32,
    seed: u32,
    min_delay_ms: u64,
    max_delay_ms: u64,
    drop_rate: f64,
    timeouts: JsTimeouts,
    sync_grace_ms: u64,
    /// Target block time, `0` for none.
    target_block_time_ms: u64,
}

impl Default for JsConfig {
    fn default() -> Self {
        let c = SimConfig::default();
        Self {
            num_nodes: c.num_nodes,
            seed: c.seed as u32,
            min_delay_ms: c.min_delay_ms,
            max_delay_ms: c.max_delay_ms,
            drop_rate: c.drop_rate,
            timeouts: JsTimeouts::from(c.timeouts),
            sync_grace_ms: c.sync_grace_ms,
            target_block_time_ms: c.target_block_time_ms.unwrap_or(0),
        }
    }
}

/// Consensus timeouts, in milliseconds.
#[derive(Deserialize)]
#[serde(default)]
struct JsTimeouts {
    propose: u64,
    propose_delta: u64,
    prevote: u64,
    prevote_delta: u64,
    precommit: u64,
    precommit_delta: u64,
    rebroadcast: u64,
    max_timeout: u64,
}

impl Default for JsTimeouts {
    fn default() -> Self {
        Self::from(LinearTimeouts::default())
    }
}

impl From<LinearTimeouts> for JsTimeouts {
    fn from(t: LinearTimeouts) -> Self {
        let ms = |d: Duration| d.as_millis() as u64;
        Self {
            propose: ms(t.propose),
            propose_delta: ms(t.propose_delta),
            prevote: ms(t.prevote),
            prevote_delta: ms(t.prevote_delta),
            precommit: ms(t.precommit),
            precommit_delta: ms(t.precommit_delta),
            rebroadcast: ms(t.rebroadcast),
            max_timeout: ms(t.max_timeout),
        }
    }
}

impl From<JsTimeouts> for LinearTimeouts {
    fn from(t: JsTimeouts) -> Self {
        let ms = Duration::from_millis;
        Self {
            propose: ms(t.propose),
            propose_delta: ms(t.propose_delta),
            prevote: ms(t.prevote),
            prevote_delta: ms(t.prevote_delta),
            precommit: ms(t.precommit),
            precommit_delta: ms(t.precommit_delta),
            rebroadcast: ms(t.rebroadcast),
            max_timeout: ms(t.max_timeout),
        }
    }
}

/// `0` means no target block time.
fn target(ms: u64) -> Option<u64> {
    (ms > 0).then_some(ms)
}

#[wasm_bindgen]
pub struct WasmSim {
    sim: Simulation,
}

#[wasm_bindgen]
impl WasmSim {
    /// Create a simulation from a JSON configuration. See `JsConfig`.
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<WasmSim, JsError> {
        let c: JsConfig = serde_json::from_str(config_json)?;
        if c.num_nodes == 0 {
            return Err(JsError::new("at least one node is required"));
        }
        Ok(Self {
            sim: Simulation::new(SimConfig {
                num_nodes: c.num_nodes,
                seed: u64::from(c.seed),
                min_delay_ms: c.min_delay_ms.min(c.max_delay_ms),
                max_delay_ms: c.max_delay_ms.max(c.min_delay_ms),
                drop_rate: c.drop_rate.clamp(0.0, 1.0),
                timeouts: c.timeouts.into(),
                sync_grace_ms: c.sync_grace_ms,
                target_block_time_ms: target(c.target_block_time_ms),
            }),
        })
    }

    /// Current virtual time, in milliseconds.
    pub fn now(&self) -> f64 {
        self.sim.now() as f64
    }

    /// Process all events up to `time_ms`, in virtual milliseconds.
    pub fn run_until(&mut self, time_ms: f64) {
        self.sim.run_until(time_ms.max(0.0) as u64);
    }

    /// JSON array of the events observed since the last call. See `SimEvent`.
    pub fn drain_events(&mut self) -> String {
        serde_json::to_string(&self.sim.drain_events()).expect("events serialize to JSON")
    }

    /// JSON array of node statuses. See `NodeStatus`.
    pub fn statuses(&self) -> String {
        serde_json::to_string(&self.sim.statuses()).expect("statuses serialize to JSON")
    }

    pub fn set_online(&mut self, node: u32, online: bool) {
        self.sim.set_online(node, online);
    }

    pub fn set_drop_rate(&mut self, drop_rate: f64) {
        self.sim.set_drop_rate(drop_rate);
    }

    pub fn set_delay_ms(&mut self, min: u32, max: u32) {
        self.sim.set_delay_ms(u64::from(min), u64::from(max));
    }

    /// Change the consensus timeouts, given as JSON. See `JsTimeouts`.
    /// Applies to timeouts scheduled from now on.
    pub fn set_timeouts(&mut self, timeouts_json: &str) -> Result<(), JsError> {
        let t: JsTimeouts = serde_json::from_str(timeouts_json)?;
        self.sim.set_timeouts(t.into());
        Ok(())
    }

    pub fn set_sync_grace_ms(&mut self, sync_grace_ms: u32) {
        self.sim.set_sync_grace_ms(u64::from(sync_grace_ms));
    }

    /// Change the target block time, `0` for none.
    pub fn set_target_block_time_ms(&mut self, target_ms: u32) {
        self.sim
            .set_target_block_time_ms(target(u64::from(target_ms)));
    }
}
