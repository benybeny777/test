// 原画の瞳を潰さず、上下の開口領域と原画由来の上まぶたを動かす。
export function eyeAperture(layer, openness) {
  const aperture=layer?.eye_aperture;
  if(!Array.isArray(aperture)||aperture.length<2||!Number.isFinite(openness))throw new Error('まぶたの実測境界がありません。分解から再生成してください');
  let previous=-Infinity;
  const open=Math.min(1,Math.max(0,openness));
  return aperture.map(point=>{
    if(!Array.isArray(point)||point.length!==4||!point.every(Number.isFinite))throw new Error('まぶたの実測境界が不正です');
    const [x,top,bottom,closed]=point;
    if(x<=previous||top>bottom)throw new Error('まぶたの実測境界が交差しています');
    previous=x;
    return [x,closed+(top-closed)*open,closed+(bottom-closed)*open,closed];
  });
}

export function drawBlink(ctx, images, rig, side, openness) {
  if(openness>=1)return;
  const base=rig.layers[side+'_eye_base'],upper=rig.layers[side+'_eyelid_upper'],original=rig.layers[side+'_eye_open'];
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
    draw(side+'_eye_open',original);ctx.restore();
  }
  const image=images.get(side+'_eyelid_upper'),[left,top]=upper.texture_box;
  for(const [x,y,,closed] of points) {
    const column=Math.floor(x)-left;
    if(column<0||column>=image.naturalWidth)continue;
    ctx.drawImage(image,column,0,1,image.naturalHeight,left+column,top+y-closed,1,image.naturalHeight);
  }
  ctx.restore();
}
