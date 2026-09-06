# DSF operation integrations

Four WASM modules implement the boundaries declared in `operation.ioa.toml`. They do not dispatch Temper actions. The IOA schedules retries, applies callbacks and releases the resource after a confirmed terminal result.

| Module | Work | Result |
| --- | --- | --- |
| `dsf_operation_validate` | Read the linked resource, Effort, Asks, ProofPacket and proof File | Validated, blocked by an existing Ask, or rejected |
| `dsf_operation_execute` | Repeat validation, look for an existing provider receipt, then submit one operation when permitted | Provider execution ID or uncertain outcome |
| `dsf_operation_observe` | Read provider state using the pinned target and correlation | Found receipt, proven media-receipt absence, or unresolved outcome |
| `dsf_operation_verify` | Read provider completion, revision health, selected flow and Datadog evidence | Verified, pending, or confirmed failure |

## Configuration

`DsfOperation.intended_configuration` contains a JSON string with `config_ref` and `config_sha256`. The reference names a Temper File whose `$value` is the exact UTF-8 configuration. The digest is lowercase SHA-256 of those bytes. Every module rereads and hashes the File. Changing it after requesting the operation invalidates the operation.

The configuration is version 1 and rejects unknown fields:

```json
{
  "version": 1,
  "resource_id": "resource-id",
  "effort_id": "effort-id",
  "operation_key": "operation-uuid",
  "revision": "full-lowercase-git-commit-sha",
  "required_ask_ids": [],
  "max_cost_cents": 0,
  "not_before_ms": 1788652800000,
  "provider": {
    "kind": "railway",
    "project_id": "project-id",
    "service_id": "service-id",
    "environment_id": "environment-id",
    "secret_name": "railway_token",
    "baseline_deployment_id": "current-deployment-id"
  },
  "flow": {
    "kind": "story",
    "story_id": "published-story-id",
    "world_id": "world-id"
  },
  "datadog": {
    "site": "datadoghq.com",
    "service": "deep-sci-fi-backend",
    "environment": "production",
    "api_key_secret": "datadog_api_key",
    "app_key_secret": "datadog_app_key"
  }
}
```

The IDs in this example are placeholders. Supply real provider IDs and a 40- or 64-character revision. `operation_key` must equal the canonical DsfOperation ID. `not_before_ms` bounds provider correlation and telemetry search; it must not be in the future beyond the five-second clock allowance. The invocation provides `temper_api_url` and `temper_api_key` through integration configuration. Provider credentials are named secrets, never configuration values.

Other provider variants:

- `vercel`: `project_id`, `team_id`, `project_name`, numeric `git_repository_id`, `secret_name`.
- `media`: `secret_name`, plus 1–20 unique `generations`. Each generation pins `id`, `target_type`, `target_id`, `media_type` and `max_cost_cents`. Its flow must be `{"kind":"media"}`.

The `operational_snapshot` flow pins `schema_version` and `secret_name`. It checks the authenticated DSF snapshot's version, migration state and runtime revision. The `story` flow reads one selected published story and checks its world, content and status. Select a flow that exercises the change; a story read does not prove a publishing workflow.

## Existing authority and proof

Routine deployment needs no new Ask. The linked Effort must be Merged, Deploying or Verified at the intended revision, with its existing proof and review gates satisfied. The resource must own this operation and allow its kind. The attached ProofPacket must pass the existing `chain_proof_ready::proof_packet_holds` validator; its Ready artifact File must also be readable.

Open blocking Asks stop validation. `required_ask_ids` defaults to empty. When configured, each required Ask must be linked to the same Effort and answered without an explicit denial. This check establishes that a recorded choice is resolved; it does not interpret arbitrary prose as a new grant of authority. Cedar and the Effort's recorded authorization remain the governing boundaries.

Deployment modules use existing service allocations and require `max_cost_cents: 0`. Media repairs check exact selected jobs, per-job ceilings and the image cost estimate and the video cost at its exact configured duration before submission, then require recorded actual costs within those ceilings before verification. This is not a global spending ledger or a provider-enforced monetary cap. New paid provisioning belongs to the separately governed experiment path.

## Provider correlation and uncertainty

Railway deploys the exact commit with `serviceInstanceDeployV2`. Before sending, it checks that the latest deployment still matches the pinned baseline. Reconciliation reads at most 50 deployments within the exact project, service and environment, matching the returned execution ID or a unique new deployment at the requested SHA after `not_before_ms`. Railway does not expose an idempotency token for this mutation. A missing or ambiguous result remains unresolved; a second execution attempt cannot resend the deployment.

Vercel creates a production deployment from the pinned Git repository and SHA with operation and Effort metadata. Reconciliation reads at most 20 candidates and checks each matching deployment's project, production target, SHA and operation metadata. Ambiguous results are not retried as new deployments.

Media repair uses the DSF durable receipt API. The POST carries the operation UUID and exact selected IDs. Reconciliation reads `/api/media/recovery-operations/{operation_id}` and checks the receipt's endpoint and selected IDs. A 404 permits replay of the same idempotent request. Partial claims remain pending while claimed jobs run, then become a failed operation. A failed selected job cannot release the resource while another selected job still runs.

## Verification and bounds

Health must return `status: healthy` and the exact `git_sha`. The frontend origin is `https://deep-sci-fi.world`; the backend origin is `https://api.deep-sci-fi.world`. Probes carry the operation UUID in `X-Request-ID`. Media verification also checks the exact attempt, selected target, completed artifact accessibility and recorded cost.

Datadog must return a non-error sampled span for the configured service and environment with `attributes.custom.git.commit.sha` and `attributes.custom.dsf.request_id` matching the operation. The resulting trace URL is evidence. Empty results, inaccessible APIs and delayed indexing remain pending; sampled span results are not request counts. Each query reads at most 20 spans from the last 30 minutes, bounded by the operation start. A later retry can observe a preceding probe after indexing.

The IOA allows 40 verification attempts and 20 observation attempts. Verification timeouts return to Observed and keep the resource lock. Exhausted verification stays unresolved with the lock held; it does not imply failure or permission to resend. HTTP response bodies are capped at 1 MiB; configuration Files at 32 KiB. Provider destinations are fixed in code. The Temper transport must enforce redirect policy before credentials cross an HTTP redirect.

## Checks

Run the common crate's native tests and clippy. Each wrapper's `build.sh` builds its standalone WASM artifact. `crates/temperpaw/tests/dsf_factory_contract.rs` runs the actual Temper actor simulator and builds/invokes all four modules through `WasmEngine` and `ProductionWasmHost` with controlled provider responses. Those engine tests prove the ABI and boundary decisions; live provider execution and deployment evidence are separate required checks.
