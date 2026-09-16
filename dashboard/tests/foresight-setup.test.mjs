import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
const source=readFileSync(new URL('../src/lib/setup-routing.ts',import.meta.url),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText;
const {requiresAgentSetup}=await import('data:text/javascript;base64,'+Buffer.from(compiled).toString('base64'));
const empty={has_anthropic_key:false,has_agents:false,has_personalized_soul:false};

test('authenticated Foresight replay is reachable without unrelated agent/provider setup',()=>{
  assert.equal(requiresAgentSetup('/foresight',empty),false);
});
test('other agent pages still require setup and welcome never redirects to itself',()=>{
  for(const path of ['/','/sessions','/agents','/settings']) assert.equal(requiresAgentSetup(path,empty),true);
  assert.equal(requiresAgentSetup('/welcome',empty),false);
  assert.equal(requiresAgentSetup('/sessions',{has_anthropic_key:true,has_agents:true,has_personalized_soul:true}),false);
});
