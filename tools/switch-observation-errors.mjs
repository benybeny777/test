// 旧比較の明示snapshot互換だけを区別し、他の取得失敗を正常化しない。
export function classifyErrors(report,origin){
  const valid=new Set(report.samples.filter(sample=>sample.status?.startsWith('素材充足:')).map(sample=>origin+'/api/characters/'+sample.id+'/snapshot'));
  const snapshot=url=>{try{const u=new URL(url);return u.origin===origin&&/^\/api\/characters\/c_[0-9a-f]{12}\/snapshot$/.test(u.pathname)&&!u.search&&!u.hash;}catch{return false;}};
  const compatible=new Set(report.httpFailures.filter(item=>item.status===404&&snapshot(item.url)&&valid.has(item.url)).map(item=>item.url));
  return {
    expectedLegacyResponses:report.httpFailures.filter(item=>item.status===404&&compatible.has(item.url)),
    expectedLegacyBodyCancellations:report.requestFailures.filter(item=>item.error==='net::ERR_ABORTED'&&compatible.has(item.url)),
    unexpectedHttpFailures:report.httpFailures.filter(item=>!(item.status===404&&compatible.has(item.url))),
    unexpectedRequestFailures:report.requestFailures.filter(item=>!(item.error==='net::ERR_ABORTED'&&compatible.has(item.url))),
  };
}
