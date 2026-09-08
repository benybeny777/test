import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from '../ui/shared/vendor/three/three.module.min.js';
import {semanticHairCoverage,coveragePixelBounds,createHairCoverage} from '../ui/shared/hair-coverage.js';

test('原寸髪所有は254も1も所有とし透明画素を増やさない',()=>{
  assert.deepEqual([...semanticHairCoverage(new Uint8Array([3,5,7,0,3,5,7,1,3,5,7,254,3,5,7,255]))],[0,255,255,255]);
  assert.throws(()=>semanticHairCoverage(new Uint8Array(3)),/RGBA/);
});
test('ROIは実画素境界へ外向きに丸め表示外を除く',()=>{
  assert.deepEqual(coveragePixelBounds([[20.25,30.5],[100.75,150.25]],120,160),{x:18,y:28,width:85,height:125});
  assert.deepEqual(coveragePixelBounds([[-100,-20],[20.5,30.5]],100,100),{x:0,y:0,width:23,height:33});
  assert.equal(coveragePixelBounds([[200,200],[300,300]],100,100),null);
  assert.throws(()=>coveragePixelBounds([[NaN,0]],100,100),/投影/);
});
function harness(){
  const geometry=new THREE.PlaneGeometry(100,100,2,2),common=geometry.attributes.position;
  const hair=new THREE.Mesh(geometry.clone(),new THREE.MeshBasicMaterial());hair.geometry.setAttribute('position',common.clone());
  const hidden=new THREE.Mesh(geometry.clone(),new THREE.MeshBasicMaterial());hidden.geometry.setAttribute('position',common);
  const camera=new THREE.OrthographicCamera(-50,50,50,-50,.1,10);camera.position.z=2;
  let target=null,fail=false,size=[200,200],scissor=true,clearAlpha=.3;
  const viewport=new THREE.Vector4(2,3,100,100),rect=new THREE.Vector4(4,5,80,80),color=new THREE.Color(.2,.3,.4),targets=new Set(),calls=[];
  const renderer={capabilities:{maxTextureSize:4096},getDrawingBufferSize:v=>v.set(...size),
    getRenderTarget:()=>target,setRenderTarget:v=>{target=v;if(v)targets.add(v);},getViewport:v=>v.copy(viewport),setViewport:v=>viewport.copy(v),
    getScissor:v=>v.copy(rect),setScissor:v=>rect.copy(v),getScissorTest:()=>scissor,setScissorTest:v=>{scissor=v;},
    getClearColor:v=>v.copy(color),getClearAlpha:()=>clearAlpha,setClearColor:(v,a)=>{color.set(v);clearAlpha=a;},clear(){},
    render(scene,cam){if(fail)throw new Error('RT故障');calls.push({scene,view:{...cam.view}});}};
  const previousDocument=globalThis.document;
  globalThis.document={createElement:()=>({getContext:()=>({drawImage(){},getImageData:()=>({data:new Uint8Array([1,2,3,254])})})})};
  let api;try{api=createHairCoverage(renderer,hair,hidden,common,{naturalWidth:1,naturalHeight:1});}finally{globalThis.document=previousDocument;}
  const shader={uniforms:{},fragmentShader:'#include <alphatest_fragment>'};hidden.material.onBeforeCompile(shader);
  const originalX=common.getX(0);
  return {api,camera,shader,calls,targets,hidden,hair,geometry,setSize:v=>{size=v;},fail:()=>{fail=true;},
    invalidateProjection:()=>common.setX(0,NaN),restoreProjection:()=>common.setX(0,originalX),
    state:()=>({target,scissor,clearAlpha,viewport:viewport.toArray(),rect:rect.toArray()}),
    dispose(){api.dispose();for(const object of [hair,hidden]){object.geometry.dispose();object.material.dispose();}geometry.dispose();}};
}
test('中立は下地ゼロで追加描画なし、移動時のみ同一投影の2パス',()=>{
  const h=harness();h.api.render(h.camera,false);
  assert.equal(h.calls.length,0);assert.equal(h.shader.uniforms.hairCoverageEnabled.value,0);
  h.api.render(h.camera,true);assert.equal(h.calls.length,2);
  assert.deepEqual(h.calls[0].view,h.calls[1].view);assert.equal(h.shader.uniforms.hairCoverageEnabled.value,1);
  assert.deepEqual(h.state(),{target:null,scissor:true,clearAlpha:.3,viewport:[2,3,100,100],rect:[4,5,80,80]});
  assert.equal(h.shader.uniforms.hairOriginalCoverage.value.format,THREE.RedFormat);
  h.setSize([300,400]);h.api.render(h.camera,true);
  for(const target of h.targets){assert.equal(target.width,300);assert.equal(target.height,400);assert.equal(target.depthBuffer,false);assert.equal(target.stencilBuffer,false);}
  let releases=0;for(const target of h.targets)target.addEventListener('dispose',()=>releases++);
  h.api.dispose();h.api.dispose();assert.equal(releases,2);h.dispose();
});
test('被覆の描画例外でも元の描画状態へ戻し下地を有効化しない',()=>{
  const h=harness();h.fail();assert.throws(()=>h.api.render(h.camera,true),/RT故障/);
  assert.equal(h.shader.uniforms.hairCoverageEnabled.value,0);
  assert.equal(h.state().target,null);assert.equal(h.state().scissor,true);assert.equal(h.state().clearAlpha,.3);h.dispose();
});
test('一過性の不正投影は次フレームへ送り継続時だけ明示失敗する',()=>{
  const recovered=harness();recovered.invalidateProjection();recovered.api.render(recovered.camera,true);
  assert.equal(recovered.calls.length,0);assert.equal(recovered.shader.uniforms.hairCoverageEnabled.value,0);
  recovered.restoreProjection();recovered.api.render(recovered.camera,true);assert.equal(recovered.calls.length,2);recovered.dispose();
  const failed=harness();failed.invalidateProjection();failed.api.render(failed.camera,true);failed.api.render(failed.camera,true);
  assert.throws(()=>failed.api.render(failed.camera,true),/3フレーム連続/);failed.dispose();
});
