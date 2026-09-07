import test from 'node:test';
import assert from 'node:assert/strict';
import {headDisplacement,armDisplacement,bodyBreathDisplacement,validateHiddenMotion,hiddenOffset,hiddenRepairAmount} from '../ui/shared/rig-motion.js';

test('耳境界の補修は中立と比較オフでゼロ、指定角度で上限になる',()=>{
  const profile={repair_layer:'scene_ear_repair',angle_limit:15};
  assert.equal(hiddenRepairAmount(profile,0,0),0);
  assert.equal(hiddenRepairAmount(profile,15,0,false),0);
  assert.equal(hiddenRepairAmount(profile,7.5,0),.5);
  assert.equal(hiddenRepairAmount(profile,-30,0),1);
  assert.equal(hiddenRepairAmount(undefined,15,15),0);
});

test('補完比較は中立と未補完領域を固定し、不正な変位定義を拒否する',()=>{
  const profile={version:1,weights:[0,.5,1],angle_limit:15,x_limit_px:10,y_limit_px:5};
  validateHiddenMotion(profile,3);
  assert.deepEqual(hiddenOffset(profile,2,0,0),[0,0]);
  assert.deepEqual(hiddenOffset(profile,0,15,15),[0,0]);
  assert.deepEqual(hiddenOffset(profile,1,30,-30),[5,-2.5]);
  assert.throws(()=>validateHiddenMotion({...profile,weights:[NaN]},1));
  assert.throws(()=>validateHiddenMotion(profile,4));
});

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
test('呼吸は肩と頭を一緒に動かし、胴体下端で0になる',()=>{
  const torso=[20,80,80,180],neck=[40,60,60,100];
  assert.notDeepEqual(bodyBreathDisplacement(50,70,torso,neck,100,200,1),[0,0]);
  assert.notEqual(bodyBreathDisplacement(20,100,torso,neck,100,200,1)[0],0);
  assert.deepEqual(bodyBreathDisplacement(50,180,torso,neck,100,200,1),[0,0]);
  assert.throws(()=>bodyBreathDisplacement(0,0,undefined,neck,100,200,1));
});
