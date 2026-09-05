import * as THREE from "./vendor/three/three.module.min.js";
import {localAssetUrl, loadLocalJson} from "./local-assets.js";
import {drawTexturedMouth,lipMesh,MOUTH_PRESETS as MOUTHS} from "./mouth-geometry.js";
import {eyeAperture,drawBlink} from './eye-geometry.js';

const clamp = (v,a,b) => Math.max(a,Math.min(b,v));
const smooth = (a,b,x) => { const t=clamp((x-a)/(b-a),0,1); return t*t*(3-2*t); };

export function randomBlinkDelay(state, random = Math.random) {
  const low=state.blinkMinMs ?? 2800;
  return low+random()*Math.max(0,(state.blinkMaxMs ?? 6500)-low);
}

export function createAvatarRenderer(canvas) {
  const renderer=new THREE.WebGLRenderer({canvas,alpha:true,antialias:true});
  renderer.setClearColor(0,0);
  renderer.setPixelRatio(devicePixelRatio || 1);
  const scene=new THREE.Scene();
  const camera=new THREE.OrthographicCamera(-1,1,1,-1,.1,10);
  camera.position.z=2;
  const sheet=document.createElement("canvas");
  const ctx=sheet.getContext("2d");
  let rig,mesh,texture,images=new Map(),state={},revision=0,disposed=false,frame;
  let nextBlink=Infinity,lastAppearance="";
  const observer=new ResizeObserver(resize);
  function resize() {
    renderer.setSize(Math.max(1,canvas.clientWidth),Math.max(1,canvas.clientHeight),false);
  }
  function release() {
    if(mesh) { scene.remove(mesh);mesh.geometry.dispose();mesh.material.dispose();mesh=null; }
    texture?.dispose(); texture=null;images.clear();
  }
  async function applyState(next) {
    const token=++revision;
    if(next.rigUrl!==state.rigUrl || !rig) {
      const incoming=await loadLocalJson(next.rigUrl);
      if(incoming.schema_version!==3) throw new Error("旧リグです。リグ工程から再生成してください");
      if(incoming.lip_rig_version!==1)throw new Error('唇の分割がない旧リグです。分解工程から再生成してください');
      lipMesh(incoming.layers?.mouth_closed,0,0);
      if(incoming.eye_rig_version!==1)throw new Error('まぶたの分割がない旧リグです。分解から再生成してください');
      for(const side of ['left','right'])eyeAperture(incoming.layers?.[side+'_eye_base'],1);
      const names=['neutral','mouth_open','mouth_closed',...['left','right'].flatMap(side=>[side+'_eye_open',side+'_eye_base',side+'_eyelid_upper'])];
      const loaded=await Promise.all(names.map(async name=>{
        const layer=incoming.layers[name];
        if(!layer) throw new Error(`必須レイヤーがありません: ${name}`);
        const src=next.partUrls?.[name] ?? layer.url;
        const asset=localAssetUrl(src);
        const image=new Image();image.src=asset.href;await image.decode();
        const box=layer.texture_box;
        if(!box || image.naturalWidth!==box[2]-box[0] || image.naturalHeight!==box[3]-box[1]) throw new Error(`切詰めレイヤー寸法が一致しません: ${name}`);
        return [name,image];
      }));
      if(disposed || token!==revision) return;
      release();rig=incoming;images=new Map(loaded);
      sheet.width=rig.canvas.width;sheet.height=rig.canvas.height;
      texture=new THREE.CanvasTexture(sheet);
      texture.colorSpace=THREE.SRGBColorSpace;
      const geometry=new THREE.PlaneGeometry(sheet.width,sheet.height,64,96);
      const material=new THREE.MeshBasicMaterial({map:texture,transparent:true,depthWrite:false});
      mesh=new THREE.Mesh(geometry,material);mesh.frustumCulled=false;scene.add(mesh);
      nextBlink=performance.now()+randomBlinkDelay(next);
      lastAppearance="";
    }
    state={...next};
  }
  function appearance(left,right,open,form) {
    const key=[left,right,open,form,state.preserveOriginalMouth?1:0].map(v=>v.toFixed(3)).join(",");
    if(key===lastAppearance) return;
    lastAppearance=key;
    ctx.clearRect(0,0,sheet.width,sheet.height);
    // 重複部位の半透明画素を重ねず、中立は原画のアルファを完全保持する。
    const drawLayer=name=>{const box=rig.layers[name].texture_box;ctx.drawImage(images.get(name),box[0],box[1]);};
    drawLayer("neutral");
    drawBlink(ctx,images,rig,'left',1-left);
    drawBlink(ctx,images,rig,'right',1-right);
    if(!state.preserveOriginalMouth || open>0 || form!==0) {
      // 下地は変形させず、元の口を消した同じ座標へ合成する。
      // 中立の閉口では原画の唇をそのまま保持する。
      if(open>0 || form!==0)drawLayer("mouth_open");
      const layer=rig.layers.mouth_open;
      const box=layer.feature_box;
      if(!box) throw new Error("口の実測座標がありません");
      drawTexturedMouth(ctx,images.get('mouth_closed'),rig.layers.mouth_closed,open,form,layer.line_color);
    }
    texture.needsUpdate=true;
  }
  function render(now) {
    if(disposed) return;
    if(rig) {
      const duration=Math.max(1,state.blinkDurationMs ?? 180);
      const phase=(now-nextBlink)/duration;
      const blink=state.expressionKey==='blink' ? 1 : (phase>=0 && phase<=1 ? Math.sin(phase*Math.PI) : 0);
      if(phase>1) nextBlink=now+randomBlinkDelay(state);
      let [open,form]=MOUTHS[state.mouthKey] ?? MOUTHS.close;
      open=clamp(state.mouthOpenY ?? open,0,1);form=clamp(state.mouthForm ?? form,-1,1);
      appearance(state.eyeLOpen===undefined?blink:1-clamp(state.eyeLOpen,0,1),
        state.eyeROpen===undefined?blink:1-clamp(state.eyeROpen,0,1),open,form);
      const w=sheet.width,h=sheet.height,face=rig.layers.face.bbox;
      const neckBox=rig.layers.neck?.bbox;
      const neck=neckBox ? neckBox[3] : face[3];
      const cx=face ? (face[0]+face[2])/2 : w/2;
      const wave=Math.sin(now/(state.idleSwayPeriodMs ?? 4200)*Math.PI*2);
      const sway=(state.idleSwayDegrees ?? .7)*wave;
      const positions=mesh.geometry.attributes.position,uv=mesh.geometry.attributes.uv;
      // 全パーツを同じ連続変位場へ通す。首・肩に独立回転の裂け目を作らない。
      for(let i=0;i<positions.count;i++) {
        const x=uv.getX(i)*w,y=(1-uv.getY(i))*h;
        const head=1-smooth(face[3],Math.max(face[3]+1,neck),y);
        let dx=head*clamp(state.yaw ?? 0,-30,30)*w*.00055;
        let dy=head*clamp(state.pitch ?? 0,-30,30)*h*.00035;
        dx+=(h-y)/h*sway*w*.002;
        const side=x<cx?"left":"right";
        const arm=rig.layers[side+"_arm"].bbox;
        if(arm) {
          const shoulder=arm[1]+(arm[3]-arm[1])*.10;
          const influence=smooth(w*.06,w*.26,Math.abs(x-cx))*smooth(shoulder,shoulder+(arm[3]-shoulder)*.45,y);
          const angle=clamp(state.armPose?.[side+"UpperArm"]?.[2] ?? 0,-30,30)*Math.PI/180;
          dx+=-(y-shoulder)*Math.sin(angle)*influence;
          dy+=(y-shoulder)*(Math.cos(angle)-1)*influence;
        }
        const hair=(state.hairSway ?? wave)*(state.idleSwayDegrees ?? .7);
        dx+=hair*w*.001*head*smooth(w*.045,w*.13,Math.abs(x-cx));
        positions.setXYZ(i,x-w/2+dx,h/2-y-dy,0);
      }
      positions.needsUpdate=true;
      const cw=Math.max(1,canvas.clientWidth),ch=Math.max(1,canvas.clientHeight);
      camera.left=-cw/2;camera.right=cw/2;camera.top=ch/2;camera.bottom=-ch/2;camera.updateProjectionMatrix();
      const fit=Math.min(cw/w,ch/h)*(state.scale ?? 1);
      mesh.scale.setScalar(fit);mesh.position.set(state.offsetX ?? 0,-(state.offsetY ?? 0),0);
      renderer.render(scene,camera);
    }
    frame=requestAnimationFrame(render);
  }
  function dispose() {
    disposed=true;++revision;cancelAnimationFrame(frame);observer.disconnect();
    release();renderer.dispose();renderer.forceContextLoss();
    sheet.width=sheet.height=0;
  }
  observer.observe(canvas);resize();frame=requestAnimationFrame(render);
  return {applyState,dispose};
}
