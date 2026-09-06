# DSF model collector

`DsfModelSync.Refresh` calls this module. It reads the configured Temper File,
the bound DsfResource, and one provider endpoint. The callback records one
immutable DsfObservation through IOA. The observation's declared reaction
projects facts to the resource using its observed-sequence comparison.

The module starts no computers, models, agents, repairs or deployments. It does
not write entities or dispatch actions. Its native `Host` boundary supplies HTTP
and named secrets; the WASM build uses the existing Temper SDK.

## Source configuration

`source_config_ref` is a Temper File ID. Its `$value` is JSON:

```json
{
  "provider_id": "service-id",
  "secret_name": "railway_token",
  "interval_seconds": 300,
  "source": {
    "provider": "railway",
    "environment_id": "environment-id"
  }
}
```

`interval_seconds` must be 60–86400. Unknown fields are rejected. `source_kind`
and `DsfResource.provider` must equal the selected provider name, and the
resource's `provider_id` must equal this File's `provider_id`. Configuration
supplies secret names, never credential values. Agents must treat changing these
bindings as a configuration decision, not evidence from the provider.

| Provider | Required fields inside `source` | `provider_id` |
| --- | --- | --- |
| `railway` | `environment_id` | Railway service ID |
| `vercel` | `team_id`, `target: "production"` | Vercel project ID |
| `supabase` | none | Supabase project ref |
| `cloudflare_r2` | `account_id` | R2 bucket name |
| `datadog` | `site`, `app_key_secret`, `query`, `window_seconds`, `max_age_seconds` | ID of the separate telemetry resource |
| `github` | `owner`, `repository`, `git_ref` | Bound repository identity |
| `dsf_operations` | `service`, `environment`, `max_age_seconds` | DSF service identity |

Datadog windows are 60–3600 seconds; maximum age is positive and no larger than
the window. Its query is limited to 2048 characters. Sites are fixed in the
module. It records up to 100 series, with the exact latest numeric point and
point count for each. Numeric zero is measured data; null or no points is absent
data. Old points are stale. These metric counts are not sampled trace counts.

The DSF endpoint is fixed to
`https://api.deep-sci-fi.world/api/operations/snapshot?participant_limit=200&job_limit=20`.
Production configuration expects service `deep-sci-fi-backend` and environment
`production`. The named secret is a registered admin API key, sent as Bearer
authentication. The parser requires `snapshot_version: 1`, checks service and
environment, and retains bounded participant and job references. It exposes
`participants.next_cursor`, `participant_inventory_complete`, and each queue's
`has_more`. When pagination remains, an agent must continue the inventory; the
first page is not the full user model. Only subsequent declared model actions
create or update DsfParticipant records.

## Evidence and failures

`evidence_ref` is the actual source URL. `query`, `window_start`, `window_end`,
`observed_at_ms`, and the redacted JSON `summary` explain the result without
requiring a mutable provider response to remain available. Credentials, product
content, environment values, Git commit messages and author emails are omitted.
A callback's observation ID is the ModelSync ID plus its incremented sequence.

Railway reads the latest deployment only in the pinned environment. Its
`meta.commitHash` and Vercel's deployment `meta.githubCommitSha` or `gitSource.sha`
are provider commit records. An absent commit remains unknown. Datadog uses a
separate resource and supplies no deployment revision. The DSF snapshot's
runtime revision also belongs to a separate runtime resource.

Provider HTTP errors, inaccessible credentials, invalid JSON, identity mismatch,
and unexpected response shapes produce `CollectionInaccessible`; no last known
configuration is overwritten. HTTP 404 does not prove a resource is absent.
Successful empty provider queries or missing production targets can produce
`CollectionAbsent`. A DSF schema mismatch or pending queue is recorded without
inferring a service outage. All response bodies are limited to 1 MiB; the source
configuration is limited to 16 KiB. The host enforces invocation time and memory
limits. IOA owns the retry limit and next refresh time.

Source Files cannot choose URLs, HTTP methods or headers. The module constructs
fixed provider endpoints and performs only reads (Railway uses a GraphQL query).
Production transport and Cedar policy must enforce outbound access, including
redirect handling; the SDK does not expose a per-call redirect setting.

## Build and test

```sh
cargo test --manifest-path os-apps/dsf-factory/wasm/dsf_model_collect/Cargo.toml
cargo clippy --manifest-path os-apps/dsf-factory/wasm/dsf_model_collect/Cargo.toml --all-targets -- -D warnings
os-apps/dsf-factory/wasm/dsf_model_collect/build.sh
```

Native tests exercise the HTTP boundary and provider parsers. A built module
must also run through Temper's WASM engine against the authorized provider
before that provider's collection path is considered verified live.

The HTTP shapes follow the providers' official references:
[Railway GraphQL](https://docs.railway.com/integrations/api),
[Vercel REST API](https://vercel.com/docs/rest-api),
[Supabase projects](https://supabase.com/docs/reference/api/v1-list-all-projects),
[Cloudflare R2 bucket](https://developers.cloudflare.com/api/resources/r2/subresources/buckets/methods/get/),
and [Datadog metrics](https://docs.datadoghq.com/api/latest/metrics/query-timeseries-points/).
The DSF response contract is `platform/backend/schemas/operations.py` in the
Deep Sci-fi repository.
