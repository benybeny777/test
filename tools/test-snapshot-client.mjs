import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {consumeSnapshot} from '../ui/shared/snapshot-client.js';
import {SnapshotHash} from '../ui/shared/snapshot-sha256.js';
test('逐次SHAは空/ブロック境界/多チャンクでNode標準と一致する',()=>{
  for(const size of [0,1,55,56,63,64,65,127,128,1000000]){
    const data=Buffer.alloc(size);for(let i=0;i<size;i++)data[i]=i%251;
    const hash=new SnapshotHash();for(let i=0;i<size;i+=37)hash.update(data.subarray(i,i+37));
    assert.equal(hash.hex(),createHash('sha256').update(data).digest('hex'));
  }
});
const limits={record_bytes:2048,chunk_bytes:256,part_bytes:4096,total_bytes:8192,parts:10,dimension:100};
function fixture(){
  const generation='g_'+'1'.repeat(32),rig=Buffer.from(JSON.stringify({layers:{face:{url:'/assets/rig2d/parts/face.png',texture_box:[0,0,4,5]}}})),png=Buffer.from('image fixture');
  const sha=b=>createHash('sha256').update(b).digest('hex');
  const list=[{type:'snapshot',schema_version:1,generation,rig_sha256:sha(rig),parts:1,total:rig.length+png.length}];
  for(const [name,data,mime,size] of [['rig',rig,'application/json',null],['face',png,'image/png',[4,5]]])list.push({type:'begin',name,mime,size,length:data.length},{type:'chunk',name,data:data.toString('base64')},{type:'end',name,sha256:sha(data)});
  list.push({type:'complete',generation});return list;
}
function response(list){const data=new TextEncoder().encode(list.map(v=>JSON.stringify(v)+'\n').join(''));return new Response(new ReadableStream({start(c){for(let i=0;i<data.length;i+=7)c.enqueue(data.slice(i,i+7));c.close();}}));}
test('細かい受信境界をまたいでも一世代を固定し明示disposeする',async()=>{
  const revoked=[],result=await consumeSnapshot(response(fixture()),limits,{createUrl:()=>`blob:${Math.random()}`,revokeUrl:u=>revoked.push(u)});
  assert.equal(result.generation,'g_'+'1'.repeat(32));assert.equal(Object.keys(result.partUrls).length,1);assert.equal(revoked.length,0);
  result.dispose();result.dispose();assert.equal(revoked.length,2);
});
test('完了欠落時は新規URLをすべて解放し表示用結果を返さない',async()=>{
  const list=fixture();list.pop();const revoked=[];
  await assert.rejects(consumeSnapshot(response(list),limits,{createUrl:()=>`blob:${Math.random()}`,revokeUrl:u=>revoked.push(u)}),/完了/);
  assert.equal(revoked.length,2);
});
test('素材SHAと総量上限を検査する',async()=>{
  const list=fixture();list[6].sha256='0'.repeat(64);
  await assert.rejects(consumeSnapshot(response(list),limits),/SHA/);
  await assert.rejects(consumeSnapshot(response(fixture()),{...limits,total_bytes:1}),/ヘッダ/);
});
test('中断は入力readerを解放する',async()=>{
  const controller=new AbortController();let cancelled=false;
  const pending=consumeSnapshot(new Response(new ReadableStream({cancel(){cancelled=true;}})),limits,{signal:controller.signal});
  controller.abort();await assert.rejects(pending);assert.equal(cancelled,true);
});
