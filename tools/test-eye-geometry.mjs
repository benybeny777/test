import assert from 'node:assert/strict';
import test from 'node:test';
import {eyeAperture} from '../ui/shared/eye-geometry.js';
const layer={eye_aperture:[[10,20,30,27],[15,18,33,29],[20,20,30,27]]};
test('完全閉眼で隙間がなく、全開では実測領域へ戻る',()=>{
  const shut=eyeAperture(layer,0),open=eyeAperture(layer,1);
  for(let i=0;i<shut.length;i++) {
    assert.equal(shut[i][1],shut[i][2]);
    assert.deepEqual(open[i],layer.eye_aperture[i]);
  }
});
test('まばたき途中の領域は連続かつ交差しない',()=>{
  for(let step=0;step<=100;step++)for(const [,top,bottom] of eyeAperture(layer,step/100))assert.ok(top<=bottom);
});
test('未測定や逆順の境界で黙って固定位置へ降格しない',()=>{
  assert.throws(()=>eyeAperture({},1));
  assert.throws(()=>eyeAperture({eye_aperture:[layer.eye_aperture[1],layer.eye_aperture[0]]},1));
});
