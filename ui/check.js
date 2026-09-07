import {consumeSnapshot} from './shared/snapshot-client.js';
import {createAvatarRenderer} from './shared/avatar-renderer.js?v=native-batch12';
import {loadLocalJson} from './shared/local-assets.js';
import {MOUTH_PRESETS} from './shared/mouth-geometry.js';

// 通常の確認画面には公開中の最新世代だけを出す。旧比較候補の資産は削除しない。
const fixtures=[];
const select=document.querySelector('#character'),status=document.querySelector('#status');
const requestedCharacter=new URL(location.href).searchParams.get('character');
const mouthShape=document.querySelector('#mouth-shape');
for(const [key,label] of [['close','閉口'],['a','あ'],['i','い'],['u','う'],['e','え'],['o','お']])mouthShape.add(new Option(label,key));
const renderer=createAvatarRenderer(document.querySelector('#avatar'),{onError:error=>{
  stopDemo();stopMotion();document.querySelector('#avatar').style.visibility='hidden';
  status.textContent='描画に失敗しました。キャラを選び直してください: '+error.message;
}});
let currentSnapshot=null,loadAbort=null;
let state={},generation=0,currentRig,faceView=false,displayedCharacter=null;
let demoFrame=0,demoStarted=0;
let motionFrame=0;
let blinkFrame=0,nextBlinkAt=0,lastBlinkOpen=1;
function stopMotion(){cancelAnimationFrame(motionFrame);motionFrame=0;document.querySelector('#motion-demo').textContent='待機動作テスト';}
function stopDemo(){cancelAnimationFrame(demoFrame);demoFrame=0;document.querySelector('#mouth-demo').textContent='口パク動作テスト（無音）';document.querySelector('#mouth-preset').textContent='';}
function stopBlink(){
  const running=Boolean(blinkFrame);cancelAnimationFrame(blinkFrame);blinkFrame=0;lastBlinkOpen=1;
  if(running){
    state.eyeLOpen=state.eyeROpen=1;
    const left=inputs.get('eyeLOpen'),right=inputs.get('eyeROpen');if(left)left.value=1;if(right)right.value=1;
  }
  document.querySelector('#blink').textContent='自動まばたきを開始';
}
const inputs=new Map();
for(const [key,title,min,max,value] of [['mouthOpenY','開き',0,1,0],['mouthForm','横幅・丸み',-1,1,0],['eyeLOpen','左目',0,1,1],['eyeROpen','右目',0,1,1],['yaw','顔左右',-15,15,0],['pitch','顔上下',-15,15,0],['roll','首の傾き',-15,15,0],['armInset','腕を寄せる',0,10,0]]) {
  const label=document.createElement('label');label.append(title);
  const input=document.createElement('input');input.type='range';input.min=min;input.max=max;input.step=(max-min)/100;input.value=value;
  input.addEventListener('input',()=>{stopDemo();stopMotion();stopBlink();mouthShape.value='';state[key]=Number(input.value);
    if(key==='armInset')state.armPose={leftUpperArm:[0,0,-state.armInset],rightUpperArm:[0,0,state.armInset]};
    state.preserveOriginalMouth=false;apply();});
  label.append(input);document.querySelector('#controls').append(label);inputs.set(key,input);
}
function reset(){stopMotion();stopBlink();state={...state,mouthOpenY:0,mouthForm:0,eyeLOpen:1,eyeROpen:1,yaw:0,pitch:0,roll:0,armInset:0,armPose:{},idleSwayDegrees:0,preserveOriginalMouth:false};for(const [key,input] of inputs)input.value=state[key];}
async function apply(token=generation){
  try{return await renderer.applyState(state)!==false&&token===generation;}
  catch(error){if(token===generation)status.textContent=error.message;return false;}
}
async function load(){
  stopMotion();
  stopDemo();
  stopBlink();
  mouthShape.value='';
  loadAbort?.abort();loadAbort=new AbortController();
  const signal=loadAbort.signal;let candidateSnapshot=null;
  const token=++generation;status.textContent='読込中';
  renderer.cancelPending?.();
  const requested=select.value;
  if(!currentRig)document.querySelector('#avatar').style.visibility='hidden';
  document.querySelectorAll('button,input,#mouth-shape').forEach(element=>element.disabled=true);
  try {
    if(!fixtures.some(([id])=>id===select.value))throw new Error('指定されたキャラは確認一覧にありません。キャラを選び直してください');
    const base=`../temp/t7-characters/${encodeURIComponent(requested)}/`;
    const limits=await loadLocalJson('/api/snapshot-config',{cache:'no-store'});
    const snapshotResponse=await fetch('/api/characters/'+encodeURIComponent(requested)+'/snapshot',{cache:'no-store',signal,redirect:'error'});
    if(snapshotResponse.status!==404){
      candidateSnapshot=await consumeSnapshot(snapshotResponse,limits,{signal});
    }else await snapshotResponse.body?.cancel();
    const character=candidateSnapshot?null:await loadLocalJson(base+'character.json?read='+Date.now());
    const finalStage=character?.model?.rig2d_base?character.stages?.complete:character?.stages?.rig2d;
    if(!candidateSnapshot&&finalStage?.status!=='complete')throw new Error('このキャラのリグ生成・局所補完は未完了です');
    const version=encodeURIComponent(candidateSnapshot?.generation??finalStage.updatedAtIso);
    const rigUrl=candidateSnapshot?.rigUrl??base+'rig2d/rig.json?v='+version;const rig=candidateSnapshot?.rig??await loadLocalJson(rigUrl);
    if(token!==generation)return;
    const candidate={rigUrl,showHiddenMaterial:document.querySelector('#hidden-material').checked,
      mouthOpenY:0,mouthForm:0,eyeLOpen:1,eyeROpen:1,yaw:0,pitch:0,roll:0,armInset:0,armPose:{},idleSwayDegrees:0,
      partUrls:candidateSnapshot?.partUrls??Object.fromEntries(Object.keys(rig.layers).map(name=>[name,base+`rig2d/parts/${name}.png?v=${version}`]))};
    const loaded=await renderer.applyState(candidate,{beforeCommit:async()=>{
      if(token!==generation)return false;
      if(candidateSnapshot)return true;
      const latest=await loadLocalJson(base+'character.json',{cache:'no-store'});
      if(token!==generation)return false;
      const latestStage=latest.model?.rig2d_base?latest.stages?.complete:latest.stages?.rig2d;
      if(latestStage?.status!=='complete'||latestStage.updatedAtIso!==finalStage.updatedAtIso)
        throw new Error('読み込み中に生成世代が変わりました。生成完了後にキャラを選び直してください');
      return true;
    }});
    if(loaded===false||token!==generation)return;
    currentSnapshot?.dispose();currentSnapshot=candidateSnapshot;candidateSnapshot=null;
    currentRig=rig;state=candidate;displayedCharacter=requested;faceView=false;
    for(const [key,input] of inputs)input.value=state[key];
    document.querySelector('#source').style.transform='';document.querySelector('#source').src=base+'source/input.png';
    document.title=`2.5D品質確認：${fixtures.find(([id])=>id===displayedCharacter)[1]}`;
    document.querySelector('#avatar').style.visibility='visible';
    const approval=fixtures.find(([id])=>id===select.value)?.[2] ?? '見た目: 未承認。動作の成立と品質の合格は別です。';
    status.textContent=`素材充足: ${rig.material_readiness?.status ?? '未検査'} ／ ${approval}`;
    if(rig.local_completion?.hidden?.warning)status.textContent+='\n補完の未達: '+rig.local_completion.hidden.warning;
    if(rig.experimental_hidden)status.textContent+=rig.experimental_hidden.redraw_ear_contour?'\n耳輪郭の修正比較: 原画ファイルは保持し、可動モデルの耳・頬の境界を修正しています。既存むぎと切り替えて比較してください。':'\n補完比較: 中立の原画を保持。動作時は耳・頬の境界だけを補修します。補完表示のオン/オフで比較できます。';
    if(rig.experimental_closed_eyes)status.textContent+='\n閉眼素材の比較: 全開は原画の目を保持し、編集画像から測定した閉眼曲線へ連続して閉じます。目以外の編集結果は採用していません。';
  }catch(error){if(token===generation){
    if(displayedCharacter){
      select.value=displayedCharacter;
      const url=new URL(location.href);url.searchParams.set('character',displayedCharacter);history.replaceState(null,'',url);
      status.textContent=error.message+'\n表示は前回の「'+fixtures.find(([id])=>id===displayedCharacter)[1]+'」を保持しています。';
    }else status.textContent=error.message;
  }}
  finally{candidateSnapshot?.dispose();if(token===generation)document.querySelectorAll('button,input,#mouth-shape').forEach(element=>element.disabled=false);}
}
select.addEventListener('change',()=>{
  const url=new URL(location.href);url.searchParams.set('character',select.value);history.replaceState(null,'',url);load();
});
mouthShape.addEventListener('change',()=>{
  const preset=MOUTH_PRESETS[mouthShape.value];if(!preset)return;
  stopDemo();state.mouthOpenY=preset[0];state.mouthForm=preset[1];state.preserveOriginalMouth=false;
  inputs.get('mouthOpenY').value=preset[0];inputs.get('mouthForm').value=preset[1];apply();
});
document.querySelector('#hidden-material').addEventListener('change',event=>{state.showHiddenMaterial=event.target.checked;apply();});
document.querySelector('#motion-demo').addEventListener('click',()=>{
  if(motionFrame){stopMotion();return;}
  const start=performance.now();document.querySelector('#motion-demo').textContent='待機動作テストを停止';
  function tick(now){
    // 比較用の既知の小角度を共通レンダラーへ渡す。キャラ別の動作は持たない。
    const phase=(now-start)/4200*Math.PI*2;
    state.yaw=Math.sin(phase)*8;state.pitch=Math.sin(phase*.5)*3;state.roll=Math.sin(phase*.75)*3;
    for(const key of ['yaw','pitch','roll'])inputs.get(key).value=state[key];
    apply();motionFrame=requestAnimationFrame(tick);
  }
  motionFrame=requestAnimationFrame(tick);
});
document.querySelector('#neutral').addEventListener('click',()=>{stopDemo();mouthShape.value='';reset();state.preserveOriginalMouth=true;apply();});
document.querySelector('#blink').addEventListener('click',()=>{
  if(blinkFrame){
    stopBlink();
    state.eyeLOpen=state.eyeROpen=1;
    inputs.get('eyeLOpen').value=inputs.get('eyeROpen').value=1;
    apply();return;
  }
  nextBlinkAt=performance.now();lastBlinkOpen=1;
  document.querySelector('#blink').textContent='自動まばたきを停止';
  function tick(now){
    const elapsed=now-nextBlinkAt;
    let open=1;
    if(elapsed>=0&&elapsed<=360)open=1-Math.sin(elapsed/360*Math.PI);
    else if(elapsed>360)nextBlinkAt=now+2800+Math.random()*3700;
    if(Math.abs(open-lastBlinkOpen)>.001){
      lastBlinkOpen=open;state.eyeLOpen=state.eyeROpen=open;
      inputs.get('eyeLOpen').value=inputs.get('eyeROpen').value=open;apply();
    }
    blinkFrame=requestAnimationFrame(tick);
  }
  blinkFrame=requestAnimationFrame(tick);
});
document.querySelector('#mouth-demo').addEventListener('click',()=>{
  if(demoFrame){stopDemo();return;}
  mouthShape.value='';
  const presets=Object.entries(MOUTH_PRESETS);
  demoStarted=performance.now();document.querySelector('#mouth-demo').textContent='口パクテストを停止';
  function tick(now){
    const phase=(now-demoStarted)/650,index=Math.floor(phase)%presets.length;
    const [name,to]=presets[index],from=presets[(index+presets.length-1)%presets.length][1];
    const t=Math.min(1,(phase-Math.floor(phase))*3),smooth=t*t*(3-2*t);
    state.mouthOpenY=from[0]+(to[0]-from[0])*smooth;state.mouthForm=from[1]+(to[1]-from[1])*smooth;state.preserveOriginalMouth=false;
    inputs.get('mouthOpenY').value=state.mouthOpenY;inputs.get('mouthForm').value=state.mouthForm;
    document.querySelector('#mouth-preset').textContent=`検証口形: ${name}`;
    apply();demoFrame=requestAnimationFrame(tick);
  }
  demoFrame=requestAnimationFrame(tick);
});
function focusFace(){
  if(!currentRig)return;
  const canvas=document.querySelector('#avatar'),source=document.querySelector('#source');
  if(!faceView){state.scale=1;state.offsetX=state.offsetY=0;source.style.transform='';apply();return;}
  const {width:w,height:h}=currentRig.canvas,[l,t,r,b]=currentRig.layers.face.bbox;
  const fit=Math.min(canvas.clientWidth/w,canvas.clientHeight/h),zoom=canvas.clientHeight/((b-t)*fit);
  state.scale=zoom;state.offsetX=(w/2-(l+r)/2)*fit*zoom;state.offsetY=(h/2-(t+b)/2)*fit*zoom;
  source.style.transform=`translate(${state.offsetX}px,${state.offsetY}px) scale(${zoom})`;apply();
}
document.querySelector('#face').addEventListener('click',()=>{faceView=!faceView;focusFace();});
addEventListener('resize',focusFace);
addEventListener('beforeunload',()=>{stopDemo();stopMotion();++generation;loadAbort?.abort();renderer.dispose();currentSnapshot?.dispose();},{once:true});
async function initialize(){
 try {
  const normal=await loadLocalJson('/api/normal-characters');
  for(const character of normal.characters){
    if(!/^c_[0-9a-f]{12}$/.test(character.id)||typeof character.name!=='string')throw new Error('通常生成のキャラ一覧が不正です');
    if(!fixtures.some(([id])=>id===character.id))fixtures.push([character.id,'通常生成：'+character.name,'通常の4工程を完了した成果物です。原画と比較して見た目を確認してください。']);
  }
  if(!fixtures.length)throw new Error('公開済みの最新キャラがありません');
  for(const [id,name] of fixtures)select.add(new Option(name,id));
  if(requestedCharacter&&fixtures.some(([id])=>id===requestedCharacter))select.value=requestedCharacter;
  else select.value=fixtures[0][0];
  const url=new URL(location.href);url.searchParams.set('character',select.value);history.replaceState(null,'',url);
  await load();
 } catch(error) {status.textContent='キャラ一覧を読み込めません: '+error.message;}
}
await initialize();
