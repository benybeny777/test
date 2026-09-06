import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import * as THREE from '../ui/shared/vendor/three/three.module.min.js';
import {planSceneBatches,sceneBatchBox,createNativeSceneBatch} from '../ui/shared/native-scene-batch.js';

const source=name=>readFileSync(new URL('../ui/'+name,import.meta.url),'utf8').replace(/^import .*;\r?\n/gm,'').replace(/export /g,'');
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
test('確認画面は旧ロード完了で次のキャラの待機・失敗画面を上書きしない',async()=>{
  const nodes=new Map();
  const node=selector=>{if(!nodes.has(selector))nodes.set(selector,{value:'',checked:true,style:{},textContent:'',
    add(){},append(){},addEventListener(){},removeAttribute(){}});return nodes.get(selector);};
  const old=deferred(),next=deferred();let applied;
  const entered=deferred();
  const env={document:{querySelector:node,querySelectorAll:()=>[],createElement:()=>node(Symbol())},
    Option:class{},URL,location:{href:'http://localhost/ui/check.html'},addEventListener(){},
    cancelAnimationFrame(){},MOUTH_PRESETS:{close:[0,0]},
    createAvatarRenderer:()=>({clear(){},dispose(){},applyState(){entered.resolve();return old.promise;}}),
    loadLocalJson:async url=>url.includes('c_190454c86edb')?next.promise:
      url.includes('character.json')?{stages:{rig2d:{status:'complete',updatedAtIso:'now'}}}:rig()};
  const api=vm.runInNewContext(source('check.js').replace('await initialize();','')+'\n({load});',env);
  node('#character').value='c_2700e1166676';applied=api.load();await entered.promise;
  node('#character').value='c_190454c86edb';const newer=api.load();
  old.resolve(true);await applied;
  assert.equal(node('#avatar').style.visibility,'hidden');assert.equal(node('#status').textContent,'読込中');
  next.reject(new Error('次のキャラの読込失敗'));await newer;
  assert.equal(node('#avatar').style.visibility,'hidden');assert.equal(node('#status').textContent,'次のキャラの読込失敗');
});
