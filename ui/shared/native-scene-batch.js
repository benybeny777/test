import {bleedTransparentRgb} from './texture-alpha.js';

// 順序を保った連続区間だけをまとめる。生成下地と独立移動する髪を越えない。
export function planSceneBatches(graph,independentHair){
  const batches=[];
  for(const part of graph){
    const separate=part.role==='hidden_face'||(independentHair&&part.role==='hair');
    if(separate)batches.push({separate:true,parts:[part]});
    else if(batches.length&&!batches.at(-1).separate)batches.at(-1).parts.push(part);
    else batches.push({separate:false,parts:[part]});
  }
  return batches;
}

export function copyPixelRows(target,width,patch,patchWidth,patchHeight,x,y){
  if(!Number.isInteger(width)||width<=0||target.length%(width*4)||
     ![patchWidth,patchHeight,x,y].every(Number.isInteger)||patchWidth<=0||patchHeight<=0||x<0||y<0||
     x+patchWidth>width||(y+patchHeight)*width*4>target.length||patch.length!==patchWidth*patchHeight*4)
    throw new Error('描画バッチの部分更新範囲が不正です');
  for(let row=0;row<patchHeight;row++)target.set(patch.subarray(row*patchWidth*4,(row+1)*patchWidth*4),((y+row)*width+x)*4);
}

export function sceneBatchBox(parts,layers){
  const boxes=parts.map(part=>layers[part.layer].texture_box);
  return [Math.min(...boxes.map(b=>b[0])),Math.min(...boxes.map(b=>b[1])),
    Math.max(...boxes.map(b=>b[2])),Math.max(...boxes.map(b=>b[3]))];
}

// 原寸でover合成してから一度だけGPU補間する。元レイヤー・リグは変更しない。
export function createNativeSceneBatch(parts,layers,images,createCanvas=()=>document.createElement('canvas')){
  const box=sceneBatchBox(parts,layers),width=box[2]-box[0],height=box[3]-box[1];
  const scratch=createCanvas(),ctx=scratch.getContext('2d');
  if(!ctx)throw new Error('原寸合成用Canvasを作成できません');
  function compose(region,face){
    scratch.width=region[2]-region[0];scratch.height=region[3]-region[1];
    for(const part of parts){
      const b=layers[part.layer].texture_box;
      if(b[2]<=region[0]||b[3]<=region[1]||b[0]>=region[2]||b[1]>=region[3])continue;
      ctx.drawImage(part.role==='face'&&face?face:images.get(part.layer),b[0]-region[0],b[1]-region[1]);
    }
    const pixels=ctx.getImageData(0,0,scratch.width,scratch.height).data;
    return bleedTransparentRgb(pixels,scratch.width,scratch.height);
  }
  let data;
  try{
    const pixels=compose(box);
    data=new Uint8Array(pixels.buffer,pixels.byteOffset,pixels.byteLength);
  }finally{scratch.width=scratch.height=0;}
  const face=parts.find(part=>part.role==='face');
  let disposed=false;
  return {box,width,height,data,
    update(faceCanvas){
      if(disposed)throw new Error('破棄済みの描画バッチは更新できません');
      if(!face)throw new Error('顔のない描画バッチは部分更新できません');
      const b=layers[face.layer].texture_box;
      // 顔周辺の透明RGBだけを更新する。全身のBFS・Canvas読出しはロード時だけ。
      const region=[Math.max(box[0],b[0]-2),Math.max(box[1],b[1]-2),Math.min(box[2],b[2]+2),Math.min(box[3],b[3]+2)];
      const pixels=compose(region,faceCanvas);
      copyPixelRows(data,width,pixels,region[2]-region[0],region[3]-region[1],region[0]-box[0],region[1]-box[1]);
    },
    dispose(){disposed=true;scratch.width=scratch.height=0;}
  };
}
