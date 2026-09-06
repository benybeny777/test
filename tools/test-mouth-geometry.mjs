import assert from 'node:assert/strict';
import test from 'node:test';
import {mouthGeometry,drawMouth,lipMesh,MOUTH_PRESETS} from '../ui/shared/mouth-geometry.js';

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
