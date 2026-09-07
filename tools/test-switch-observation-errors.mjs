import test from 'node:test';
import assert from 'node:assert/strict';
import {classifyErrors} from './switch-observation-errors.mjs';
const origin='http://127.0.0.1:8791',id='c_123456789abc',url=origin+'/api/characters/'+id+'/snapshot';
test('表示成功した同じsnapshotの実404とbody cancelだけを分ける',()=>{
  const value=classifyErrors({samples:[{id,status:'素材充足: incomplete'}],httpFailures:[{url,status:404}],requestFailures:[{url,error:'net::ERR_ABORTED'}]},origin);
  assert.equal(value.expectedLegacyResponses.length,1);assert.equal(value.expectedLegacyBodyCancellations.length,1);
  assert.equal(value.unexpectedRequestFailures.length,0);
});
test('一般404・snapshot500・画像abort・未観測404・別URLは失敗を維持',()=>{
  const result=classifyErrors({samples:[{id,status:'素材充足: incomplete'}],httpFailures:[{url,status:500},{url:origin+'/image.png',status:404}],requestFailures:[{url,error:'net::ERR_ABORTED'},{url:origin+'/image.png',error:'net::ERR_ABORTED'}]},origin);
  assert.equal(result.expectedLegacyResponses.length,0);assert.equal(result.unexpectedHttpFailures.length,2);assert.equal(result.unexpectedRequestFailures.length,2);
});
test('未表示・異なるキャラ・非abortエラーは互換へ落とさない',()=>{
  const result=classifyErrors({samples:[],httpFailures:[{url,status:404}],requestFailures:[{url,error:'net::ERR_ABORTED'}]},origin);
  assert.equal(result.unexpectedHttpFailures.length,1);assert.equal(result.unexpectedRequestFailures.length,1);
  const other=classifyErrors({samples:[{id,status:'素材充足: incomplete'}],httpFailures:[{url,status:404}],requestFailures:[{url,error:'net::ERR_FAILED'}]},origin);
  assert.equal(other.unexpectedRequestFailures.length,1);
});
test('query付きsnapshotや別snapshotの404は同じURLとして扱わない',()=>{
  const queried=url+'?v=2',other=origin+'/api/characters/c_abcdef123456/snapshot';
  const result=classifyErrors({samples:[{id,status:'素材充足: incomplete'}],httpFailures:[{url:queried,status:404},{url:other,status:404}],requestFailures:[{url,error:'net::ERR_ABORTED'}]},origin);
  assert.equal(result.expectedLegacyResponses.length,0);assert.equal(result.unexpectedHttpFailures.length,2);assert.equal(result.unexpectedRequestFailures.length,1);
});
