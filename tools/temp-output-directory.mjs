import {lstat,realpath,mkdir} from 'node:fs/promises';
import {resolve,relative,isAbsolute,dirname,sep} from 'node:path';

const normalized=value=>process.platform==='win32'?value.toLowerCase():value;

async function plainDirectory(path){
  // 既存の祖先も調べ、tempの上位や途中にあるjunctionを経由しない。
  for(let current=path;;current=dirname(current)){
    const stat=await lstat(current);
    if(stat.isSymbolicLink()||!stat.isDirectory())throw new Error('リンクではない通常ディレクトリが必要です: '+current);
    if(normalized(await realpath(current))!==normalized(current))throw new Error('実パスが異なる出力経路は使えません: '+current);
    if(dirname(current)===current)break;
  }
}

export async function createTempOutputDirectory(output,tempRoot=resolve('temp')){
  const root=resolve(tempRoot),destination=resolve(output);
  const rel=relative(normalized(root),normalized(destination));
  if(!rel||rel==='..'||rel.startsWith('..'+sep)||isAbsolute(rel))throw new Error('専用temp子フォルダだけへ保存します');
  await plainDirectory(root);
  await plainDirectory(dirname(destination));
  try{
    await lstat(destination);
  }catch(error){
    if(error.code!=='ENOENT')throw error;
    // 非再帰mkdirで既存出力を上書きしない。並行した経路置換へのOSハンドル保証は対象外。
    await mkdir(destination,{recursive:false});
    return destination;
  }
  throw new Error('既存出力は上書きしません: '+destination);
}
