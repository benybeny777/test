const status=document.querySelector('#status'),results=document.querySelector('#results'),button=document.querySelector('#reload');
let controller=null;
async function refresh(){
  controller?.abort();controller=new AbortController();button.disabled=true;
  try{
    const response=await fetch('/api/qwen-comparisons',{cache:'no-store',signal:controller.signal});
    if(!response.ok)throw new Error(`HTTP ${response.status}`);
    const data=await response.json();results.replaceChildren();
    status.textContent=data.runs.length?`${data.runs.length}件の実験結果。合格の意味ではありません。`:'生成結果はまだありません。取得・生成の進捗は作業メッセージをご確認ください。';
    for(const run of data.runs){
      const section=document.createElement('section'),heading=document.createElement('h2'),state=document.createElement('p'),grid=document.createElement('div');
      heading.textContent=run.name;grid.className='grid';
      state.textContent=run.status==='complete'?`生成完了・品質未承認（${run.seconds}秒）`:'生成失敗またはレポート不正。採用対象ではありません。';
      for(const item of run.images){
        const figure=document.createElement('figure'),caption=document.createElement('figcaption'),link=document.createElement('a'),image=document.createElement('img');
        const url=new URL(item.url,location.origin);
        if(url.origin!==location.origin||!url.pathname.startsWith('/temp/qwen-eval-'))throw new Error('公開対象外の画像です');
        caption.textContent=item.label;image.alt=item.label;image.src=url.href;image.loading='lazy';link.href=url.href;link.target='_blank';link.rel='noopener';link.append(image);figure.append(caption,link);grid.append(figure);
      }
      section.append(heading,state,grid);results.append(section);
    }
  }catch(error){if(error.name!=='AbortError')status.textContent=`結果を読み込めません: ${error.message}`;}
  finally{button.disabled=false;}
}
button.addEventListener('click',refresh);
window.addEventListener('pagehide',()=>controller?.abort(),{once:true});
refresh();
