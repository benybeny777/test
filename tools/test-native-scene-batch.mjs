import test from 'node:test';
import assert from 'node:assert/strict';
import {planSceneBatches,createNativeSceneBatch,copyPixelRows} from '../ui/shared/native-scene-batch.js';

// GPU/ブラウザを起動せず、原寸Canvasのover合成と読出し範囲を模擬する。
function canvasMock(){
  let width=0,height=0,pixels=new Uint8ClampedArray();const reads=[];
  const context={
    drawImage(image,x,y){
      for(let row=0;row<image.height;row++)for(let col=0;col<image.width;col++){
        const dx=x+col,dy=y+row;if(dx<0||dy<0||dx>=width||dy>=height)continue;
        const s=(row*image.width+col)*4,d=(dy*width+dx)*4,a=image.pixels[s+3]/255,b=pixels[d+3]/255;
        const alpha=a+b*(1-a);
        for(let c=0;c<3;c++)pixels[d+c]=alpha?(image.pixels[s+c]*a+pixels[d+c]*b*(1-a))/alpha:0;
        pixels[d+3]=alpha*255;
      }
    },
    getImageData(x,y,w,h){
      assert.equal(x,0);assert.equal(y,0);assert.equal(w,width);assert.equal(h,height);
      reads.push([w,h]);return {data:pixels.slice()};
    }
  };
  return {reads,get width(){return width;},set width(v){width=v;pixels=new Uint8ClampedArray(width*height*4);},
    get height(){return height;},set height(v){height=v;pixels=new Uint8ClampedArray(width*height*4);},getContext:()=>context};
}
const image=(width,height,color)=>({width,height,pixels:Uint8ClampedArray.from(Array.from({length:width*height},()=>color).flat())});

test('生成下地と独立髪を越えず連続した共通変位群だけを合成する',()=>{
  const graph=['torso','neck','hidden_face','face','hair','residual'].map(role=>({role,layer:'scene_'+role}));
  assert.deepEqual(planSceneBatches(graph,true).map(b=>b.parts.map(p=>p.role)),[['torso','neck'],['hidden_face'],['face'],['hair'],['residual']]);
  assert.deepEqual(planSceneBatches(graph,false).map(b=>b.parts.map(p=>p.role)),[['torso','neck'],['hidden_face'],['face','hair','residual']]);
});
test('254の相補境界は原寸合成後の補間で254を保つ',()=>{
  const face=image(2,1,[80,120,160,254]),hair=image(2,1,[80,120,160,254]);
  face.pixels[7]=0;hair.pixels[3]=0;
  const layers={face:{texture_box:[0,0,2,1]},hair:{texture_box:[0,0,2,1]}};
  const canvas=canvasMock(),before=face.pixels.slice();
  const batch=createNativeSceneBatch([{role:'face',layer:'face'},{role:'hair',layer:'hair'}],layers,new Map([['face',face],['hair',hair]]),()=>canvas);
  assert.deepEqual([...batch.data],[80,120,160,254,80,120,160,254]);
  assert.equal((batch.data[3]+batch.data[7])/2,254);
  assert.deepEqual(face.pixels,before);assert.equal(canvas.width,0);batch.dispose();
});
test('顔の更新は原寸の顔周辺だけを再合成し、離れた行の色を変えない',()=>{
  const base=image(20,10,[10,20,30,254]),face=image(2,2,[100,110,120,255]);
  const layers={base:{texture_box:[30,40,50,50]},face:{texture_box:[38,44,40,46]}};
  const canvas=canvasMock();
  const batch=createNativeSceneBatch([{role:'torso',layer:'base'},{role:'face',layer:'face'}],layers,new Map([['base',base],['face',face]]),()=>canvas);
  const before=batch.data.slice();batch.update(image(2,2,[200,210,220,255]));
  assert.deepEqual(batch.box,[30,40,50,50]);assert.deepEqual(canvas.reads,[[20,10],[6,6]]);
  assert.deepEqual(batch.data.slice(0,20*4),before.slice(0,20*4));
  assert.deepEqual([...batch.data.slice((4*20+8)*4,(4*20+8)*4+4)],[200,210,220,255]);
  batch.dispose();assert.equal(canvas.height,0);assert.throws(()=>batch.update(face),/破棄済み/);
});
test('部分行コピーが画像外へ書き込まない',()=>{
  const target=new Uint8Array(16),patch=new Uint8Array([1,2,3,4]);
  copyPixelRows(target,2,patch,1,1,1,1);assert.deepEqual([...target.slice(12)],[1,2,3,4]);
  assert.throws(()=>copyPixelRows(target,2,patch,1,1,2,1),/範囲/);
});
