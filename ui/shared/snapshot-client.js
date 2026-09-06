import {SnapshotHash} from './snapshot-sha256.js';
// 1素材ずつ検査してBlobへ移す。完了レコードがない応答は表示へ渡さない。
export async function consumeSnapshot(response,limits,{signal,createUrl=blob=>URL.createObjectURL(blob),revokeUrl=url=>URL.revokeObjectURL(url)}={}){
  if(!response.ok||!response.body)throw new Error('公開世代を取得できません');
  for(const key of ['record_bytes','chunk_bytes','part_bytes','total_bytes','parts','dimension'])if(!Number.isSafeInteger(limits[key])||limits[key]<=0)throw new Error('配信上限が不正です');
  const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{fatal:true}),urls=[];
  // フォールバック許可: 取消の後片付けだけ。取得失敗そのものは下の読取／完了検査で通知する。
  const abort=()=>{reader.cancel(signal.reason).catch(()=>{});};
  signal?.addEventListener('abort',abort,{once:true});
  let pending='',header,current,total=0,finished=false,rig,rigUrl;const partUrls={},dimensions={},seen=new Set();
  const fail=message=>{throw new Error(message);};
  async function accept(line){
    if(new TextEncoder().encode(line).length>limits.record_bytes)fail('JSON上限を超えました');
    const value=JSON.parse(line);
    if(finished)fail('完了後の余分なレコードです');
    if(!header){
      if(value.type!=='snapshot'||value.schema_version!==1||!/^g_[0-9a-f]{32}$/.test(value.generation)||!Number.isSafeInteger(value.total)||value.total<0||value.total>limits.total_bytes||!Number.isSafeInteger(value.parts)||value.parts<1||value.parts>limits.parts||!/^[0-9a-f]{64}$/.test(value.rig_sha256))fail('公開世代ヘッダが不正です');
      header=value;return;
    }
    if(value.type==='begin'){
      const name=value.name;
      if(current||typeof name!=='string'||!/^([a-z][a-z0-9_]{0,127})$/.test(name)||/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(name)||seen.has(name)||seen.size>header.parts)fail('素材名/順序が不正です');
      if(!Number.isSafeInteger(value.length)||value.length<1||value.length>limits.part_bytes||total+value.length>limits.total_bytes)fail('素材容量上限を超えました');
      if(name==='rig'?value.mime!=='application/json':value.mime!=='image/png'||!Array.isArray(value.size)||value.size.length!==2||value.size.some(v=>!Number.isSafeInteger(v)||v<=0||v>limits.dimension))fail('素材形式/寸法が不正です');
      seen.add(name);current={...value,chunks:[],received:0,hash:new SnapshotHash()};return;
    }
    if(value.type==='chunk'){
      if(!current||value.name!==current.name||typeof value.data!=='string'||value.data.length>Math.ceil(limits.chunk_bytes/3)*4)fail('素材チャンクが不正です');
      const binary=atob(value.data),bytes=Uint8Array.from(binary,c=>c.charCodeAt(0));
      current.received+=bytes.length;total+=bytes.length;
      if(bytes.length>limits.chunk_bytes||current.received>current.length||total>header.total)fail('受信容量が宣言を超えました');
      current.hash.update(bytes);current.chunks.push(bytes);return;
    }
    if(value.type==='end'){
      if(!current||value.name!==current.name||current.received!==current.length)fail('素材が未完了です');
      const blob=new Blob(current.chunks,{type:current.mime});current.chunks=[];
      const digest=current.hash.hex();
      if(digest!==value.sha256||(current.name==='rig'&&digest!==header.rig_sha256))fail('素材SHAが不一致です');
      if(current.name==='rig')rig=JSON.parse(await blob.text());
      const url=createUrl(blob);urls.push(url);
      if(current.name==='rig')rigUrl=url;else {partUrls[current.name]=url;dimensions[current.name]=current.size;}
      current=null;return;
    }
    if(value.type==='complete'){
      if(current||value.generation!==header.generation||total!==header.total||!rigUrl||Object.keys(partUrls).length!==header.parts||!rig?.layers||Object.keys(rig.layers).length!==header.parts)fail('公開世代が未完了です');
      for(const [name,layer] of Object.entries(rig.layers)){
        const box=layer.texture_box,size=dimensions[name];
        if(!partUrls[name]||layer.url!==`/assets/rig2d/parts/${name}.png`||!Array.isArray(box)||box.length!==4||!box.every(Number.isFinite)||box[0]<0||box[1]<0||box[2]-box[0]!==size[0]||box[3]-box[1]!==size[1])fail('素材一覧または寸法が一致しません');
      }
      finished=true;return;
    }
    fail('未対応の配信レコードです');
  }
  try{
    while(true){
      if(signal?.aborted)throw signal.reason??new Error('公開世代の取得を中断しました');
      const {done,value}=await reader.read();if(done)break;
      // ネットワークの受信単位が巨大でも、保持する未処理文字列を制限する。
      for(let offset=0;offset<value.length;offset+=limits.record_bytes){
        pending+=decoder.decode(value.subarray(offset,offset+limits.record_bytes),{stream:true});
        let end;
        while((end=pending.indexOf('\n'))>=0){const line=pending.slice(0,end);pending=pending.slice(end+1);await accept(line);}
        if(pending.length>limits.record_bytes)fail('未終端JSONが上限を超えました');
      }
    }
    pending+=decoder.decode();if(pending||!finished)fail('公開世代の完了レコードがありません');
    return {rig,rigUrl,partUrls,generation:header.generation,dispose(){for(const url of urls.splice(0))revokeUrl(url);}};
  }catch(error){for(const url of urls.splice(0))revokeUrl(url);throw error;}
  // フォールバック許可: 成否確定後の重複cancel失敗だけを無視し、readerのロックは必ず解放する。
  finally{signal?.removeEventListener('abort',abort);await reader.cancel().catch(()=>{});reader.releaseLock();}
}
