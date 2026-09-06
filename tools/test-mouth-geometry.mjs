import assert from 'node:assert/strict';
import test from 'node:test';
import {mouthGeometry,drawMouth,lipMesh,localLipStrips,upperTeethGeometry,drawTexturedMouth,MOUTH_PRESETS} from '../ui/shared/mouth-geometry.js';

test('2軸の全域で輪郭と口角が有限で上下が交差しない',()=>{
  for(let o=0;o<=100;o++)for(let f=-100;f<=100;f++) {
    const g=mouthGeometry([20,30,50,40],o/100,f/100);
    for(const key of ['cx','cy','width','half','height'])assert.ok(Number.isFinite(g[key]));
    assert.ok(g.upper[0][1]<=g.lower[1][1]);
    assert.equal(g.left[1],g.right[1]);
  }
});
test('母音は横幅だけでなく縦横比と曲率も変わる',()=>{
  const wide=mouthGeometry([0,0,30,10],.5,1),round=mouthGeometry([0,0,30,10],.5,-1);
  assert.ok(wide.half>round.half);assert.ok(wide.height<round.height);
  assert.notEqual(wide.upper[0][0]/wide.half,round.upper[0][0]/round.half);
  assert.equal(mouthGeometry([0,0,30,10],0,0).height,0);
});
test('原画全体を平行移動しても口の相対形状は変わらない',()=>{
  const a=mouthGeometry([0,0,30,10],.5,.4),b=mouthGeometry([100,200,130,210],.5,.4);
  assert.equal(a.half,b.half);assert.equal(b.cx-a.cx,100);assert.equal(b.cy-a.cy,200);
});
test('不正な測定値を固定座標で補わない',()=>{
  for(const box of [null,[0,0,0,0],[0,0,NaN,10]])assert.throws(()=>mouthGeometry(box,0,0));
  assert.throws(()=>mouthGeometry([0,0,30,10],NaN,0));
  assert.throws(()=>drawMouth({},[0,0,30,10],0,0,null));
});

test('あは十分に開き、他の固定母音へ開口補正を漏らさない',()=>{
  const layer={texture_box:[0,0,100,100],feature_box:[25,35,75,55],lip_seam:[[25,45],[50,45],[75,45]]};
  for(const [name,[open,form]] of Object.entries(MOUTH_PRESETS)) {
    const mesh=lipMesh(layer,open,form),gap=mesh.lower[2][0][1]-mesh.upper[2][1][1];
    const roundness=Math.max(0,-form);
    const expected=name==='a'?50*.58:50*(.42+.12*roundness)*open*(1-.2*form);
    assert.ok(Math.abs(gap-expected)<1e-9,name);
  }
});

test('上下唇メッシュは中立時に原画と一致し、外周を動かさない',()=>{
  const layer={texture_box:[0,0,70,60],feature_box:[20,20,50,40],lip_seam:[[20,29],[35,31],[50,29]]};
  const neutral=lipMesh(layer,0,0);
  for(let i=0;i<neutral.source.length;i++) {
    assert.deepEqual(neutral.upper[i],neutral.source[i].slice(0,2));
    assert.deepEqual(neutral.lower[i],neutral.source[i].slice(1));
  }
  for(const form of [-1,0,1])for(const open of [0,.5,1]) {
    const mesh=lipMesh(layer,open,form);
    for(let i=0;i<mesh.source.length;i++) {
      assert.deepEqual(mesh.upper[i][0],mesh.source[i][0]);
      assert.deepEqual(mesh.lower[i][1],mesh.source[i][2]);
      assert.ok(mesh.upper[i][1][1]<=mesh.lower[i][0][1]);
    }
  }
});

test('すぼめた口は笑った口角を弱め、横引きより丸い縦横比になる',()=>{
  const layer={texture_box:[0,0,100,100],feature_box:[25,35,75,55],lip_seam:[[25,39],[37.5,43],[50,49],[62.5,43],[75,39]]};
  const wide=lipMesh(layer,.7,.8),round=lipMesh(layer,.7,-.8);
  const dimensions=mesh=>{
    const upper=mesh.upper.slice(1,-1).map(column=>column[1]),lower=mesh.lower.slice(1,-1).map(column=>column[0]);
    return {width:upper.at(-1)[0]-upper[0][0],height:lower[2][1]-upper[2][1]};
  };
  const a=dimensions(wide),b=dimensions(round);
  assert.ok(b.width<a.width);assert.ok(b.height/b.width>a.height/a.width);
  const puckered=lipMesh(layer,0,-1);
  for(const column of puckered.upper.slice(1,-1))assert.equal(column[1][1],45);
});

test('上歯は閉口で消え、開口の内側に収まり、すぼめで露出が減る',()=>{
  const layer={texture_box:[0,0,100,100],feature_box:[25,35,75,55],lip_seam:[[25,45],[37.5,45],[50,45],[62.5,45],[75,45]]};
  assert.equal(upperTeethGeometry(lipMesh(layer,0,0)),null);
  for(const [opening,form] of Object.values(MOUTH_PRESETS)){
    const mesh=lipMesh(layer,opening,form),teeth=upperTeethGeometry(mesh);if(!teeth)continue;
    assert.ok(teeth.opacity>0&&teeth.opacity<=1);
    for(let i=0;i<teeth.top.length;i++){
      const [x,y]=teeth.top[i];assert.ok(Number.isFinite(x)&&Number.isFinite(y));
      assert.ok(x>=mesh.upper[1][1][0]&&x<=mesh.upper.at(-2)[1][0]);
      assert.ok(teeth.bottom[i][1]>=y);
      // 帯の下端も上下唇間の同じ線形補間に収まる。
      const j=mesh.upper.findIndex((column,index)=>index>0&&column[1][0]>=x);
      const a=mesh.upper[j-1][1],b=mesh.upper[j][1],t=(x-a[0])/(b[0]-a[0]);
      const upper=a[1]+(b[1]-a[1])*t;
      const lower=mesh.lower[j-1][0][1]+(mesh.lower[j][0][1]-mesh.lower[j-1][0][1])*t;
      assert.ok(y>=upper-1e-9);assert.ok(teeth.bottom[i][1]<=lower+1e-9);
    }
  }
  const wide=upperTeethGeometry(lipMesh(layer,.7,.8)),round=upperTeethGeometry(lipMesh(layer,.7,-.8));
  assert.ok(round.top.at(-1)[0]-round.top[0][0]<wide.top.at(-1)[0]-wide.top[0][0]);
  assert.ok(round.bottom[8][1]-round.top[8][1]<wide.bottom[8][1]-wide.top[8][1]);
  assert.ok(round.opacity<wide.opacity);
});

test('原画閉口は画像だけを描き、上歯は口内クリップ後・原画唇より前に描く',()=>{
  const layer={texture_box:[0,0,100,100],feature_box:[25,35,75,55],lip_seam:[[25,45],[50,45],[75,45]]};
  const events=[],stack=[];
  const ctx={globalAlpha:1,save(){stack.push(this.globalAlpha);},restore(){this.globalAlpha=stack.pop();},
    beginPath(){},moveTo(){},lineTo(){},closePath(){},ellipse(){},transform(){},
    createLinearGradient(){return {stops:[],addColorStop(offset,color){this.stops.push([offset,color]);}};},
    clip(){events.push('clip');},fill(){events.push(this.fillStyle);},drawImage(){events.push('image');}};
  drawTexturedMouth(ctx,{},layer,0,0,[80,30,40]);assert.deepEqual(events,['image']);events.length=0;
  const before=JSON.stringify(layer);drawTexturedMouth(ctx,{},layer,1,0,[80,30,40]);
  const tooth=events.findIndex(event=>event?.stops?.some(([,color])=>color==='rgb(239,230,214)'));
  assert.ok(tooth>events.indexOf('clip'));assert.ok(tooth<events.lastIndexOf('image'));
  assert.equal(events[tooth].stops.length,3);assert.notEqual(events[tooth].stops[0][1],events[tooth].stops[1][1]);
  assert.equal(ctx.globalAlpha,1);assert.equal(stack.length,0);assert.equal(JSON.stringify(layer),before);
});

test('広い口切出しでも鼻下まで引かず、局所アンカーで原画座標へ戻る',()=>{
  const layer={texture_box:[427,415,633,572],feature_box:[483,471,577,516],lip_seam:[[483,497],[530,494],[577,473.5]]};
  const original=JSON.stringify(layer);
  for(const [open,form] of Object.values(MOUTH_PRESETS)){
    const mesh=lipMesh(layer,open,form),strips=localLipStrips(mesh);
    for(let i=0;i<mesh.source.length;i++){
      assert.deepEqual(strips.sourceUpper[i][0],strips.targetUpper[i][0]);
      assert.deepEqual(strips.sourceLower[i][3],strips.targetLower[i][3]);
      assert.ok(strips.sourceUpper[i][0][1]>450,'鼻下の上側は変形範囲に含めない');
      assert.ok(strips.sourceLower[i][3][1]<=572);
      assert.deepEqual(strips.targetUpper[i][3],mesh.upper[i][1]);
      assert.deepEqual(strips.targetLower[i][0],mesh.lower[i][0]);
      for(const columns of [strips.targetUpper,strips.targetLower])for(let row=1;row<4;row++)
        assert.ok(columns[i][row][1]>=columns[i][row-1][1],'縦の帯を折り返さない');
    }
  }
  assert.equal(JSON.stringify(layer),original);
  const neutral=localLipStrips(lipMesh(layer,0,0));
  assert.deepEqual(neutral.sourceUpper,neutral.targetUpper);assert.deepEqual(neutral.sourceLower,neutral.targetLower);
});
