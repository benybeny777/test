// 作業用Chromeで共通確認画面を撮影する。生成素材を描き替えたり拡大保存したりしない。
import {mkdir,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const args=process.argv.slice(2),video=args.includes('--video');
if(args.some(arg=>arg.startsWith('--')&&arg!=='--video'))throw new Error('未対応の撮影オプションです');
const [id,output='temp/character-captures']=args.filter(arg=>arg!=='--video');
if(!/^c_[0-9a-f]{12}$/.test(id??''))throw new Error('確認対象のキャラIDを指定してください');
const destination=resolve(output);
const tempRoot=resolve('temp');
if(!destination.startsWith(tempRoot+'\\')&&!destination.startsWith(tempRoot+'/'))throw new Error('撮影先はtemp内に限定します');
const previewUrl=process.env.LVS_PREVIEW_URL??'http://127.0.0.1:8791/ui/check.html';
if(!/^http:\/\/127\.0\.0\.1:\d+\/ui\/check\.html$/.test(previewUrl))throw new Error('撮影先は管理下のローカル確認画面に限定します');
const packagePath=process.env.LVS_PLAYWRIGHT_MODULE;
const {chromium}=await import(packagePath?pathToFileURL(packagePath).href:'playwright');
await mkdir(destination,{recursive:true});
const browser=await chromium.launch({channel:'chrome',headless:true});
let page;
try{
  page=await browser.newPage({viewport:{width:1440,height:1050},deviceScaleFactor:1});
  const errors=[];
  const captures=[];
  const animationChecks={};
  page.on('pageerror',error=>errors.push(error.stack??error.message));
  await page.goto(`${previewUrl}?character=${id}`);
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).waitFor();
  await page.waitForFunction(()=>{
    const text=document.querySelector('#status')?.textContent;
    return text&&text!=='読込中';
  });
  const initialStatus=await page.locator('#status').textContent();
  if(!initialStatus.startsWith('素材充足:'))throw new Error('確認画面の読込に失敗しました: '+initialStatus);
  const settle=()=>page.evaluate(()=>new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done))));
  const waitForValue=async(locator,predicate,timeoutMs)=>{
    const deadline=Date.now()+timeoutMs;
    let lastValue=Number(await locator.inputValue());
    while(Date.now()<deadline){if(predicate(lastValue))return lastValue;await page.waitForTimeout(50);lastValue=Number(await locator.inputValue());}
    throw new Error('アニメーションのパラメータ変化を確認できません。最終値: '+lastValue);
  };
  const capture=async name=>{
    await settle();
    if(errors.length)throw new Error(errors.join('\n'));
    const status=await page.locator('#status').textContent();
    if(!status.startsWith('素材充足:'))throw new Error('描画状態が異常です: '+status);
    const canvasPixels=await page.locator('#avatar').screenshot({animations:'allow'});
    const pixels=await page.screenshot({path:resolve(destination,name+'.png'),fullPage:true});
    captures.push({name,sha256:createHash('sha256').update(pixels).digest('hex'),canvasSha256:createHash('sha256').update(canvasPixels).digest('hex'),status});
    console.log(JSON.stringify({character:id,image:name}));
  };
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).click();
  await capture('full');
  await page.getByRole('button',{name:'顔の拡大検査',exact:true}).click();
  await capture('neutral');
  for(const value of ['0.5','0']){
    await page.getByRole('slider',{name:'左目',exact:true}).fill(value);
    await page.getByRole('slider',{name:'右目',exact:true}).fill(value);
    await capture(value==='0'?'closed':'half');
  }
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).click();
  for(const [key,label] of [['a','あ'],['i','い'],['u','う'],['e','え'],['o','お']]){
    await page.getByRole('combobox',{name:'固定口形',exact:true}).selectOption({label});
    await capture('mouth-'+key);
  }
  await page.getByRole('combobox',{name:'固定口形',exact:true}).selectOption({label:'閉口'});
  for(const value of ['-12','12']){
    await page.getByRole('slider',{name:'顔左右',exact:true}).fill(value);
    await capture(value==='-12'?'left':'right');
  }
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).click();
  for(const [key,label] of [['left','左目'],['right','右目']]){
    await page.getByRole('slider',{name:label,exact:true}).fill('0');
    await capture(key+'-closed');
    await page.getByRole('slider',{name:label,exact:true}).fill('1');
  }
  // 首・襟・腕の確認では顔拡大を解除し、接続部を画面内に収める。
  await page.getByRole('button',{name:'顔の拡大検査',exact:true}).click();
  for(const [key,label,value] of [['pitch','顔上下','12'],['roll','首の傾き','12'],['arms','腕を寄せる','10']]){
    await page.getByRole('slider',{name:label,exact:true}).fill(value);
    await capture(key);
    await page.getByRole('slider',{name:label,exact:true}).fill('0');
  }
  await page.getByRole('button',{name:'顔の拡大検査',exact:true}).click();
  await page.getByRole('button',{name:'口パク動作テスト（無音）',exact:true}).click();
  animationChecks.mouthOpenY=await waitForValue(page.getByRole('slider',{name:'開き',exact:true}),value=>value>.2,10000);
  await page.getByRole('button',{name:'自動まばたきを開始',exact:true}).click();
  animationChecks.autoBlinkOpen=await waitForValue(page.getByRole('slider',{name:'左目',exact:true}),value=>value<=.01,1500);
  await page.getByRole('button',{name:'口パクテストを停止',exact:true}).click();
  await page.getByRole('button',{name:'自動まばたきを停止',exact:true}).click();
  // 顔拡大を解除し、口と目を止めた状態で待機時の首・肩・胴体だけを検証する。
  await page.getByRole('button',{name:'顔の拡大検査',exact:true}).click();
  await page.getByRole('button',{name:'待機動作テスト',exact:true}).click();
  animationChecks.idleYaw=await waitForValue(page.getByRole('slider',{name:'顔左右',exact:true}),value=>Math.abs(value)>2,1500);
  for(let index=0;index<4;index++){
    await page.waitForTimeout(550);
    await capture('idle-'+index);
  }
  const idleFrames=new Set(captures.filter(item=>item.name.startsWith('idle-')).map(item=>item.canvasSha256));
  if(idleFrames.size<2)throw new Error('待機時の首・肩・胴体に実canvasのフレーム差がありません');
  // 動画と総合動作フレームは、待機動作に口パク・まばたきを重ねた実利用相当とする。
  await page.getByRole('button',{name:'口パク動作テスト（無音）',exact:true}).click();
  await page.getByRole('button',{name:'自動まばたきを開始',exact:true}).click();
  let recording;
  if(video){
    // 新しい描画面を作らず、表示中の共通レンダラーの画素だけを録画する。
    const result=await page.evaluate(async()=>{
      const canvas=document.querySelector('#avatar');
      if(!(canvas instanceof HTMLCanvasElement)||!canvas.captureStream||typeof MediaRecorder==='undefined')throw new Error('ChromeのCanvas録画機能が利用できません');
      const mimeType=['video/webm;codecs=vp8','video/webm'].find(type=>MediaRecorder.isTypeSupported(type));
      if(!mimeType)throw new Error('WebM録画形式が利用できません');
      const stream=canvas.captureStream(20),chunks=[];
      let recorder,timer,watchdog;
      const started=performance.now();
      try{
        recorder=new MediaRecorder(stream,{mimeType,videoBitsPerSecond:1800000});
        const stopped=new Promise((done,fail)=>{
          recorder.ondataavailable=event=>{if(event.data.size)chunks.push(event.data);};
          recorder.onerror=event=>fail(new Error(event.error?.message??'Canvas録画に失敗しました'));
          recorder.onstop=done;
          watchdog=setTimeout(()=>fail(new Error('Canvas録画の終了がタイムアウトしました')),12000);
        });
        recorder.start(500);
        timer=setTimeout(()=>recorder.stop(),8000);
        await stopped;
        const durationMs=performance.now()-started;
        const blob=new Blob(chunks,{type:mimeType});
        if(!blob.size)throw new Error('録画データが空です');
        const dataUrl=await new Promise((done,fail)=>{
          const reader=new FileReader();reader.onload=()=>done(reader.result);reader.onerror=()=>fail(new Error('録画データの読出しに失敗しました'));reader.readAsDataURL(blob);
        });
        return {dataUrl,mimeType,durationMs,width:canvas.width,height:canvas.height,requestedFps:20};
      }finally{
        clearTimeout(timer);clearTimeout(watchdog);
        if(recorder&&recorder.state!=='inactive')recorder.stop();
        for(const track of stream.getTracks())track.stop();
      }
    });
    if(errors.length)throw new Error(errors.join('\n'));
    const status=await page.locator('#status').textContent();
    if(!status.startsWith('素材充足:'))throw new Error('録画中の描画状態が異常です: '+status);
    const bytes=Buffer.from(result.dataUrl.split(',')[1],'base64');
    await writeFile(resolve(destination,'motion.webm'),bytes);
    const {dataUrl,...metadata}=result;
    recording={file:'motion.webm',...metadata,sha256:createHash('sha256').update(bytes).digest('hex'),status,audio:false,quality:'unverified',source:'共通レンダラーの実canvas.captureStream'};
    console.log(JSON.stringify({character:id,video:recording.file,durationMs:recording.durationMs}));
  }
  // 一巡を複数時点で保存する。ハッシュ差は動作の証拠で、造形の合格判定ではない。
  for(let index=0;index<8;index++){
    await page.waitForTimeout(450);
    await capture('motion-'+index);
  }
  const animated=new Set(captures.filter(item=>item.name.startsWith('motion-')).map(item=>item.canvasSha256));
  if(animated.size<2)throw new Error('口パク・まばたきの実canvasにフレーム差がありません');
  await writeFile(resolve(destination,'capture-report.json'),JSON.stringify({character:id,viewport:{width:1440,height:1050},captures,animationChecks,errors,...(recording?{recording}:{})},null,2));
} finally {try{if(page)await page.close();}finally{await browser.close();}}
