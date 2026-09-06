# Decisions and tradeoffs

## D1: Keep the operational model in a separate DSF factory app

**Decision:** Add `os-apps/dsf-factory` and extend existing SDLC deployment linkage where required.

**Came up because:** DSF resources and observations outlive individual Efforts, while the user explicitly excluded migrating DSF product logic into Temper.

**Options:** Put operational records on each Effort. migrate DSF product entities. use a separate operational app linked to existing Efforts.

**Chose the operational app because:** It preserves stable resource identity and existing product implementation. It adds one published app and explicit cross-app dependencies.

**Where:** docs/efforts/ARN-467/spec.md, Operational records and Resource operations and delivery.

## D2: Preserve Foundry's run system and Temper's work records

**Decision:** Link Foundry runs to Temper Efforts and render existing Asks through a narrow integration.

**Came up because:** Foundry already provides direct computer chat, durable commands and transcripts, but currently declines MCP elicitation and overwrites harness configuration during bootstrap.

**Options:** Replace Foundry orchestration. duplicate Effort/Ask records in Foundry. retain each system's existing records and add explicit linkage and request/reply handling.

**Chose explicit linkage because:** It reuses working computer/chat behavior and gives both interfaces one authoritative decision record. The integration must handle synchronous elicitation and recovery deliberately.

**Where:** docs/efforts/ARN-467/spec.md, Foundry and agent access. private fork arni-labs/foundry at 6ca87a793df711c79e609111560ee0c7491b0c1b.

## D3: Use subscriptions for agents and bound additional spend

**Decision:** Use existing agent subscriptions without automatic API-key fallback and cap additional overnight costs at $200.

**Came up because:** Rita approved $200 and clarified that agents should run on subscriptions. DSF product verification can independently call metered APIs.

**Options:** Allow metered agent fallback. stop all paid product verification. keep agent auth subscription-only and account for necessary product API calls plus hosting/compute under the cap.

**Chose the third option because:** It matches the user's authorization while permitting real application proof. Unavailable subscription consent remains a concrete blocker. It does not authorize a different billing mode.

**Where:** docs/efforts/ARN-467/intent.md and ARN-467 authorization record.

## D4: Use separate storage for experiments

**Decision:** Isolated experiments use a disposable database and a separate media bucket.

**Came up because:** The current staging service uses production data and media, and a prefix in that bucket would not meet the declared isolation contract.

**Options:** Reuse staging. restrict a prefix in the production bucket. provision separate disposable data and storage.

**Chose separate storage because:** Provider identity gives a direct isolation check and cleanup target. It requires a small additional resource whose cost counts against the cap.

**Where:** docs/efforts/ARN-467/spec.md, Isolated exploration.

## D5: Start with Codex subscription auth in Foundry

**Decision:** Use Codex with the existing ChatGPT subscription for the first factory harness.

**Came up because:** Current Foundry supports ChatGPT subscription credentials, while its Claude adapter currently requires a metered API key. The existing arni-big computer has a Codex auth file whose validity still needs a real probe.

**Options:** Add Claude subscription support first. use metered model keys. verify and connect existing Codex subscription auth.

**Chose Codex because:** It uses Foundry's implemented subscription path and the user's intended billing mode. Unavailable auth remains visible and cannot trigger a paid fallback.

**Where:** docs/efforts/ARN-467/spec.md, Foundry and agent access.

## D6: Fix the shared action boundary before installing the factory

**Decision:** Add strict IOA parameter contracts in the Temper kernel and pin that version before installing DSF factory specifications.

**Came up because:** Real actor and HTTP tests showed undeclared action inputs and generic writes could change protected operational fields.

**Options:** Depend on agent instructions; add DSF-specific endpoint checks; enforce the specification in the shared actor boundary.

**Chose the shared boundary because:** It protects the same contract through MCP, HTTP, reactions and simulation. It adds a kernel dependency that must be reviewed and deployed before app installation.

**Where:** nerdsane/temper branch codex/dsf-factory-boundaries, docs/efforts/ARN-467/spec.md.

## D7: Put behavior on typed resources

**Decision:** Replace the generic resource and operation dispatcher with resource-specific Temper contracts whose actions own configuration, deployment, observation and rollback sequences.

**Came up because:** The user clarified that the production Railway API must be a resource with its own configuration, telemetry and deploy/rollback behavior. The current branch's generic provider switch did not express that model.

**Options:** Keep `DsfResource` and `DsfOperation` with a shared executor. Split only the executor WASMs. Define resource-specific contracts and attach narrow provider integrations to their actions.

**Chose resource-specific contracts because:** The graph exposes what each resource can do and how it changes. Production and staging can reuse a type and its integrations without sharing identity. This requires replacing the uninstalled generic contracts and updating their callers and tests.

**Where:** docs/efforts/ARN-467/spec.md, Operational records and Resource operations and delivery; docs/efforts/ARN-467/plan.md, steps 2 and 4.

## D8: Use unique app-specific names and provider-scoped identities

**Decision:** Prefix new DSF entity types and sets with `Dsf`, validate their ownership before publication, and derive resource instance IDs from full provider scope.

**Came up because:** The user reminded us that Temper cannot distinguish different entity types with the same name. The registry strips CSDL namespaces, and the live default tenant already contains `DsfDeploy`.

**Options:** Rely on app or CSDL namespace separation. Add kernel namespace support. Use accurate app-specific names with collision checks against existing definitions and installed ownership.

**Chose explicit names and checks because:** They work with the current registry without a kernel namespace change. Names become longer, but the modeled object and owning app remain clear. A matching prefix alone never authorizes replacing an installed type.

**Where:** docs/efforts/ARN-467/spec.md, Names and identities and invariants 11–12; docs/efforts/ARN-467/plan.md, step 2. Evidence: Temper registry entity-set mapping and relation target lookup, plus production `temper.specs('default')` on 2026-09-06.
