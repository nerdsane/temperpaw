# ADR-013: Question-driven semantic exploration in the existing Foresight app

Status: implementation in progress.

The user wants the existing Deep Sci-Fi Foresight UI and native Temper app rebuilt around fast composable semantic functions. The separate offline viewer is insufficient.

World.RequestSemanticExploration records the request before research. SeedComplete then starts a SemanticRun. A native reasoning Session proposes four causal axes with three options each against the world's frozen evidence. A deterministic WASM expands the 81 combinations. Each event's actual requires edges are traversed with cycle, missing-reference and depth accounting. Shared prerequisites are assessed once. Each Jev classify_gap and dependent choose_next_operation is an individual Calling/Recorded transition. Assessments remain judgments, never forecast probabilities or observed truth.

After broad evaluation, a bounded reasoning Session deepens up to eight scenarios that Jev actually recommended repairing, selected to cover different causal options. If no repair is recommended or the shared call/time budget is exhausted, the declared workflow proceeds directly to synthesis. Unchanged prerequisite assessments are reused. A final reasoning Session synthesizes competing worlds, mechanisms, early signals and falsifiers. Sessions are spawned by declared effects and observed through bounded scheduled actions. The browser never orchestrates the loop. Provider credentials stay on Temper.

The existing /worlds and /world/[id] UI displays actual durable run state in a dense purple/teal matrix, graph and decision inspector. Existing immutable forecasts and learning stay separate from speculative branch assessments. Candidate count does not establish independent useful hypotheses or forecast accuracy.

Limits: 256 graph nodes, 8 recursion levels, 320 semantic calls across the entire run, 600-second shared semantic admission budget (including time spent deepening), three reasoning phases, at most eight deepened scenarios. Unknown and incomplete work stays visible. No provider retries silently duplicate progress.


## Verification and activation status (2026-09-18)

The six module builds and 32 Rust unit cases pass. The installed SemanticRun specification passes all four verification levels. The original UI passes 183 unit cases, TypeScript, its production build, and browser checks for matrix/graph switching, focus, replay isolation, and 390px layout with no page errors. The dense browser fixture is explicitly simulated and is not provider evidence.

The first live native run `en-01a0b597-ea7c-73e1-918b-2a4f06eccc90` moved Created → Preparing → Failed. Its recorded error was `authorization denied for http_call: no matching permit policy`. The new scoped policy declarations must be installed through the governed app release before retrying. No substitute principal or direct provider loop was used to bypass this denial.

The source remains an implementation candidate: live provider completion, World entry wiring, full app policy installation, fair full-pipeline comparison, and connected preview verification are outstanding. Do not treat simulated UI counts or the formal state-machine checks as a successful live run.
