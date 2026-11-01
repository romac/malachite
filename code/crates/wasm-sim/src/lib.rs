//! A deterministic, in-process simulation of Malachite consensus nodes.
//!
//! The crate only depends on the core consensus crates, so that it can be
//! compiled to `wasm32-unknown-unknown` and drive a browser visualization.

pub mod context;
pub mod sim;

pub use sim::{MsgKind, NodeStatus, SimConfig, SimEvent, Simulation};
