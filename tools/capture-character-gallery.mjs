// 作業用Chromeで共通確認画面を撮影する。生成素材を描き替えたり拡大保存したりしない。
import {mkdir} from 'node:fs/promises';
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
  page.on('pageerror',error=>errors.push(error.message));
  await page.goto(`http://127.0.0.1:8791/ui/check.html?character=${id}`);
  await page.getByRole('button',{name:'中立状態へ戻す',exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('素材充足:'));
  const settle=()=>page.evaluate(()=>new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done))));
  const capture=async name=>{
    await settle();
    if(errors.length)throw new Error(errors.join('\n'));
    await page.screenshot({path:resolve(destination,name+'.png'),fullPage:true});
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
  for(const [key,label] of [['a','あ'],['u','う'],['o','お']]){
    await page.getByRole('combobox',{name:'固定口形',exact:true}).selectOption({label});
    await capture('mouth-'+key);
  }
  await page.getByRole('combobox',{name:'固定口形',exact:true}).selectOption({label:'閉口'});
  for(const value of ['-12','12']){
    await page.getByRole('slider',{name:'顔左右',exact:true}).fill(value);
    await capture(value==='-12'?'left':'right');
  }
} finally {await browser.close();}
