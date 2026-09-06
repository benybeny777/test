import {createAvatarRenderer} from './shared/avatar-renderer.js?v=hidden-preview8';
import {loadLocalJson} from './shared/local-assets.js';
import {MOUTH_PRESETS} from './shared/mouth-geometry.js';

// 比較対象の一覧だけを持つ。キャラごとの生成・変形パラメータは持たない。
const fixtures=[['c_2700e1166676','女性A'],['c_190454c86edb','むぎ'],['c_828ead7c98ab','実写テスト'],['c_2379190bb3b3','むぎ・耳輪郭修正候補'],['c_df28cf7d4d11','むぎ・耳＋閉眼修正候補']];
const select=document.querySelector('#character'),status=document.querySelector('#status');
for(const [id,name] of fixtures)select.add(new Option(name,id));
const renderer=createAvatarRenderer(document.querySelector('#avatar'));
let state={},generation=0,currentRig,faceView=false;
let demoFrame=0,demoStarted=0;
let motionFrame=0;
function stopMotion(){cancelAnimationFrame(motionFrame);motionFrame=0;document.querySelector('#motion-demo').textContent='待機動作テスト';}
function stopDemo(){cancelAnimationFrame(demoFrame);demoFrame=0;document.querySelector('#mouth-demo').textContent='口パク動作テスト（無音）';document.querySelector('#mouth-preset').textContent='';}
const inputs=new Map();
for(const [key,title,min,max,value] of [['mouthOpenY','開き',0,1,0],['mouthForm','横幅・丸み',-1,1,0],['eyeLOpen','左目',0,1,1],['eyeROpen','右目',0,1,1],['yaw','顔左右',-15,15,0],['pitch','顔上下',-15,15,0],['roll','首の傾き',-15,15,0],['armInset','腕を寄せる',0,10,0]]) {
  const label=document.createElement('label');label.append(title);
  const input=document.createElement('input');input.type='range';input.min=min;input.max=max;input.step=(max-min)/100;input.value=value;
  input.addEventListener('input',()=>{stopDemo();stopMotion();state[key]=Number(input.value);
    if(key==='armInset')state.armPose={leftUpperArm:[0,0,-state.armInset],rightUpperArm:[0,0,state.armInset]};
    state.preserveOriginalMouth=false;apply();});
  label.append(input);document.querySelector('#controls').append(label);inputs.set(key,input);
}
function reset(){stopMotion();state={...state,mouthOpenY:0,mouthForm:0,eyeLOpen:1,eyeROpen:1,yaw:0,pitch:0,roll:0,armInset:0,armPose:{},idleSwayDegrees:0,preserveOriginalMouth:false};for(const [key,input] of inputs)input.value=state[key];}
async function apply(){try{await renderer.applyState(state);return true;}catch(error){status.textContent=error.message;return false;}}
async function load(){
  stopMotion();
  stopDemo();
  const token=++generation;status.textContent='読込中';
  document.querySelector('#avatar').style.visibility='hidden';
  document.querySelectorAll('button,input').forEach(element=>element.disabled=true);
  try {
    const base=`../temp/t7-characters/${encodeURIComponent(select.value)}/`;
    const character=await loadLocalJson(base+'character.json?read='+Date.now());
    if(character.stages?.rig2d?.status!=='complete')throw new Error('このキャラのリグ再生成は未完了です');
    const version=encodeURIComponent(character.stages.rig2d.updatedAtIso);
    const rigUrl=base+'rig2d/rig.json?v='+version;const rig=await loadLocalJson(rigUrl);
    if(token!==generation)return;
    currentRig=rig;state={rigUrl,showHiddenMaterial:document.querySelector('#hidden-material').checked,partUrls:Object.fromEntries(Object.keys(rig.layers).map(name=>[name,base+`rig2d/parts/${name}.png?v=${version}`]))};reset();faceView=false;document.querySelector('#source').style.transform='';
    document.querySelector('#source').src=base+'source/input.png';if(!await apply())return;
    document.querySelector('#avatar').style.visibility='visible';
    status.textContent=`素材充足: ${rig.material_readiness?.status ?? '未検査'} ／ 見た目: 未承認。動作の成立と品質の合格は別です。`;
    if(rig.experimental_hidden)status.textContent+=rig.experimental_hidden.redraw_ear_contour?'\n耳輪郭の修正比較: 原画ファイルは保持し、可動モデルの耳・頬の境界を修正しています。既存むぎと切り替えて比較してください。':'\n補完比較: 中立の原画を保持。動作時は耳・頬の境界だけを補修します。補完表示のオン/オフで比較できます。';
    if(rig.experimental_closed_eyes)status.textContent+='\n閉眼素材の比較: 全開は原画の目を保持し、編集画像から測定した閉眼曲線へ連続して閉じます。目以外の編集結果は採用していません。';
  }catch(error){if(token===generation)status.textContent=error.message;}
  finally{if(token===generation)document.querySelectorAll('button,input').forEach(element=>element.disabled=false);}
}
select.addEventListener('change',load);
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
document.querySelector('#neutral').addEventListener('click',()=>{stopDemo();reset();state.preserveOriginalMouth=true;apply();});
document.querySelector('#blink').addEventListener('click',()=>{delete state.eyeLOpen;delete state.eyeROpen;apply();});
document.querySelector('#mouth-demo').addEventListener('click',()=>{
  if(demoFrame){stopDemo();return;}
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
addEventListener('beforeunload',()=>{stopDemo();stopMotion();++generation;renderer.dispose();},{once:true});
await load();
