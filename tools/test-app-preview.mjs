import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';

const source=readFileSync(new URL('../ui/app.js',import.meta.url),'utf8');
const functionSource=source.slice(source.indexOf('async function loadPreview('),source.indexOf('$("#create").addEventListener'));
const deferred=()=>{let resolve;const promise=new Promise(yes=>{resolve=yes;});return {promise,resolve};};
function harness(invoke,apply=async()=>true){
  const panel={},revoked=[],applied=[];let serial=0,cleared=0;
  const env={invoke,previewGeneration:0,previewUrls:['blob:old'],
    $:()=>panel,requireCharacter:()=>({characterId:'current'}),framing:()=>({}),
    bytesUrl:()=>`blob:new-${++serial}`,URL:{revokeObjectURL:url=>revoked.push(url)},
    renderer:{clear(){cleared++;},async applyState(state){applied.push(state);return apply(state);}}};
  vm.createContext(env);vm.runInContext(functionSource,env);
  return {env,panel,revoked,applied,cleared:()=>cleared};
}
const assets={rig:[1],parts:{face:[2]}};
const config={avatar:{}};

test('取得拒否では表示中の素材とURLを保持する',async()=>{
  const h=harness(async()=>{throw new Error('生成中');});
  await assert.rejects(h.env.loadPreview(),/生成中/);
  assert.equal(h.cleared(),0);assert.equal(h.applied.length,0);
  assert.deepEqual(h.revoked,[]);assert.deepEqual(h.env.previewUrls,['blob:old']);
  assert.equal(h.panel.hidden,true);
});

test('遅れて返った旧要求は新しいプレビューを置き換えない',async()=>{
  const old=deferred();let count=0;
  const h=harness(async name=>name==='get_config'?config:(++count===1?old.promise:assets));
  const first=h.env.loadPreview();
  await h.env.loadPreview();
  old.resolve(assets);await first;
  assert.equal(h.applied.length,1);assert.equal(h.cleared(),0);
  assert.deepEqual(h.revoked,['blob:old']);assert.equal(h.env.previewUrls.length,2);
});

test('新素材の検証失敗では新URLだけを解放する',async()=>{
  const h=harness(async name=>name==='get_config'?config:assets,async()=>{throw new Error('寸法不正');});
  await assert.rejects(h.env.loadPreview(),/寸法不正/);
  assert.deepEqual(h.revoked,['blob:new-1','blob:new-2']);
  assert.deepEqual(h.env.previewUrls,['blob:old']);assert.equal(h.cleared(),0);
});
