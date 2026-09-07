// 原画の瞳を潰さず、上下の開口領域と原画由来の上まぶたを動かす。
export const AUTOMATIC_BLINK_DURATION_MS=650;

export function automaticBlinkOpen(elapsed) {
  if(!Number.isFinite(elapsed))throw new Error('まばたき時刻が不正です');
  if(elapsed<=0)return 1;
  // 完全閉眼を一定時間保持し、低フレームレートでも閉じた絵を必ず表示する。
  if(elapsed<180){const t=elapsed/180;return 1-t*t*(3-2*t);}
  if(elapsed<400)return 0;
  if(elapsed<AUTOMATIC_BLINK_DURATION_MS){const t=(elapsed-400)/250;return t*t*(3-2*t);}
  return 1;
}

export function eyeAperture(layer, openness) {
  const aperture=layer?.eye_aperture;
  if(!Array.isArray(aperture)||aperture.length<2||!Number.isFinite(openness))throw new Error('まぶたの実測境界がありません。分解から再生成してください');
  let previous=-Infinity;
  const open=Math.min(1,Math.max(0,openness));
  const curve=layer.closed_curve;
  if(layer.closed_material===true&&curve===undefined)throw new Error('編集閉眼の曲線がありません');
  if(curve!==undefined&&(!Array.isArray(curve)||curve.length!==aperture.length||!curve.every(Number.isFinite)||layer.closed_material!==true))throw new Error('比較用閉眼曲線が不正です');
  return aperture.map((point,index)=>{
    if(!Array.isArray(point)||point.length!==4||!point.every(Number.isFinite))throw new Error('まぶたの実測境界が不正です');
    const [x,top,bottom,closed]=point;
    if(x<=previous||top>bottom)throw new Error('まぶたの実測境界が交差しています');
    previous=x;
    // フォールバック許可: 比較情報のない既存素材は、保存済みの従来閉眼曲線を使う。
    const target=curve?.[index]??closed;
    return [x,target+(top-target)*open,target+(bottom-target)*open,closed];
  });
}

export function drawBlink(ctx, images, rig, side, openness) {
  if(openness>=1)return;
  const base=rig.layers[side+'_eye_base'],upper=rig.layers[side+'_eyelid_upper'];
  const points=eyeAperture(base,openness);
  const draw=(name,layer)=>ctx.drawImage(images.get(name),layer.texture_box[0],layer.texture_box[1]);
  ctx.save();draw(side+'_eye_base',base);
  if(openness>0) {
    ctx.save();ctx.beginPath();
    ctx.moveTo(points[0][0]-.5,points[0][1]);
    for(const [x,y] of points)ctx.lineTo(x,y);
    ctx.lineTo(points.at(-1)[0]+.5,points.at(-1)[1]);
    ctx.lineTo(points.at(-1)[0]+.5,points.at(-1)[2]);
    for(const [x,,y] of [...points].reverse())ctx.lineTo(x,y);
    ctx.lineTo(points[0][0]-.5,points[0][2]);ctx.closePath();ctx.clip();
    for(const suffix of ['eye_backplate','eye_iris'])draw(side+'_'+suffix,rig.layers[side+'_'+suffix]);
    ctx.restore();
  }
  const image=images.get(side+'_eyelid_upper'),[left,top]=upper.texture_box;
  for(const [x,y,,closed] of points) {
    const column=Math.floor(x)-left;
    if(column<0||column>=image.naturalWidth)continue;
    ctx.drawImage(image,column,0,1,image.naturalHeight,left+column,top+y-closed,1,image.naturalHeight);
  }
  ctx.restore();
}
