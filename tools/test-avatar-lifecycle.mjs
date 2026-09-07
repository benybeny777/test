import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
import {consumeSnapshot} from '../ui/shared/snapshot-client.js';
import * as THREE from '../ui/shared/vendor/three/three.module.min.js';
import {planSceneBatches,sceneBatchBox,createNativeSceneBatch} from '../ui/shared/native-scene-batch.js';

const source=name=>readFileSync(new URL('../ui/'+name,import.meta.url),'utf8').replace(/^import .*;\r?\n/gm,'').replace(/export /g,'');
const checkSource=()=>source('check.js').replace('const fixtures=[];',"const fixtures=[['c_2700e1166676','女性A'],['c_190454c86edb','むぎ']];");
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};};
function rig(){
  const names=['scene_face','scene_neck','scene_hair','scene_torso','scene_left_arm','scene_right_arm'];
  return {schema_version:3,eye_rig_version:3,lip_rig_version:1,scene_graph_version:1,
    canvas:{width:1,height:1},scene_graph:names.map(layer=>({layer,role:layer.slice(6)})),
    layers:Object.fromEntries([...names,'face','left_arm','right_arm','mouth_open','mouth_closed',
      ...['left','right'].flatMap(side=>['eye_iris','eye_backplate','eye_base','eyelid_upper'].map(part=>side+'_'+part))]
      .map(name=>[name,{bbox:[0,0,1,1],texture_box:[0,0,1,1],url:name}]))};
}
function harness(){
  let frame,fail=false,rendered=0,cleared=0;
  const errors=[];
  const context={save(){},restore(){},clearRect(){},translate(){},drawImage(){},putImageData(){},
    getImageData(){return {data:new Uint8ClampedArray([0,0,0,255])};}};
  class Renderer{
    setClearColor(){}setPixelRatio(){}setSize(){}dispose(){}forceContextLoss(){}
    clear(){cleared++;}render(){if(fail)throw new Error('描画故障');rendered++;}
  }
  const env={THREE:{...THREE,WebGLRenderer:Renderer},devicePixelRatio:1,performance:{now:()=>0},
    document:{createElement:()=>({getContext:()=>context})},ResizeObserver:class{observe(){}disconnect(){}},
    Image:class{naturalWidth=1;naturalHeight=1;async decode(){}},
    requestAnimationFrame:callback=>{frame=callback;return 1;},cancelAnimationFrame:()=>{frame=undefined;},
    loadLocalJson:async()=>rig(),localAssetUrl:url=>({href:url}),lipMesh(){},eyeAperture(){},
    validateHiddenMotion(){},hiddenRepairAmount:()=>0,drawBlink(){},drawTexturedMouth(){},
    planSceneBatches,sceneBatchBox,
    createNativeSceneBatch:(parts,layers,images)=>createNativeSceneBatch(parts,layers,images,()=>({getContext:()=>context})),
    bleedTransparentRgb:data=>data,headDisplacement:()=>[0,0],armDisplacement:()=>[0,0],MOUTH_PRESETS:{close:[0,0]}};
  const api=vm.runInNewContext(source('shared/avatar-renderer.js')+'\n({createAvatarRenderer,validateRenderBounds});',env);
  const renderer=api.createAvatarRenderer({clientWidth:1,clientHeight:1},{onError:error=>errors.push(error.message)});
  return {api,renderer,env,errors,tick:()=>frame(0),fail:value=>{fail=value;},counts:()=>({rendered,cleared})};
}
test('通常確認画面は固定した旧比較候補を一覧へ混ぜない',()=>{
  const value=readFileSync(new URL('../ui/check.js',import.meta.url),'utf8');
  assert.match(value,/const fixtures=\[\];/);
  assert.doesNotMatch(value,/const fixtures=\[\['c_/);
});
test('描画必須の部位範囲を読み込み前に検証する',()=>{
  const h=harness();const value=rig();h.api.validateRenderBounds(value);
  delete value.layers.left_arm;assert.throws(()=>h.api.validateRenderBounds(value),/left_arm/);
  value.layers.left_arm={bbox:[0,0,NaN,1]};assert.throws(()=>h.api.validateRenderBounds(value),/left_arm/);
  h.renderer.dispose();
});
test('描画例外を通知し、正常な次のキャラで復帰する',async()=>{
  const h=harness();await h.renderer.applyState({rigUrl:'a'});
  h.fail(true);h.tick();assert.deepEqual(h.errors,['描画故障']);
  h.tick();assert.equal(h.errors.length,1);
  h.fail(false);await h.renderer.applyState({rigUrl:'b'});h.tick();
  assert.equal(h.counts().rendered,1);h.renderer.dispose();
  await assert.rejects(h.renderer.applyState({rigUrl:'c'}),/破棄済み/);
});
test('clearは完了待ちの旧キャラを無効にする',async()=>{
  const h=harness(),pending=deferred();h.env.loadLocalJson=()=>pending.promise;
  const old=h.renderer.applyState({rigUrl:'a'});h.renderer.clear();pending.resolve(rig());
  assert.equal(await old,false);h.tick();assert.equal(h.counts().rendered,0);h.renderer.dispose();
});
test('公開直前の世代照合失敗は現在の描画を解放しない',async()=>{
  const h=harness();await h.renderer.applyState({rigUrl:'a'});h.tick();
  await assert.rejects(h.renderer.applyState({rigUrl:'b'},{beforeCommit:async()=>{throw new Error('世代変更');}}),/世代変更/);
  h.tick();assert.equal(h.counts().rendered,2);assert.equal(h.counts().cleared,0);h.renderer.dispose();
});
test('候補照合待ちの取消は旧表示を維持する',async()=>{
  const h=harness();await h.renderer.applyState({rigUrl:'a'});
  const wait=deferred(),entered=deferred();
  const candidate=h.renderer.applyState({rigUrl:'b'},{beforeCommit:()=>{entered.resolve();return wait.promise;}});
  await entered.promise;h.renderer.cancelPending();wait.resolve(true);
  assert.equal(await candidate,false);h.tick();assert.equal(h.counts().rendered,1);assert.equal(h.counts().cleared,0);h.renderer.dispose();
});
test('確認画面は旧ロード完了で次のキャラの待機・失敗画面を上書きしない',async()=>{
  const nodes=new Map();
  const node=selector=>{if(!nodes.has(selector))nodes.set(selector,{value:'',checked:true,style:{},textContent:'',
    add(){},append(){},addEventListener(){},removeAttribute(){}});return nodes.get(selector);};
  const old=deferred(),next=deferred();let applied;
  const entered=deferred();
  const env={document:{querySelector:node,querySelectorAll:()=>[],createElement:()=>node(Symbol())},
    Option:class{},URL,AbortController,fetch:async()=>new Response(null,{status:404}),location:{href:'http://localhost/ui/check.html'},addEventListener(){},
    cancelAnimationFrame(){},MOUTH_PRESETS:{close:[0,0]},
    createAvatarRenderer:()=>({clear(){},dispose(){},applyState(){entered.resolve();return old.promise;}}),
    loadLocalJson:async url=>url.includes('c_190454c86edb')?next.promise:
      url.includes('character.json')?{stages:{rig2d:{status:'complete',updatedAtIso:'now'}}}:rig()};
  const api=vm.runInNewContext(checkSource().replace('await initialize();','')+'\n({load});',env);
  node('#character').value='c_2700e1166676';applied=api.load();await entered.promise;
  node('#character').value='c_190454c86edb';const newer=api.load();
  old.resolve(true);await applied;
  assert.equal(node('#avatar').style.visibility,'hidden');assert.equal(node('#status').textContent,'読込中');
  next.reject(new Error('次のキャラの読込失敗'));await newer;
  assert.equal(node('#avatar').style.visibility,'hidden');assert.equal(node('#status').textContent,'次のキャラの読込失敗');
});
test('確認画面は次候補の失敗時に表示キャラと選択名を旧キャラへ戻す',async()=>{
  const nodes=new Map();const node=key=>{if(!nodes.has(key))nodes.set(key,{value:'',checked:true,style:{},textContent:'',add(){},append(){},addEventListener(){}});return nodes.get(key);};
  let failed=false,cleared=0,commits=0,lastUrl;
  const env={document:{querySelector:node,querySelectorAll:()=>[],createElement:()=>node(Symbol())},Option:class{},URL,AbortController,fetch:async()=>new Response(null,{status:404}),
    location:{href:'http://localhost/ui/check.html'},history:{replaceState(_a,_b,url){lastUrl=url;}},addEventListener(){},cancelAnimationFrame(){},MOUTH_PRESETS:{close:[0,0]},
    createAvatarRenderer:()=>({clear(){cleared++;},dispose(){},async applyState(_state,{beforeCommit}){if(await beforeCommit()===false)return false;commits++;return true;}}),
    loadLocalJson:async url=>{if(failed)throw new Error('候補取得失敗');return url.includes('character.json')?{stages:{rig2d:{status:'complete',updatedAtIso:'now'}}}:rig();}};
  const api=vm.runInNewContext(checkSource().replace('await initialize();','')+'\n({load});',env);
  node('#character').value='c_2700e1166676';await api.load();
  assert.equal(commits,1);assert.equal(node('#avatar').style.visibility,'visible');
  failed=true;node('#character').value='c_190454c86edb';await api.load();
  assert.equal(cleared,0);assert.equal(commits,1);assert.equal(node('#character').value,'c_2700e1166676');
  assert.equal(lastUrl.searchParams.get('character'),'c_2700e1166676');assert.match(node('#status').textContent,/女性A.*保持/);
});

test('確認画面の実snapshotデコーダーは欠落応答とAbortで旧Blobを保持し次commit後だけ解放する',async()=>{
  const limits={record_bytes:2048,chunk_bytes:512,part_bytes:8192,total_bytes:16384,parts:4,dimension:8};
  const nodes=new Map(),revoked=[],created=[],committed=[];
  const node=key=>{if(!nodes.has(key))nodes.set(key,{value:'',checked:true,style:{},textContent:'',add(){},append(){},addEventListener(){}});return nodes.get(key);};
  let mode='complete',serial=0,cancelled=0;const entered=deferred();
  function response(complete=true){
    const generation='g_'+'1'.repeat(32),rigBytes=Buffer.from(JSON.stringify({layers:{face:{url:'/assets/rig2d/parts/face.png',texture_box:[0,0,1,1]}}}));
    const png=Buffer.from('CPU配信境界fixture');const sha=data=>createHash('sha256').update(data).digest('hex');
    const records=[{type:'snapshot',schema_version:1,generation,rig_sha256:sha(rigBytes),parts:1,total:rigBytes.length+png.length}];
    for(const [name,data,mime,size] of [['rig',rigBytes,'application/json',null],['face',png,'image/png',[1,1]]])records.push(
      {type:'begin',name,mime,size,length:data.length},{type:'chunk',name,data:data.toString('base64')},{type:'end',name,sha256:sha(data)});
    if(complete)records.push({type:'complete',generation});
    const bytes=new TextEncoder().encode(records.map(row=>JSON.stringify(row)+'\n').join(''));
    return new Response(new ReadableStream({start(controller){for(let i=0;i<bytes.length;i+=7)controller.enqueue(bytes.slice(i,i+7));controller.close();}}));
  }
  const env={document:{querySelector:node,querySelectorAll:()=>[],createElement:()=>node(Symbol())},Option:class{},URL,AbortController,
    location:{href:'http://localhost/ui/check.html'},history:{replaceState(){}},addEventListener(){},cancelAnimationFrame(){},MOUTH_PRESETS:{close:[0,0]},
    createAvatarRenderer:()=>({dispose(){},cancelPending(){},async applyState(candidate,{beforeCommit}){if(await beforeCommit()===false)return false;committed.push(candidate.rigUrl);return true;}}),
    loadLocalJson:async url=>{assert.equal(url,'/api/snapshot-config');return limits;},
    fetch:async(url,options)=>{
      assert.match(url,/^\/api\/characters\/c_[0-9a-f]{12}\/snapshot$/);assert.equal(options.redirect,'error');assert.ok(options.signal instanceof AbortSignal);
      if(mode==='waiting')return new Response(new ReadableStream({pull(){entered.resolve();},cancel(){cancelled++;}}));
      return response(mode==='complete');
    },
    consumeSnapshot:(response,limits,options)=>consumeSnapshot(response,limits,{...options,createUrl:()=>{const url='blob:fixture-'+(++serial);created.push(url);return url;},revokeUrl:url=>revoked.push(url)})};
  const api=vm.runInNewContext(checkSource().replace('await initialize();','')+'\n({load});',env);
  node('#character').value='c_2700e1166676';await api.load();const original=created.slice();
  assert.equal(committed.length,1);assert.equal(revoked.length,0);
  mode='incomplete';node('#character').value='c_190454c86edb';await api.load();
  assert.equal(committed.length,1);assert.equal(node('#character').value,'c_2700e1166676');
  assert.deepEqual(revoked,created.slice(original.length));assert.match(node('#status').textContent,/完了.*\n.*保持/s);
  mode='waiting';node('#character').value='c_190454c86edb';const waiting=api.load();await entered.promise;
  mode='complete';node('#character').value='c_2700e1166676';await api.load();await waiting;
  assert.equal(cancelled,1);assert.equal(committed.length,2);assert.ok(original.every(url=>revoked.includes(url)));
  assert.ok(created.slice(-2).every(url=>!revoked.includes(url)));assert.equal(node('#avatar').style.visibility,'visible');
});
