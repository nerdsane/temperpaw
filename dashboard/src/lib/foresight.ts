/** Parse the Foresight API at the boundary. Missing measurements stay missing. */
type Row = Record<string, unknown>;
function field(row: Row, name: string): unknown {
  const key = Object.keys(row).find((key) => key.replaceAll('_', '').toLowerCase() === name.replaceAll('_', '').toLowerCase());
  return key ? row[key] : undefined;
}
function text(row: Row, name: string): string {
  const value = field(row, name);
  return typeof value === 'string' ? value : '';
}
function number(value: unknown): number | null {
  if ((typeof value !== 'number' && typeof value !== 'string') || (typeof value === 'string' && value.trim() === '')) return null;
  const result = Number(value);
  return Number.isFinite(result) ? result : null;
}
export function probability(value: unknown): number | null {
  const result = number(value);
  return result !== null && result >= 0 && result <= 1 ? result : null;
}
function record(value: unknown): Row | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Row : null;
}
function json(value: string): unknown { return JSON.parse(value); }
function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}
function identity(row: Row) {
  return { id: text(row, 'Id') || text(row, '_entity_id'), status: text(row, 'Status') };
}
export interface PredictiveModel { version: string; slope: number; intercept: number }
function parseModel(value: string): PredictiveModel | null {
  if (!value) return null;
  try {
    const model = record(json(value));
    if (!model) return null;
    const slope = number(model.slope), intercept = number(model.intercept);
    return typeof model.version === 'string' && slope !== null && intercept !== null ? { version: model.version, slope, intercept } : null;
  } catch { return null; }
}
export function parseWorld(row: Row) {
  return {
    ...identity(row), title: text(row, 'name') || text(row, 'domain') || text(row, 'Id'),
    domain: text(row, 'domain'), description: text(row, 'description'), target: text(row, 'target_date'), frontier: text(row, 'frontier_date'),
    axes: text(row, 'uncertainty_axes'), mode: text(row, 'learning_mode') || 'observed',
    model: parseModel(text(row, 'model_json')), adoptedRunId: text(row, 'adopted_learning_run_id'),
    error: text(row, 'error_message'), corpusId: text(row, 'corpus_file_id'),
  };
}
export type World = ReturnType<typeof parseWorld>;
export function parseForecast(row: Row) {
  return {
    ...identity(row), error:text(row,'error_message'), eventId: text(row, 'event_node_id'), worldId: text(row, 'world_id'),
    question: text(row, 'question'), probability: probability(field(row, 'probability')),
    baseProbability: probability(field(row, 'base_probability')), registered: text(row, 'registered_at'),
    deadline: text(row, 'resolve_by'), outcome: text(row, 'outcome'), resolved: text(row, 'resolved_at'),
    score: probability(field(row, 'brier')), sources: text(row, 'outcome_source_refs'),
    model: text(row, 'model_version') || text(row, 'engine_version'),
    learningRunId: text(row, 'learning_run_id'), previousId: text(row, 'previous_forecast_id'),
    evidence: text(row, 'evidence_kind'), outcomeEvidence: text(row, 'outcome_evidence_kind'), marketRef: text(row, 'market_ref'),
  };
}
export type Forecast = ReturnType<typeof parseForecast>;
export function forecastGroups(forecasts: Forecast[]) {
  const groups = new Map<string, Forecast[]>();
  for (const forecast of forecasts) {
    const key = forecast.eventId || forecast.id;
    groups.set(key, [...(groups.get(key) ?? []), forecast]);
  }
  return Array.from(groups, ([eventId, revisions]) => {
    revisions.sort((a, b) => b.registered.localeCompare(a.registered) || b.id.localeCompare(a.id));
    return { eventId, latest: revisions[0], revisions };
  }).sort((a, b) => b.latest.registered.localeCompare(a.latest.registered));
}
export function parseEvent(row: Row) {
  return { ...identity(row), statement: text(row, 'statement'), layer: text(row, 'layer'), date: text(row, 'resolve_by'),
    probability: probability(field(row, 'probability')), provenance: text(row, 'provenance'), sources: text(row, 'source_refs'),
    edges: text(row, 'edges'), resolution: text(row, 'resolution') };
}
export function parseEndpoint(row: Row) {
  return { ...identity(row), summary: text(row, 'summary'), assumptions: text(row, 'driver_config'),
    weight: probability(field(row, 'weight')), error: text(row, 'error_message'), discardReason: text(row, 'discard_reason') };
}
export function parsePath(row: Row) {
  return { ...identity(row), endpointId: text(row, 'endpoint_id'), classification: text(row, 'classification'),
    note: text(row, 'classification_note'), cost: number(field(row, 'repair_cost')), nodes: text(row, 'required_node_ids'),
    error: text(row, 'error_message') };
}
export function parseClaim(row: Row) {
  return { ...identity(row), endpointId: text(row, 'endpoint_id'), statement: text(row, 'current_text') || text(row, 'original_text'),
    classification: text(row, 'classification'), reason: text(row, 'unreachable_reason') };
}
export interface LearningReport {
  decision: string; reason: string; mode: string; asOf: string;
  trainingCount: number | null; validationCount: number | null; skippedCount: number | null;
  incumbentBrier: number | null; candidateBrier: number | null; improvement: number | null;
  trainingIds: string[]; validationIds: string[]; limitations: string[];
  changes: { name: string; before: number | null; after: number | null; meaning: string }[];
  provenance: string; skipped: string;
}
function parseReport(value: string): LearningReport | null {
  if (!value) return null;
  const r = record(json(value));
  if (!r || r.schema_version !== 1 || typeof r.decision !== 'string' || typeof r.reason !== 'string') throw new Error('Unsupported learning report');
  const changes = Array.isArray(r.learned_changes) ? r.learned_changes.flatMap((value) => {
    const c = record(value);
    return c && typeof c.name === 'string' && typeof c.meaning === 'string'
      ? [{name:c.name, before:number(c.before), after:number(c.after), meaning:c.meaning}] : [];
  }) : [];
  return {
    decision:r.decision, reason:r.reason, mode:typeof r.mode === 'string' ? r.mode : '', asOf:typeof r.as_of === 'string' ? r.as_of : '',
    trainingCount:number(r.training_count), validationCount:number(r.validation_count), skippedCount:number(r.skipped_count),
    incumbentBrier:probability(r.incumbent_brier), candidateBrier:probability(r.candidate_brier), improvement:number(r.improvement),
    trainingIds:strings(r.training_ids), validationIds:strings(r.validation_ids), limitations:strings(r.evaluation_limitations),
    changes, provenance:typeof r.provenance_summary === 'string' ? r.provenance_summary : JSON.stringify(r.provenance_summary ?? {}),
    skipped:JSON.stringify(r.skipped_reasons ?? {}),
  };
}
export function parseLearningRun(row: Row) {
  let report: LearningReport | null = null, reportError = '';
  try { report = parseReport(text(row, 'report_json')); } catch { reportError = 'The learning report could not be read. Open the record to inspect the stored data.'; }
  return { ...identity(row), worldId:text(row,'world_id'), mode:text(row,'mode'), asOf:text(row,'as_of'),
    error:text(row,'error_message'), report, reportError, candidate:parseModel(text(row,'candidate_model_json')),
    incumbent:parseModel(text(row,'incumbent_model_json')) };
}
export type LearningRun = ReturnType<typeof parseLearningRun>;
export function sourceLinks(value: string): string[] {
  let candidates = [value];
  try { candidates = strings(json(value)); } catch { /* A single URL is also a valid source reference. */ }
  return candidates.filter((candidate) => { try { const u = new URL(candidate); return u.protocol === 'https:' || u.protocol === 'http:'; } catch { return false; } });
}
export function parseDataset(value: string): string {
  if (!value.trim()) return '';
  let parsed: unknown;
  try { parsed = json(value); } catch { throw new Error('Dataset must be valid JSON.'); }
  if (!Array.isArray(parsed)) throw new Error('Dataset must be a JSON array.');
  if (parsed.length > 512) throw new Error('Dataset is limited to 512 experiences.');
  if (value.length > 1_000_000) throw new Error('Dataset exceeds the 1 MB limit.');
  return JSON.stringify(parsed);
}
export function percent(value: number | null): string { return value === null ? 'Unmeasured' : `${(value * 100).toFixed(1)}%`; }
export function measure(value: number | null): string { return value === null ? '—' : value.toFixed(4); }

/** Convert UI date inputs to the backend's fixed UTC-second action format. */
export function utcTime(value: string): string {
  const parsed = new Date(value.endsWith('Z') ? value : value + 'Z');
  if (!Number.isFinite(parsed.getTime())) throw new Error('Enter a valid UTC date and time.');
  return parsed.toISOString().slice(0, 19) + 'Z';
}

export interface ForesightLocation { worldId: string; asOf: string; tab: string }
export function readForesightLocation(href: string): ForesightLocation {
  const params = new URL(href).searchParams;
  const at = params.get('at') ?? '';
  const date = new Date(at + 'Z');
  const asOf = Number.isFinite(date.getTime()) && date.toISOString().slice(0, 16) === at ? at : '';
  const view = params.get('view') ?? '';
  return { worldId: params.get('world') ?? '', asOf,
    tab: ['world','predictions','learning','activity'].includes(view) ? view : 'world' };
}
export function writeForesightLocation(href: string, state: ForesightLocation): string {
  const url = new URL(href);
  for (const [key, value] of Object.entries({world:state.worldId, at:state.asOf, view:state.tab})) {
    if (value) url.searchParams.set(key, value);
    else url.searchParams.delete(key);
  }
  return url.href;
}
