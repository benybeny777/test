// 画像枚数の切替ではなく、上下の輪郭を同じ2軸から連続して求める。
const clamp=(value,min,max)=>Math.max(min,Math.min(max,value));

export function mouthGeometry(box, opening, form) {
  if(!Array.isArray(box)||box.length!==4||!box.every(Number.isFinite)||box[2]<=box[0]||box[3]<=box[1]) {
    throw new Error('口の実測座標が不正です');
  }
  if(!Number.isFinite(opening)||!Number.isFinite(form))throw new Error('口パラメータが不正です');
  const open=clamp(opening,0,1),shape=clamp(form,-1,1);
  const width=box[2]-box[0],cx=(box[0]+box[2])/2,cy=(box[1]+box[3])/2;
  const half=width*.46*(1+shape*.36);
  // 丸めた口は縦長、横に引いた口は浅くする。上下の輪郭は同じ口角で接続する。
  const height=width*.57*open*(1-.28*shape);
  const cornerY=cy-width*.065*shape;
  const middleY=cy+width*.045*shape;
  const roundness=.50-.15*shape;
  return {
    cx,cy,width,half,height,open,form:shape,
    left:[cx-half,cornerY],right:[cx+half,cornerY],
    upper:[[cx-half*roundness,middleY-height*.45],[cx+half*roundness,middleY-height*.45]],
    lower:[[cx+half*roundness,middleY+height*.88],[cx-half*roundness,middleY+height*.88]],
  };
}

export function drawMouth(ctx, box, opening, form, color) {
  const g=mouthGeometry(box,opening,form);
  if(!Array.isArray(color)||color.length<3||!color.slice(0,3).every(Number.isFinite))throw new Error('口の原画色がありません');
  const rgb=color.slice(0,3).map(v=>clamp(v,0,255));
  const mix=(other,amount)=>`rgb(${rgb.map((v,i)=>Math.round(v*(1-amount)+other[i]*amount)).join(',')})`;
  ctx.save();ctx.lineCap='round';ctx.lineJoin='round';
  const upper=()=>{ctx.moveTo(...g.left);ctx.bezierCurveTo(...g.upper[0],...g.upper[1],...g.right);};
  const lower=()=>{ctx.moveTo(...g.right);ctx.bezierCurveTo(...g.lower[0],...g.lower[1],...g.left);};
  if(g.open>0) {
    ctx.beginPath();upper();ctx.bezierCurveTo(...g.lower[0],...g.lower[1],...g.left);ctx.closePath();
    ctx.fillStyle=mix([50,15,29],.72);ctx.fill();
    ctx.save();ctx.clip();
    // 舌は口内の奥から現れ、歯は横に引くほど見える。輪郭の外へ漏らさない。
    ctx.fillStyle=mix([220,112,135],.82);
    ctx.beginPath();ctx.ellipse(g.cx,g.cy+g.height*.47,g.half*.65,g.height*.26,0,0,Math.PI*2);ctx.fill();
    const teeth=Math.max(0,g.form)*g.open;
    if(teeth>0) {
      ctx.globalAlpha=teeth;ctx.fillStyle='rgb(255,246,235)';
      ctx.fillRect(g.cx-g.half*.78,g.cy-g.height*.34,g.half*1.56,g.height*.16);
      ctx.globalAlpha=1;
    }
    ctx.restore();
  }
  // 唇を口内の外周一本にまとめない。下唇は薄く、上唇と口角を明瞭にする。
  ctx.beginPath();upper();ctx.strokeStyle=mix([91,39,43],.22);
  ctx.lineWidth=Math.max(.65,g.width*.036);ctx.stroke();
  if(g.open>0) {
    ctx.beginPath();lower();ctx.strokeStyle=mix([191,108,101],.48);
    ctx.lineWidth=Math.max(.5,g.width*.026);ctx.stroke();
  }
  ctx.restore();
}
