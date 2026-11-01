//! JavaScript bindings, exchanging data as JSON strings.

use wasm_bindgen::prelude::*;

use crate::{SimConfig, Simulation};

#[wasm_bindgen]
pub struct WasmSim {
    sim: Simulation,
}

#[wasm_bindgen]
impl WasmSim {
    #[wasm_bindgen(constructor)]
    pub fn new(
        num_nodes: u32,
        seed: u32,
        min_delay_ms: u32,
        max_delay_ms: u32,
        drop_rate: f64,
    ) -> WasmSim {
        Self {
            sim: Simulation::new(SimConfig {
                num_nodes,
                seed: u64::from(seed),
                min_delay_ms: u64::from(min_delay_ms.min(max_delay_ms)),
                max_delay_ms: u64::from(max_delay_ms.max(min_delay_ms)),
                drop_rate: drop_rate.clamp(0.0, 1.0),
                ..SimConfig::default()
            }),
        }
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
}
