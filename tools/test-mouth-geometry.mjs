import assert from 'node:assert/strict';
import test from 'node:test';
import {mouthGeometry,drawMouth} from '../ui/shared/mouth-geometry.js';

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
