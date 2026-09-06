// 作業用Chromeで共通確認画面を撮影する。生成素材を描き替えたり拡大保存したりしない。
import {mkdir,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const [id,output='temp/character-captures']=process.argv.slice(2);
if(!/^c_[0-9a-f]{12}$/.test(id??''))throw new Error('確認対象のキャラIDを指定してください');
const destination=resolve(output);
const tempRoot=resolve('temp');
if(!destination.startsWith(tempRoot+'\\')&&!destination.startsWith(tempRoot+'/'))throw new Error('撮影先はtemp内に限定します');
const packagePath=process.env.LVS_PLAYWRIGHT_MODULE;
const {chromium}=await import(packagePath?pathToFileURL(packagePath).href:'playwright');
await mkdir(destination,{recursive:true});
const browser=await chromium.launch({channel:'chrome',headless:true});
try{
  const page=await browser.newPage({viewport:{width:1440,height:1050},deviceScaleFactor:1});
  const errors=[];
  const captures=[];
  page.on('pageerror',error=>errors.push(error.message));
  await page.goto(`http://127.0.0.1:8791/ui/check.html?character=${id}`);
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('素材充足:'));
  const settle=()=>page.evaluate(()=>new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done))));
  const capture=async name=>{
    await settle();
    if(errors.length)throw new Error(errors.join('\n'));
    const status=await page.locator('#status').textContent();
    if(!status.startsWith('素材充足:'))throw new Error('描画状態が異常です: '+status);
    const pixels=await page.screenshot({path:resolve(destination,name+'.png'),fullPage:true});
    captures.push({name,sha256:createHash('sha256').update(pixels).digest('hex'),status});
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
  for(const [key,label,value] of [['pitch','顔上下','12'],['roll','首の傾き','12'],['arms','腕を寄せる','10']]){
    await page.getByRole('slider',{name:label,exact:true}).fill(value);
    await capture(key);
    await page.getByRole('slider',{name:label,exact:true}).fill('0');
  }
  await page.getByRole('button',{name:'口パク動作テスト（無音）',exact:true}).click();
  await page.getByRole('button',{name:'自動まばたき',exact:true}).click();
  // 一巡を複数時点で保存する。ハッシュ差は動作の証拠で、造形の合格判定ではない。
  for(let index=0;index<8;index++){
    await page.waitForTimeout(450);
    await capture('motion-'+index);
  }
  await writeFile(resolve(destination,'capture-report.json'),JSON.stringify({character:id,viewport:{width:1440,height:1050},captures,errors},null,2));
} finally {await browser.close();}
