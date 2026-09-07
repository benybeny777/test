// Chromeの同一ページで3体を2巡する。描画状態以外の本番ファイルは変更しない。
import {writeFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {classifyErrors} from './switch-observation-errors.mjs';
import {createTempOutputDirectory} from './temp-output-directory.mjs';

const [list,output,url='http://127.0.0.1:8791/ui/check.html']=process.argv.slice(2);
const ids=(list??'').split(',');
if(ids.length!==3||new Set(ids).size!==3||ids.some(id=>!/^c_[0-9a-f]{12}$/.test(id)))throw new Error('異なる3キャラIDをカンマ区切りで指定してください');
if(!output)throw new Error('専用temp出力フォルダを指定してください');
const base=new URL(url);
if(base.protocol!=='http:'||!['127.0.0.1','localhost','[::1]'].includes(base.hostname)||base.pathname!='/ui/check.html')throw new Error('既存のローカルcheck.htmlだけを対象にしてください');
const destination=await createTempOutputDirectory(output);
const packagePath=process.env.LVS_PLAYWRIGHT_MODULE;
const {chromium}=await import(packagePath?pathToFileURL(packagePath).href:'playwright');
let browser,page,cdp,timer;
const started=Date.now(),timeoutMs=180000;
const report={ids,scope:'共通rendererの既存キャラ読込・破棄観測。最新3体の生成完了/画質判定ではない。OS RAM/VRAMは対象外。',timeoutMs,viewport:{width:1440,height:1050},samples:[],pageerrors:[],requestFailures:[],httpFailures:[],blockedExternalRequests:[],
  measurementLimits:['JSヒープはV8の計測値でVRAM/プロセス全体/OS全体メモリではない。GCを強制しないため増減だけでリーク判定しない。',
    'DOM画像数はdocumentのimgのみ。WebGLテクスチャ/非DOM画像/画像デコード総メモリではない。',
    'BlobURLはこのページで観測したcreate/revoke呼出しのみ。解放呼出しはメモリ回収完了の証明ではない。0件は利用なしであり、解放実証ではない。',
    '2巡は長時間安定性試験ではない。画質の合格を判定しない。']};
const pathOnly=value=>{try{const u=new URL(value);return ['http:','https:'].includes(u.protocol)?u.origin+u.pathname+(u.search?'?クエリ付き（互換対象外）':''):u.protocol+'(内容省略)';}catch{return '(不正URL)';}};
try{
  const task=(async()=>{
    browser=await chromium.launch({channel:'chrome',headless:true,timeout:30000});
    page=await browser.newPage({viewport:report.viewport,deviceScaleFactor:1});
    page.setDefaultTimeout(30000);
    page.on('pageerror',error=>report.pageerrors.push(error.message));
    page.on('requestfailed',request=>report.requestFailures.push({url:pathOnly(request.url()),error:request.failure()?.errorText}));
    page.on('response',response=>{if(response.status()>=400)report.httpFailures.push({url:pathOnly(response.url()),status:response.status()});});
    await page.route('**/*',async route=>{
      const target=new URL(route.request().url());
      if(['http:','https:'].includes(target.protocol)&&target.origin!==base.origin){
        report.blockedExternalRequests.push(pathOnly(target.href));await route.abort('blockedbyclient');
      }else await route.continue();
    });
    await page.addInitScript(()=>{
      // 観測専用。元のAPIへ同じthis/引数を渡し、生成物の内容やURL寿命を変えない。
      const create=URL.createObjectURL,revoke=URL.revokeObjectURL;
      const active=new Map();let created=0,revoked=0,unknownRevokes=0;
      URL.createObjectURL=function(...args){const value=Reflect.apply(create,this,args);active.set(value,++created);return value;};
      URL.revokeObjectURL=function(...args){const result=Reflect.apply(revoke,this,args);if(active.delete(String(args[0])))revoked++;else unknownRevokes++;return result;};
      Object.defineProperty(window,'__lvsSwitchBlobObservation',{value:()=>({created,revoked,unknownRevokes,activeIds:[...active.values()]})});
    });
    cdp=await page.context().newCDPSession(page);
    await cdp.send('Performance.enable');
    base.searchParams.set('character',ids[0]);await page.goto(base.href,{waitUntil:'domcontentloaded'});
    let previousActive=[];
    for(let round=0;round<2;round++)for(let index=0;index<ids.length;index++){
      const id=ids[index];
      if(round||index)await page.locator('#character').selectOption(id);
      await page.waitForFunction(expected=>{
        const state=document.querySelector('#status')?.textContent??'';
        const source=document.querySelector('#source');
        return state!=='読込中'&&(state.startsWith('素材充足:')?
          document.querySelector('#character')?.value===expected&&source?.src.includes('/'+expected+'/source/input.png')&&source.complete&&source.naturalWidth>0:true);
      },id);
      const status=await page.locator('#status').textContent();
      if(!status.startsWith('素材充足:'))throw new Error('キャラ読込失敗: '+id+' '+status);
      await page.evaluate(()=>new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done))));
      const metrics=Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(item=>[item.name,item.value]));
      const counters=await cdp.send('Memory.getDOMCounters');
      const dom=await page.evaluate(()=>({elements:document.querySelectorAll('*').length,images:document.images.length,
        loadedImages:[...document.images].filter(image=>image.complete&&image.naturalWidth>0).length,
        canvasCount:document.querySelectorAll('canvas').length,blob:window.__lvsSwitchBlobObservation()}));
      const {blob,...counts}=dom;
      const sample={round:round+1,id,status,elapsedSeconds:(Date.now()-started)/1000,
        jsHeapUsedMB:Number.isFinite(metrics.JSHeapUsedSize)?metrics.JSHeapUsedSize/1e6:null,
        jsHeapTotalMB:Number.isFinite(metrics.JSHeapTotalSize)?metrics.JSHeapTotalSize/1e6:null,
        dom:counts,cdpDOM:counters,blob:{...blob,previousActiveRemaining:previousActive.filter(value=>blob.activeIds.includes(value)).length},
        pageerrorCount:report.pageerrors.length,requestFailureCount:report.requestFailures.length,httpFailureCount:report.httpFailures.length};
      previousActive=blob.activeIds;report.samples.push(sample);
      await writeFile(resolve(destination,'report.json'),JSON.stringify(report,null,2));
      console.log(JSON.stringify(sample));
      if(sample.blob.previousActiveRemaining)throw new Error('切替後に旧キャラのBlob URLが残っています');
      if(counts.canvasCount!==1||counts.images!==1||counts.loadedImages!==1)throw new Error('表示Canvas/原画画像の数または読込状態が不正です');
      const first=report.samples.find(item=>item.id===id);
      if(first!==sample&&JSON.stringify(first.dom)!==JSON.stringify(sample.dom))throw new Error('同じキャラの2巡目でDOM構成が変わりました');
    }
    if(report.samples.length!==6)throw new Error('全3体2巡の表示が完了していません');
    report.classification=classifyErrors(report,base.origin);
    report.status=(report.pageerrors.length||report.classification.unexpectedRequestFailures.length||report.classification.unexpectedHttpFailures.length||report.blockedExternalRequests.length)?'observed_errors':'completed';
    if(report.status==='observed_errors')throw new Error('取得/実行エラーを観測しました。report.jsonを確認してください');
  })();
  await Promise.race([task,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('全体180秒タイムアウト')),timeoutMs);})]);
}catch(error){report.status='failed';report.failure=error.message;process.exitCode=1;}
finally{
  report.classification=classifyErrors(report,base.origin);
  clearTimeout(timer);
  try{if(cdp)await cdp.detach();}catch(error){report.cdpCleanupError=error.message;report.status='failed';process.exitCode=1;}
  try{if(page)await page.close();}catch(error){report.pageCleanupError=error.message;report.status='failed';process.exitCode=1;}
  try{if(browser)await browser.close();}catch(error){report.browserCleanupError=error.message;report.status='failed';process.exitCode=1;}
  report.elapsedSeconds=(Date.now()-started)/1000;
  await writeFile(resolve(destination,'report.json'),JSON.stringify(report,null,2));
}
