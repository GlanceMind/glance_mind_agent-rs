# Agent Harness

`glance_mind_agent_rs` owns agent orchestration and provider-facing ports.
Harness changes should use the existing ports/mock design as the first control
plane.

Use:

- `cargo test mock_gateway`
- `cargo test orchestrator`

Provider behavior should be represented as success, not found, rate limit,
auth, or network boundaries before a full-stack scenario is used.
