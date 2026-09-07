import test from 'node:test';
import assert from 'node:assert/strict';
import {bleedTransparentRgb} from '../ui/shared/texture-alpha.js';

test('透明色だけを延長し可視画素とアルファを保持する',()=>{
  const data=new Uint8Array(5*3*4);data.set([180,120,90,64],6*4);
  const before=data.slice();bleedTransparentRgb(data,5,3);
  assert.deepEqual(data.slice(24,28),before.slice(24,28));
  for(let i=0;i<15;i++){
    assert.equal(data[i*4+3],before[i*4+3]);
    assert.deepEqual([...data.slice(i*4,i*4+3)],[180,120,90]);
  }
});
test('空素材と不正寸法は明示失敗する',()=>{
  assert.throws(()=>bleedTransparentRgb(new Uint8Array(4),1,1));
  assert.throws(()=>bleedTransparentRgb(new Uint8Array(4),2,1));
});
