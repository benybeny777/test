import * as THREE from './vendor/three/three.module.min.js';
import {localAssetUrl,loadLocalJson} from './local-assets.js';
import {drawTexturedMouth,lipMesh,MOUTH_PRESETS} from './mouth-geometry.js';
import {eyeAperture,drawBlink} from './eye-geometry.js';
import {bleedTransparentRgb} from './texture-alpha.js';
import {headDisplacement,armDisplacement,validateHiddenMotion,hiddenOffset,hiddenRepairAmount} from './rig-motion.js?v=ear-repair1';

const clamp=(v,a,b)=>Math.max(a,Math.min(b,v));
const smooth=(a,b,x)=>{const t=clamp((x-a)/(b-a),0,1);return t*t*(3-2*t);};
export function randomBlinkDelay(state,random=Math.random){const low=state.blinkMinMs??2800;return low+random()*Math.max(0,(state.blinkMaxMs??6500)-low);}

export function createAvatarRenderer(canvas){
  const renderer=new THREE.WebGLRenderer({canvas,alpha:true,antialias:true});
  renderer.setClearColor(0,0);renderer.setPixelRatio(devicePixelRatio||1);
  const scene=new THREE.Scene(),group=new THREE.Group();scene.add(group);
  const camera=new THREE.OrthographicCamera(-1,1,1,-1,.1,10);camera.position.z=2;
  const faceCanvas=document.createElement('canvas'),ctx=faceCanvas.getContext('2d');
  let rig,state={},images=new Map(),meshes=[],textures=[],positions,worldUV,faceTexture,faceAlpha;
  let revision=0,disposed=false,frame,nextBlink=Infinity,lastAppearance='';
  const resize=()=>renderer.setSize(Math.max(1,canvas.clientWidth),Math.max(1,canvas.clientHeight),false);
  const observer=new ResizeObserver(resize);observer.observe(canvas);resize();
  function release(){
    for(const mesh of meshes){group.remove(mesh);mesh.geometry.dispose();mesh.material.dispose();}
    for(const texture of textures)texture.dispose();
    meshes=[];textures=[];images.clear();faceTexture=null;faceAlpha=null;rig=null;
  }
  async function applyState(next){
    const token=++revision;
    if(next.rigUrl!==state.rigUrl||!rig){
      const incoming=await loadLocalJson(next.rigUrl);
      if(incoming.schema_version!==3||incoming.eye_rig_version!==3||incoming.lip_rig_version!==1||incoming.scene_graph_version!==1)
        throw new Error('独立部位または目口の素材がない旧リグです。分解から再生成してください');
      const graph=incoming.scene_graph;
      validateHiddenMotion(incoming.hidden_motion,65*97);
      if(incoming.hidden_motion&&!graph?.some(p=>p.role==='hidden_face'))throw new Error('補完比較の下地がありません');
      if(!Array.isArray(graph)||graph.length<6||new Set(graph.map(p=>p.role)).size!==graph.length||!graph.some(p=>p.role==='face'))
        throw new Error('独立部位の構造が不正です');
      lipMesh(incoming.layers?.mouth_closed,0,0);
      for(const side of ['left','right'])eyeAperture(incoming.layers?.[side+'_eye_base'],1);
      const names=[...graph.map(p=>p.layer),'mouth_open','mouth_closed',...['left','right'].flatMap(side=>
        ['eye_iris','eye_backplate','eye_base','eyelid_upper'].map(part=>side+'_'+part))];
      if(incoming.hidden_motion?.repair_layer)names.push(incoming.hidden_motion.repair_layer);
      const loaded=await Promise.all(names.map(async name=>{
        const layer=incoming.layers[name];if(!layer)throw new Error('必須素材がありません: '+name);
        const image=new Image();image.src=localAssetUrl(next.partUrls?.[name]??layer.url).href;await image.decode();
        const box=layer.texture_box;
        if(!box||image.naturalWidth!==box[2]-box[0]||image.naturalHeight!==box[3]-box[1])throw new Error('素材寸法が一致しません: '+name);
        return [name,image];
      }));
      if(disposed||token!==revision)return;
      release();rig=incoming;images=new Map(loaded);
      const {width:w,height:h}=rig.canvas;
      const template=new THREE.PlaneGeometry(w,h,64,96);
      positions=template.attributes.position;worldUV=template.attributes.uv.array.slice();
      const face=images.get('scene_face');faceCanvas.width=face.naturalWidth;faceCanvas.height=face.naturalHeight;
      ctx.drawImage(face,0,0);faceAlpha=ctx.getImageData(0,0,faceCanvas.width,faceCanvas.height).data.filter((_,i)=>i%4===3);
      for(const [index,part] of graph.entries()){
        const geometry=template.clone();
        geometry.setAttribute('position',rig.hidden_motion&&part.role==='hair'?positions.clone():positions);
        const box=rig.layers[part.layer].texture_box,uv=geometry.attributes.uv;
        for(let i=0;i<uv.count;i++)uv.setXY(i,(worldUV[i*2]*w-box[0])/(box[2]-box[0]),1-((1-worldUV[i*2+1])*h-box[1])/(box[3]-box[1]));
        // Canvasは透明RGBを失うため、合成後に色を補完した画素を直接アップロードする。
        const texture=part.role==='face'?new THREE.DataTexture(new Uint8Array(faceCanvas.width*faceCanvas.height*4),faceCanvas.width,faceCanvas.height):new THREE.Texture(images.get(part.layer));
        if(part.role==='face'){texture.flipY=true;texture.magFilter=THREE.LinearFilter;texture.minFilter=THREE.LinearMipmapLinearFilter;texture.generateMipmaps=true;}
        texture.colorSpace=THREE.SRGBColorSpace;texture.needsUpdate=true;textures.push(texture);
        if(part.role==='face')faceTexture=texture;
        const material=new THREE.MeshBasicMaterial({map:texture,transparent:true,depthWrite:false,depthTest:false});
        // 全部位で同じ頂点格子を共有し、切詰めテクスチャの外は描かない。
        material.onBeforeCompile=shader=>{
          const marker='#include <map_fragment>';
          if(!shader.fragmentShader.includes(marker))throw new Error('部位クリップのシェーダーが未対応です');
          shader.fragmentShader=shader.fragmentShader.replace(marker,'if(any(lessThan(vMapUv,vec2(0.0)))||any(greaterThan(vMapUv,vec2(1.0)))) discard;\n'+marker);
        };
        material.customProgramCacheKey=()=> 'lvs-independent-crop-v1';
        const mesh=new THREE.Mesh(geometry,material);mesh.userData.role=part.role;mesh.renderOrder=index;mesh.frustumCulled=false;group.add(mesh);meshes.push(mesh);
      }
      template.dispose();lastAppearance='';nextBlink=performance.now()+randomBlinkDelay(next);
    }
    state={...next};
  }
  function appearance(left,right,open,form){
    const repair=rig.hidden_motion?.repair_layer;
    const repairAmount=hiddenRepairAmount(rig.hidden_motion,state.yaw??0,state.pitch??0,state.showHiddenMaterial!==false);
    const key=[left,right,open,form,state.preserveOriginalMouth?1:0,repairAmount].map(v=>v.toFixed(3)).join(',');
    if(key===lastAppearance)return;lastAppearance=key;
    const box=rig.layers.scene_face.texture_box;
    ctx.save();ctx.clearRect(0,0,faceCanvas.width,faceCanvas.height);ctx.translate(-box[0],-box[1]);
    ctx.drawImage(images.get('scene_face'),box[0],box[1]);
    if(repairAmount>0){const bounds=rig.layers[repair].texture_box;ctx.globalAlpha=repairAmount;ctx.drawImage(images.get(repair),bounds[0],bounds[1]);ctx.globalAlpha=1;}
    drawBlink(ctx,images,rig,'left',left);drawBlink(ctx,images,rig,'right',right);
    if(open>0||form!==0){const base=rig.layers.mouth_open.texture_box;ctx.drawImage(images.get('mouth_open'),base[0],base[1]);
      drawTexturedMouth(ctx,images.get('mouth_closed'),rig.layers.mouth_closed,open,form,rig.layers.mouth_open.line_color);}
    ctx.restore();
    const pixels=ctx.getImageData(0,0,faceCanvas.width,faceCanvas.height).data;
    // 元のアルファを再設定し、顔マスクを二重乗算して輪郭を薄くしない。
    for(let i=0;i<faceAlpha.length;i++)pixels[i*4+3]=faceAlpha[i];
    faceTexture.image.data.set(bleedTransparentRgb(pixels,faceCanvas.width,faceCanvas.height));faceTexture.needsUpdate=true;
  }
  function render(now){
    if(disposed)return;
    if(rig){
      const phase=(now-nextBlink)/Math.max(1,state.blinkDurationMs??180);
      const blink=state.expressionKey==='blink'?1:phase>=0&&phase<=1?Math.sin(phase*Math.PI):0;
      if(phase>1)nextBlink=now+randomBlinkDelay(state);
      let [open,form]=MOUTH_PRESETS[state.mouthKey]??MOUTH_PRESETS.close;
      open=clamp(state.mouthOpenY??open,0,1);form=clamp(state.mouthForm??form,-1,1);
      appearance(state.eyeLOpen===undefined?1-blink:clamp(state.eyeLOpen,0,1),state.eyeROpen===undefined?1-blink:clamp(state.eyeROpen,0,1),open,form);
      const {width:w,height:h}=rig.canvas,face=rig.layers.face.bbox,neck=rig.layers.scene_neck.bbox,cx=(face[0]+face[2])/2;
      const wave=Math.sin(now/(state.idleSwayPeriodMs??4200)*Math.PI*2),sway=(state.idleSwayDegrees??.7)*wave;
      // 部位は独立テクスチャ・メッシュ。未補完の接続部を裂かない共通変位場を当面共有する。
      for(let i=0;i<positions.count;i++){
        const x=worldUV[i*2]*w,y=(1-worldUV[i*2+1])*h,head=1-smooth(face[3],Math.max(face[3]+1,neck[3]),y);
        let [dx,dy]=headDisplacement(x,y,face,neck,w,h,state.yaw??0,state.pitch??0,state.roll??state.armPose?.head?.[2]??0);
        dx+=(h-y)/h*sway*w*.002;
        const side=x<cx?'left':'right',arm=rig.layers[side+'_arm'].bbox;
        const [armX,armY]=armDisplacement(x,y,arm,cx,w,state.armPose?.[side+'UpperArm']?.[2]??0);
        dx+=armX;dy+=armY;
        dx+=(state.hairSway??wave)*(state.idleSwayDegrees??.7)*w*.001*head*smooth(w*.045,w*.13,Math.abs(x-cx));
        positions.setXYZ(i,x-w/2+dx,h/2-y-dy,0);
      }
      positions.needsUpdate=true;
      for(const mesh of meshes){
        if(mesh.userData.role==='hidden_face')mesh.visible=state.showHiddenMaterial!==false;
        if(rig.hidden_motion&&mesh.userData.role==='hair'){
          const hairPositions=mesh.geometry.attributes.position;
          for(let i=0;i<positions.count;i++){
            const [dx,dy]=hiddenOffset(rig.hidden_motion,i,state.yaw??0,state.pitch??0);
            hairPositions.setXYZ(i,positions.getX(i)+dx,positions.getY(i)-dy,0);
          }
          hairPositions.needsUpdate=true;
        }
      }
      const cw=Math.max(1,canvas.clientWidth),ch=Math.max(1,canvas.clientHeight);
      camera.left=-cw/2;camera.right=cw/2;camera.top=ch/2;camera.bottom=-ch/2;camera.updateProjectionMatrix();
      group.scale.setScalar(Math.min(cw/w,ch/h)*(state.scale??1));group.position.set(state.offsetX??0,-(state.offsetY??0),0);
      renderer.render(scene,camera);
    }
    frame=requestAnimationFrame(render);
  }
  function dispose(){disposed=true;++revision;cancelAnimationFrame(frame);observer.disconnect();release();renderer.dispose();renderer.forceContextLoss();faceCanvas.width=faceCanvas.height=0;}
  frame=requestAnimationFrame(render);
  return {applyState,dispose};
}
