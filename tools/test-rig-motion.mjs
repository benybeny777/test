import test from 'node:test';
import assert from 'node:assert/strict';
import {headDisplacement,armDisplacement} from '../ui/shared/rig-motion.js';

test('腕を動かしても腕の下の脚や反対側へ変形を漏らさない',()=>{
  const arm=[5,30,30,80];
  assert.deepEqual(armDisplacement(20,120,arm,50,100,10),[0,0]);
  assert.deepEqual(armDisplacement(80,70,arm,50,100,10),[0,0]);
  assert.notEqual(armDisplacement(20,70,arm,50,100,10)[0],0);
});

test('中立で原画座標を変えず首の下で回転が消える',()=>{
  const f=[30,10,70,70],n=[40,65,60,90];
  assert.deepEqual(headDisplacement(45,40,f,n,100,200,0,0,0),[0,0]);
  for(const y of [90,100,200])assert.deepEqual(headDisplacement(45,y,f,n,100,200,10,5,10),[0,0]);
});
test('同じ原画を平行移動しても相対変形が一致する',()=>{
  const original=headDisplacement(45,60,[30,10,70,70],[40,65,60,90],100,200,0,0,10);
  const moved=headDisplacement(65,90,[50,40,90,100],[60,95,80,120],100,200,0,0,10);
  assert.deepEqual(original,moved);
});
test('首の接続点の両側で変形が連続する',()=>{
  for(const boundary of [70,90]){
    const a=headDisplacement(45,boundary-1e-5,[30,10,70,70],[40,65,60,90],100,200,10,5,10);
    const b=headDisplacement(45,boundary+1e-5,[30,10,70,70],[40,65,60,90],100,200,10,5,10);
    assert.ok(Math.hypot(a[0]-b[0],a[1]-b[1])<1e-4);
  }
});
