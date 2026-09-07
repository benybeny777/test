import assert from 'node:assert/strict';
import test from 'node:test';
import {eyeAperture,automaticBlinkOpen,AUTOMATIC_BLINK_DURATION_MS} from '../ui/shared/eye-geometry.js';
const layer={eye_aperture:[[10,20,30,27],[15,18,33,29],[20,20,30,27]]};
test('編集閉眼は全開の原画座標を保ち、測定した閉眼曲線へ連続して閉じる',()=>{
  const edited={...layer,closed_curve:[25,28,25],closed_material:true};
  assert.deepEqual(eyeAperture(edited,1),layer.eye_aperture);
  for(const [i,point] of eyeAperture(edited,0).entries()){
    assert.equal(point[1],edited.closed_curve[i]);assert.equal(point[2],point[1]);
    assert.equal(point[3],layer.eye_aperture[i][3]);
  }
  for(let n=0;n<=100;n++)for(const [,top,bottom] of eyeAperture(edited,n/100))assert.ok(top<=bottom);
  assert.throws(()=>eyeAperture({...edited,closed_curve:[1]},0));
  assert.throws(()=>eyeAperture({...layer,closed_material:true},0));
});
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
test('自動まばたきは低フレームレートでも完全閉眼を保持する',()=>{
  assert.equal(automaticBlinkOpen(0),1);
  assert.equal(automaticBlinkOpen(180),0);
  assert.equal(automaticBlinkOpen(300),0);
  assert.equal(automaticBlinkOpen(400),0);
  assert.equal(automaticBlinkOpen(AUTOMATIC_BLINK_DURATION_MS),1);
  assert.ok(automaticBlinkOpen(90)>0&&automaticBlinkOpen(90)<1);
  assert.ok(automaticBlinkOpen(525)>0&&automaticBlinkOpen(525)<1);
  assert.throws(()=>automaticBlinkOpen(Number.NaN));
});
