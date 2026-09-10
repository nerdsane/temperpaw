# Effort resource delivery

These three read-only gates connect an Effort to up to eight exact operations on typed resources. They make no provider requests and dispatch no actions.

`ConfigureResourceDelivery` stores the plan and tested head. The validation gate reads the linked resource permissions, attached proof packets and their File artifacts. `MergeResourceDelivery` reuses the existing recorded review and proof validators. After merge, the agent invokes each resource action. `VerifyResourceDelivery` checks the durable resource results before the Effort becomes Verified.

Every result carries the captured plan bytes, head and delivery sequence. Reconfiguration or another verification attempt invalidates older callbacks. A failed operation that was acknowledged back to Active still lacks the required shared and action-specific verification flags. The existing TemperPaw image deployment path remains separate and available.

Each plan entry has `entity_type`, `resource_id`, `action`, `operation_key`, `operation_sequence`, `revision`, `configuration_sha256` and `proof_ref`. Resource types, sets, supported actions and verification flags come from the generated DSF module contract. Each resource can occur once because it retains the current operation result.

Run the shared crate tests and `audit_callers.py`. Native actor, HTTP and compiled-WASM coverage is in `crates/temperpaw/tests/effort_resource_delivery.rs`. Its GitHub boundary uses the real gate with recorded HTTP responses; it does not claim an external GitHub request or provider deployment.

Build wrappers with their default Cargo target directories. If sharing a target directory for these wrappers, keep standalone `chain_*` entrypoint builds in their own targets: the wrappers depend on those crates in library mode.
