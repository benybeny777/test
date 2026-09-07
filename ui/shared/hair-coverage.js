import * as THREE from './vendor/three/three.module.min.js';

// 元の所有画素だけを二値化する。GPU補間後の値には閾値をかけない。
export function semanticHairCoverage(rgba){
  if(rgba.length%4)throw new Error('髪被覆のRGBA寸法が不正です');
  const mask=new Uint8Array(rgba.length/4);
  for(let i=0;i<mask.length;i++)mask[i]=rgba[i*4+3]>0?255:0;
  return mask;
}

export function coveragePixelBounds(points,width,height){
  if(!points.length)return null;
  let left=Infinity,top=Infinity,right=-Infinity,bottom=-Infinity;
  for(const [x,y] of points){
    if(!Number.isFinite(x)||!Number.isFinite(y))throw new Error('髪被覆の投影座標が不正です');
    left=Math.min(left,x);top=Math.min(top,y);right=Math.max(right,x);bottom=Math.max(bottom,y);
  }
  left=Math.max(0,Math.floor(left)-2);top=Math.max(0,Math.floor(top)-2);
  right=Math.min(width,Math.ceil(right)+2);bottom=Math.min(height,Math.ceil(bottom)+2);
  return right>left&&bottom>top?{x:left,y:top,width:right-left,height:bottom-top}:null;
}

// 切詰めUV領域と交差する三角形の頂点を残す。変形後も安全側の外接領域になる。
function visibleVertices(geometry){
  const uv=geometry.attributes.uv,index=geometry.index,ids=new Set();
  for(let i=0;i<index.count;i+=3){
    const triangle=[index.getX(i),index.getX(i+1),index.getX(i+2)];
    const xs=triangle.map(n=>uv.getX(n)),ys=triangle.map(n=>uv.getY(n));
    if(Math.max(...xs)<0||Math.min(...xs)>1||Math.max(...ys)<0||Math.min(...ys)>1)continue;
    for(const id of triangle)ids.add(id);
  }
  return [...ids];
}

export function createHairCoverage(renderer,hair,hidden,commonPositions,image){
  const scratch=document.createElement('canvas');
  let mask,material,h0,ht,originalGeometry,movedGeometry;
  try{
    scratch.width=image.naturalWidth;scratch.height=image.naturalHeight;
    const ctx=scratch.getContext('2d');if(!ctx)throw new Error('髪被覆用Canvasを作成できません');
    ctx.drawImage(image,0,0);
    mask=new THREE.DataTexture(semanticHairCoverage(ctx.getImageData(0,0,scratch.width,scratch.height).data),scratch.width,scratch.height,THREE.RedFormat);
    mask.flipY=true;mask.minFilter=THREE.LinearMipmapLinearFilter;mask.magFilter=THREE.LinearFilter;mask.generateMipmaps=true;mask.needsUpdate=true;
    material=new THREE.ShaderMaterial({uniforms:{coverage:{value:mask}},
      vertexShader:'varying vec2 maskUv;void main(){maskUv=uv;gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.0);}',
      fragmentShader:'varying vec2 maskUv;uniform sampler2D coverage;void main(){if(any(lessThan(maskUv,vec2(0.0)))||any(greaterThan(maskUv,vec2(1.0))))discard;gl_FragColor=vec4(texture2D(coverage,maskUv).r,0.0,0.0,1.0);}',
      depthTest:false,depthWrite:false,blending:THREE.NoBlending});
    const target=()=>new THREE.WebGLRenderTarget(1,1,{format:THREE.RedFormat,type:THREE.UnsignedByteType,depthBuffer:false,stencilBuffer:false,minFilter:THREE.NearestFilter,magFilter:THREE.NearestFilter,generateMipmaps:false});
    h0=target();ht=target();
    // 描画中は髪と同じUV・索引・頂点バッファを借りる。解放は本体の全メッシュ破棄時だけ。
    const maskGeometry=positions=>{
      const geometry=new THREE.BufferGeometry();geometry.setIndex(hair.geometry.index);
      geometry.setAttribute('uv',hair.geometry.attributes.uv);geometry.setAttribute('position',positions);return geometry;
    };
    originalGeometry=maskGeometry(commonPositions);movedGeometry=maskGeometry(hair.geometry.attributes.position);
    const original=new THREE.Mesh(originalGeometry,material),moved=new THREE.Mesh(movedGeometry,material);
    for(const mesh of [original,moved]){mesh.matrixAutoUpdate=false;mesh.frustumCulled=false;}
    const originalScene=new THREE.Scene(),movedScene=new THREE.Scene();originalScene.add(original);movedScene.add(moved);
    const uniforms={hairOriginalCoverage:{value:h0.texture},hairMovedCoverage:{value:ht.texture},hairCoverageBounds:{value:new THREE.Vector4()},hairCoverageEnabled:{value:0}};
    const beforeCompile=hidden.material.onBeforeCompile,oldKey=hidden.material.customProgramCacheKey();
    hidden.material.onBeforeCompile=shader=>{
      beforeCompile(shader);Object.assign(shader.uniforms,uniforms);
      shader.fragmentShader='uniform sampler2D hairOriginalCoverage;uniform sampler2D hairMovedCoverage;uniform vec4 hairCoverageBounds;uniform float hairCoverageEnabled;\n'+shader.fragmentShader;
      const marker='#include <alphatest_fragment>';
      if(!shader.fragmentShader.includes(marker))throw new Error('露出下地のシェーダーが未対応です');
      shader.fragmentShader=shader.fragmentShader.replace(marker,`if(hairCoverageEnabled<0.5)discard;
        vec2 coverageUv=(gl_FragCoord.xy-hairCoverageBounds.xy)/hairCoverageBounds.zw;
        if(any(lessThan(coverageUv,vec2(0.0)))||any(greaterThan(coverageUv,vec2(1.0))))discard;
        diffuseColor.a*=max(0.0,texture2D(hairOriginalCoverage,coverageUv).r-texture2D(hairMovedCoverage,coverageUv).r);
        ${marker}`);
    };
    hidden.material.customProgramCacheKey=()=>oldKey+'-continuous-hair-coverage-v1';hidden.material.needsUpdate=true;
    const ids=visibleVertices(hidden.geometry),point=new THREE.Vector3(),size=new THREE.Vector2(),maskCamera=new THREE.OrthographicCamera();
    const viewport=new THREE.Vector4(),scissor=new THREE.Vector4(),clearColor=new THREE.Color();
    let disposed=false;
    return {
      render(camera,active){
        uniforms.hairCoverageEnabled.value=0;
        if(disposed||!active||!hidden.visible)return;
        renderer.getDrawingBufferSize(size);hidden.updateWorldMatrix(true,false);hair.updateWorldMatrix(true,false);camera.updateMatrixWorld();
        const projected=ids.map(id=>{
          point.fromBufferAttribute(commonPositions,id).applyMatrix4(hidden.matrixWorld).project(camera);
          return [(point.x+1)*size.x/2,(1-point.y)*size.y/2];
        });
        const roi=coveragePixelBounds(projected,size.x,size.y);if(!roi)return;
        if(roi.width>renderer.capabilities.maxTextureSize||roi.height>renderer.capabilities.maxTextureSize)throw new Error('髪被覆の表示解像度がGPU上限を超えています');
        if(h0.width!==roi.width||h0.height!==roi.height){h0.setSize(roi.width,roi.height);ht.setSize(roi.width,roi.height);}
        maskCamera.copy(camera);maskCamera.setViewOffset(size.x,size.y,roi.x,roi.y,roi.width,roi.height);
        original.matrix.copy(hair.matrixWorld);moved.matrix.copy(hair.matrixWorld);
        uniforms.hairCoverageBounds.value.set(roi.x,size.y-roi.y-roi.height,roi.width,roi.height);
        const previousTarget=renderer.getRenderTarget(),previousScissor=renderer.getScissorTest(),previousAlpha=renderer.getClearAlpha();
        renderer.getViewport(viewport);renderer.getScissor(scissor);renderer.getClearColor(clearColor);
        try{
          renderer.setScissorTest(false);renderer.setClearColor(0,0);
          renderer.setRenderTarget(h0);renderer.clear();renderer.render(originalScene,maskCamera);
          renderer.setRenderTarget(ht);renderer.clear();renderer.render(movedScene,maskCamera);
          uniforms.hairCoverageEnabled.value=1;
        }finally{
          renderer.setRenderTarget(previousTarget);renderer.setViewport(viewport);renderer.setScissor(scissor);renderer.setScissorTest(previousScissor);renderer.setClearColor(clearColor,previousAlpha);
        }
      },
      dispose(){if(disposed)return;disposed=true;uniforms.hairCoverageEnabled.value=0;h0.dispose();ht.dispose();mask.dispose();material.dispose();originalGeometry.dispose();movedGeometry.dispose();},
    };
  }catch(error){for(const resource of [mask,material,h0,ht,originalGeometry,movedGeometry])resource?.dispose();throw error;}
  finally{scratch.width=scratch.height=0;}
}
