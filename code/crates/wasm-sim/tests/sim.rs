use arc_malachitebft_wasm_sim::{MsgKind, SimConfig, SimEvent, Simulation};

/// Run until every online node has decided `heights` heights, or `max_time_ms` elapses.
fn run(sim: &mut Simulation, heights: usize, max_time_ms: u64) {
    while sim.now() < max_time_ms
        && sim
            .statuses()
            .iter()
            .filter(|s| s.online)
            .any(|s| s.decisions.len() < heights)
    {
        if !sim.step() {
            break;
        }
    }
}

fn assert_agreement(sim: &Simulation, heights: usize) {
    let statuses = sim.statuses();
    let reference = &statuses[0].decisions[..heights];
    for s in &statuses {
        assert_eq!(
            &s.decisions[..heights],
            reference,
            "node {} disagrees",
            s.node
        );
    }
}

#[test]
fn four_nodes_decide_ten_heights() {
    let mut sim = Simulation::new(SimConfig::default());
    run(&mut sim, 10, 600_000);

    for s in sim.statuses() {
        assert!(
            s.decisions.len() >= 10,
            "node {} only decided {:?}",
            s.node,
            s.decisions
        );
    }
    assert_agreement(&sim, 10);

    // Round-robin proposers with no faults: the proposer of height h is node (h - 1) % 4,
    // and it proposes the value h * 100 + its index.
    let expected: Vec<u64> = (1..=10).map(|h| h * 100 + (h - 1) % 4).collect();
    assert_eq!(sim.statuses()[0].decisions[..10], expected[..]);
}

#[test]
fn runs_are_deterministic() {
    let config = SimConfig {
        seed: 42,
        drop_rate: 0.1,
        ..SimConfig::default()
    };

    let trace = |config: SimConfig| {
        let mut sim = Simulation::new(config);
        run(&mut sim, 5, 600_000);
        sim.drain_events()
    };

    assert_eq!(trace(config.clone()), trace(config));
}

#[test]
fn lossy_network_still_agrees() {
    let mut sim = Simulation::new(SimConfig {
        seed: 7,
        drop_rate: 0.2,
        ..SimConfig::default()
    });
    run(&mut sim, 5, 600_000);

    for s in sim.statuses() {
        assert!(
            s.decisions.len() >= 5,
            "node {} only decided {:?}",
            s.node,
            s.decisions
        );
    }
    assert_agreement(&sim, 5);
}

#[test]
fn one_crashed_node_does_not_stop_progress() {
    let mut sim = Simulation::new(SimConfig::default());
    sim.set_online(3, false);
    run(&mut sim, 5, 600_000);

    let statuses = sim.statuses();
    for s in statuses.iter().filter(|s| s.online) {
        assert!(
            s.decisions.len() >= 5,
            "node {} only decided {:?}",
            s.node,
            s.decisions
        );
    }
    assert!(statuses[3].decisions.is_empty());

    // Height 4 was node 3's turn to propose: the others must have moved to round 1.
    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(
        e,
        SimEvent::RoundStarted {
            height: 4,
            round: 1,
            ..
        }
    )));
}

#[test]
fn two_crashed_nodes_stop_progress_until_one_returns() {
    let mut sim = Simulation::new(SimConfig::default());
    sim.set_online(2, false);
    sim.set_online(3, false);
    sim.run_until(30_000);

    // 2 out of 4 is not a quorum.
    assert!(sim.statuses().iter().all(|s| s.decisions.is_empty()));

    sim.set_online(3, true);
    run(&mut sim, 3, 600_000);

    for s in sim.statuses().iter().filter(|s| s.online) {
        assert!(
            s.decisions.len() >= 3,
            "node {} only decided {:?}",
            s.node,
            s.decisions
        );
    }
}

#[test]
fn node_rejoining_after_others_moved_on_catches_up_via_sync() {
    let mut sim = Simulation::new(SimConfig::default());
    sim.set_online(3, false);

    // Let the others decide a few heights without node 3.
    while sim.statuses()[0].decisions.len() < 3 {
        assert!(sim.step());
    }
    assert!(sim.statuses()[3].decisions.is_empty());

    sim.set_online(3, true);
    run(&mut sim, 6, 600_000);

    for s in sim.statuses() {
        assert!(
            s.decisions.len() >= 6,
            "node {} only decided {:?}",
            s.node,
            s.decisions
        );
    }
    assert_agreement(&sim, 6);

    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(
        e,
        SimEvent::MessageSent {
            from: 3,
            kind: MsgKind::SyncRequest,
            ..
        }
    )));
}

#[test]
fn node_learns_it_is_behind_from_status_while_the_others_are_stalled() {
    // Long timeouts, so that no consensus message flows while quorum is lost.
    let mut config = SimConfig::default();
    config.timeouts.propose = std::time::Duration::from_secs(600);
    config.timeouts.rebroadcast = std::time::Duration::from_secs(600);
    let mut sim = Simulation::new(config);

    sim.set_online(3, false);
    while sim.statuses()[0].decisions.len() < 3 {
        assert!(sim.step());
    }

    // Without node 0, nodes 1 and 2 soon get stuck. Node 3 comes back at height 1.
    sim.set_online(0, false);
    sim.set_online(3, true);
    sim.run_until(sim.now() + 1_000);
    sim.drain_events();
    sim.run_until(sim.now() + 10_000);

    // Nodes 1 and 2 send no consensus messages while stuck,
    // so only their statuses can have told node 3 that they are ahead.
    let events = sim.drain_events();
    let first_request = events
        .iter()
        .position(|e| {
            matches!(
                e,
                SimEvent::MessageSent {
                    from: 3,
                    kind: MsgKind::SyncRequest,
                    ..
                }
            )
        })
        .expect("node 3 requests sync");
    assert!(events[..first_request].iter().all(|e| !matches!(
        e,
        SimEvent::MessageSent {
            from: 1 | 2,
            kind: MsgKind::Proposal | MsgKind::Prevote | MsgKind::Precommit,
            ..
        }
    )));

    let statuses = sim.statuses();
    assert!(statuses[1].decisions.len() >= 3);
    assert_eq!(statuses[3].decisions, statuses[1].decisions);
}

#[test]
fn no_sync_without_faults() {
    let mut sim = Simulation::new(SimConfig::default());
    run(&mut sim, 20, 600_000);

    let events = sim.drain_events();
    assert!(!events.iter().any(|e| matches!(
        e,
        SimEvent::MessageSent {
            kind: MsgKind::SyncRequest,
            ..
        }
    )));
}

/// Times at which `node` started each height, from its round 0 `RoundStarted` events.
fn height_starts(events: &[SimEvent], node: u32) -> Vec<(u64, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            SimEvent::RoundStarted {
                time,
                node: n,
                height,
                round: 0,
                ..
            } if *n == node => Some((*height, *time)),
            _ => None,
        })
        .collect()
}

#[test]
fn target_block_time_spaces_heights_evenly() {
    let config = SimConfig {
        target_block_time_ms: Some(1500),
        ..SimConfig::default()
    };

    let trace = |config: SimConfig| {
        let mut sim = Simulation::new(config);
        run(&mut sim, 10, 600_000);
        assert_agreement(&sim, 10);
        sim.drain_events()
    };

    let events = trace(config.clone());
    assert_eq!(
        events,
        trace(config),
        "runs with a target are deterministic"
    );

    // Heights take far less than 1.5 s to decide on this network, so every node
    // starts a new height exactly every 1.5 s.
    for node in 0..4 {
        let starts = height_starts(&events, node);
        assert!(starts.len() >= 10);
        for pair in starts.windows(2) {
            assert_eq!(pair[1].1 - pair[0].1, 1500, "node {node}: {pair:?}");
        }
    }
}

#[test]
fn slow_height_does_not_wait_for_the_target() {
    let config = SimConfig {
        target_block_time_ms: Some(1000),
        ..SimConfig::default()
    };
    let mut sim = Simulation::new(config);

    // Height 4 is proposed by node 3: without it, height 4 needs a second round,
    // which takes longer than the target, so height 5 starts right after deciding.
    sim.set_online(3, false);
    run(&mut sim, 6, 600_000);
    let events = sim.drain_events();
    let starts = height_starts(&events, 0);
    let start = |h: u64| starts.iter().find(|(height, _)| *height == h).unwrap().1;
    let decided_4 = events
        .iter()
        .find_map(|e| match e {
            SimEvent::Decided {
                time,
                node: 0,
                height: 4,
                ..
            } => Some(*time),
            _ => None,
        })
        .unwrap();

    assert_eq!(start(3) - start(2), 1000);
    assert!(start(5) - start(4) > 1000);
    assert_eq!(start(5), decided_4);
}

#[test]
fn node_offline_during_finalization_period_resumes() {
    let config = SimConfig {
        target_block_time_ms: Some(2000),
        ..SimConfig::default()
    };
    let mut sim = Simulation::new(config);

    // Wait until node 3 decides height 1, then crash it while it waits for the target.
    while !sim.drain_events().iter().any(|e| {
        matches!(
            e,
            SimEvent::Decided {
                node: 3,
                height: 1,
                ..
            }
        )
    }) {
        assert!(sim.step());
    }
    sim.set_online(3, false);

    // The end of its finalization period passes while it is offline.
    sim.run_until(sim.now() + 3000);
    sim.set_online(3, true);

    run(&mut sim, 5, 600_000);
    assert!(sim.statuses()[3].decisions.len() >= 5);
    assert_agreement(&sim, 5);
}
