# Malachite Metrics Documentation

This document provides a comprehensive overview of all Prometheus metrics exposed by the Malachite consensus implementation.

## Table of Contents

- [Core Consensus Metrics](#core-consensus-metrics)
  - [Timing Metrics](#timing-metrics)
  - [Round and Height Metrics](#round-and-height-metrics)
  - [Network Metrics](#network-metrics)
  - [Cryptographic Operation Metrics](#cryptographic-operation-metrics)
  - [Queue Metrics](#queue-metrics)
- [Sync Metrics](#sync-metrics)
  - [Value Sync Metrics](#value-sync-metrics)
  - [Peer Scoring Metrics](#peer-scoring-metrics)

---

## Core Consensus Metrics

Prefix: `malachitebft_core_consensus`

### Timing Metrics

#### `consensus_time`
**Full name**: `malachitebft_core_consensus_consensus_time`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Linear buckets (0.0 to 2.0s, 0.1s increments, 20 buckets)
- **Description**: Total time taken for the consensus process.

#### `time_per_block`
**Full name**: `malachitebft_core_consensus_time_per_block`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Linear buckets (0.0 to 2.0s, 0.1s increments, 20 buckets)
- **Description**: Time taken to finalize a block from start to end.

#### `time_per_step`
**Full name**: `malachitebft_core_consensus_time_per_step`

- **Type**: Histogram (with labels)
- **Unit**: Seconds
- **Buckets**: Linear buckets (0.0 to 2.0s, 0.1s increments, 20 buckets)
- **Labels**: `step` (Step enum: Propose, Prevote, Precommit, Commit, etc.)
- **Description**: Time taken for each step within a consensus round. Tracks performance of individual consensus phases.

### Round and Height Metrics

#### `consensus_round`
**Full name**: `malachitebft_core_consensus_consensus_round`

- **Type**: Histogram
- **Unit**: Round number
- **Buckets**: Linear buckets (0.0 to 20.0, 1.0 increments, 20 buckets)
- **Description**: The consensus round in which the node was when it finalized a block. Higher values may indicate network issues or disagreement.

#### `proposal_round`
**Full name**: `malachitebft_core_consensus_proposal_round`

- **Type**: Histogram
- **Unit**: Round number
- **Buckets**: Linear buckets (0.0 to 20.0, 1.0 increments, 20 buckets)
- **Description**: The round of the proposal that was ultimately decided on. Can differ from `consensus_round` in cases where earlier proposals were rejected.

#### `height`
**Full name**: `malachitebft_core_consensus_height`

- **Type**: Gauge
- **Description**: Current blockchain height being processed by the consensus engine.

#### `round`
**Full name**: `malachitebft_core_consensus_round`

- **Type**: Gauge
- **Description**: Current consensus round number within the current height.

### Network Metrics

#### `rebroadcast_timeouts`
**Full name**: `malachitebft_core_consensus_rebroadcast_timeouts`

- **Type**: Counter
- **Description**: Number of times consensus rebroadcasted Prevote or Precommit votes due to no round progress. High values indicate network delays or peer connectivity issues.

#### `connected_peers`
**Full name**: `malachitebft_core_consensus_connected_peers`

- **Type**: Gauge
- **Description**: Number of peers currently connected to this consensus node. Represents the node's view of network connectivity.

### Cryptographic Operation Metrics

#### `signature_signing_time`
**Full name**: `malachitebft_core_consensus_signature_signing_time`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Exponential buckets (0.001 to ~0.512s, factor of 2.0, 10 buckets)
- **Description**: Time taken to sign a message using the node's private key.

#### `signature_verification_time`
**Full name**: `malachitebft_core_consensus_signature_verification_time`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Exponential buckets (0.001 to ~0.512s, factor of 2.0, 10 buckets)
- **Description**: Time taken to verify a signature from another node.

### Queue Metrics

#### `queue_heights`
**Full name**: `malachitebft_core_consensus_queue_heights`

- **Type**: Gauge
- **Description**: Number of distinct heights currently in the consensus input queue. Indicates how many different heights have pending messages.

#### `queue_size`
**Full name**: `malachitebft_core_consensus_queue_size`

- **Type**: Gauge
- **Description**: Total number of inputs in the consensus input queue across all heights. High values may indicate processing bottlenecks.

---

## Sync Metrics

Prefix: `malachitebft_sync`

### Value Sync Metrics

These metrics track the request-response pattern for syncing block values between nodes.

#### `value_requests_sent`
**Full name**: `malachitebft_sync_value_requests_sent`

- **Type**: Counter
- **Description**: Total number of ValueSync requests sent by this node to fetch missing block data from peers.

#### `value_requests_received`
**Full name**: `malachitebft_sync_value_requests_received`

- **Type**: Counter
- **Description**: Total number of ValueSync requests received by this node from other peers.

#### `value_responses_sent`
**Full name**: `malachitebft_sync_value_responses_sent`

- **Type**: Counter
- **Description**: Total number of ValueSync responses sent by this node in reply to peer requests.

#### `value_responses_received`
**Full name**: `malachitebft_sync_value_responses_received`

- **Type**: Counter
- **Description**: Total number of ValueSync responses received by this node from peers.

#### `value_client_latency`
**Full name**: `malachitebft_sync_value_client_latency`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Exponential buckets (0.1 to ~52,428.8s, factor of 2.0, 20 buckets)
- **Description**: Interval of time between when a ValueSync request was sent and when the corresponding response was received. Measures round-trip latency from the client perspective.

#### `value_server_latency`
**Full name**: `malachitebft_sync_value_server_latency`

- **Type**: Histogram
- **Unit**: Seconds
- **Buckets**: Exponential buckets (0.1 to ~52,428.8s, factor of 2.0, 20 buckets)
- **Description**: Interval of time between when a ValueSync request was received and when the response was sent. Measures processing time from the server perspective.

#### `value_request_timeouts`
**Full name**: `malachitebft_sync_value_request_timeouts`

- **Type**: Counter
- **Description**: Number of ValueSync requests that timed out without receiving a response. High values indicate peer reliability issues.

### Peer Scoring Metrics

#### `scores`
**Full name**: `malachitebft_sync_scores`

- **Type**: Histogram (with labels)
- **Unit**: Score value
- **Buckets**: Linear buckets (0.0 to 1.0, 0.05 increments, 20 buckets)
- **Labels**: `peer_id` (PeerId)
- **Description**: Tracks the score assigned to each peer based on their behavior and reliability. Used for peer selection and reputation management.

---

## Notes

### Bucket Configurations

- **Linear buckets**: Used for metrics with expected linear distributions (e.g., time measurements with relatively consistent ranges)
- **Exponential buckets**: Used for metrics with potentially wide-ranging values or where small values are common but large outliers are important (e.g., latency measurements, cryptographic operations)

### Internal State

Some metrics maintain internal state for measurement purposes:
- `instant_consensus_started`: Tracks when consensus started for calculating `consensus_time`
- `instant_block_started`: Tracks when block processing started for calculating `time_per_block`
- `instant_step_started`: Tracks when each step started for calculating `time_per_step`
- `instant_request_sent`: Maps height to request send time for calculating `value_client_latency`
- `instant_request_received`: Maps height to request receive time for calculating `value_server_latency`
