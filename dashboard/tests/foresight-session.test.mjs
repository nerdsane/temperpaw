import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
const source = readFileSync(new URL('../src/lib/foresight.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {parseWorld, loadResearchSession, researchSessionProblem} = await import('data:text/javascript;base64,' + Buffer.from(compiled).toString('base64'));

test('world research uses its recorded session ID, not names or prompt text', async () => {
  const world = parseWorld({Id:'world-1',Status:'Seeding',ResearchSessionId:'session-1'});
  const calls=[];
  const state = await loadResearchSession(world.researchSessionId, async (set,id) => {
    calls.push({set,id});
    return {Id:id,Status:'Failed',error_message:'Provider rejected the request',turn_count:'3'};
  });
  assert.deepEqual(calls,[{set:'Sessions',id:'session-1'}]);
  assert.equal(state.kind,'ready');
  assert.equal(state.id,'session-1');
  assert.equal(state.status,'Failed');
  assert.equal(state.turns,3);
  assert.match(researchSessionProblem(state,'Seeding'),/Provider rejected the request/);
});

test('unlinked worlds remain unknown without searching for matching text', async () => {
  let queried=false;
  const world=parseWorld({Id:'world-1',Status:'Seeding',name:'Surveyor-session-1'});
  const state=await loadResearchSession(world.researchSessionId,async()=>{queried=true;return {};});
  assert.deepEqual(state,{kind:'unlinked'});
  assert.equal(queried,false);
});

test('a denied, missing or mismatched session cannot look healthy', async () => {
  for (const fetch of [
    async()=>{throw new Error('OData get failed: 403');},
    async()=>({Id:'other-session',Status:'Completed'}),
    async()=>({Id:'session-1'}),
  ]) {
    const state=await loadResearchSession('session-1',fetch);
    assert.equal(state.kind,'unavailable');
    assert.equal(state.id,'session-1');
    assert.ok(researchSessionProblem(state,'Seeding'));
  }
});

test('failed and cancelled sessions stay visible even without error text', async () => {
  for (const status of ['Failed','Cancelled']) {
    const state=await loadResearchSession('session-1',async()=>({Id:'session-1',Status:status}));
    assert.match(researchSessionProblem(state,'Seeding'),new RegExp(status,'i'));
  }
});

test('a completed session does not imply the world finished seeding', async () => {
  const state=await loadResearchSession('session-1',async()=>({Id:'session-1',Status:'Completed'}));
  assert.match(researchSessionProblem(state,'Seeding'),/last (reported|recorded) research session completed/i);
  assert.equal(researchSessionProblem(state,'Active'),'');
});

test('running research retains actual progress without an invented failure', async () => {
  const state=await loadResearchSession('session-1',async()=>({Id:'session-1',Status:'Executing',turn_count:2,last_heartbeat_at:'2026-09-16T12:00:00Z'}));
  assert.equal(state.status,'Executing');
  assert.equal(state.heartbeat,'2026-09-16T12:00:00Z');
  assert.equal(researchSessionProblem(state,'Seeding'),'');
});
