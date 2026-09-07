// 画像枚数の切替ではなく、上下の輪郭を同じ2軸から連続して求める。
const clamp=(value,min,max)=>Math.max(min,Math.min(max,value));
export const MOUTH_PRESETS = Object.freeze({close:[0,0],a:[1,0],i:[.28,.9],u:[.5,-.9],e:[.55,.65],o:[.9,-.7]});

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

export function lipMesh(layer, opening, form) {
  if(!layer)throw new Error('原画の唇がありません');
  const box=layer.texture_box,seam=layer.lip_seam,feature=layer.feature_box;
  mouthGeometry(feature,opening,form);
  if(!Array.isArray(box)||box.length!==4||!box.every(Number.isFinite)||!Array.isArray(seam)||seam.length<3)throw new Error('原画の唇の分割境界がありません。分解から再生成してください');
  const [l,t,r,b]=box,[ml,,mr]=feature,width=mr-ml,cx=(ml+mr)/2;
  const open=clamp(opening,0,1),shape=clamp(form,-1,1);
  let previous=-Infinity;
  for(const point of seam) {
    if(!Array.isArray(point)||point.length!==2||!point.every(Number.isFinite)||point[0]<=previous||point[0]<l||point[0]>r||point[1]<=t||point[1]>=b)throw new Error('唇の分割境界が不正です');
    previous=point[0];
  }
  const knots=[[l,seam[0][1]],...seam,[r,seam.at(-1)[1]]];
  const roundness=Math.max(0,-shape),centerY=(feature[1]+feature[3])/2;
  const source=[],upper=[],lower=[];
  for(const [x,y] of knots) {
    const u=clamp((x-ml)/width,0,1);
    const interior=x>=ml&&x<=mr;
    const envelope=interior?1:x<ml?(x-l)/Math.max(1,ml-l):(r-x)/Math.max(1,r-mr);
    // すぼめるほど口角の笑い曲線を弱め、縦横比と側面を丸い開口へ連続変形する。
    const bulge=interior?Math.pow(Math.max(0,Math.sin(Math.PI*u)),.8-.3*roundness):0;
    // 中央の開口を深くする。すぼめ・横引きへ滑らかに減衰し、閉口は変えない。
    const openBoost=.16*Math.pow(Math.max(0,1-Math.pow(shape/.65,2)),2);
    const gap=width*(.42+.12*roundness+openBoost)*.4*open*(1-.2*shape)*bulge;
    const dx=(x-cx)*shape*(.30+.25*roundness)*envelope;
    const bend=shape*width*.055*(bulge-.65)*(1-roundness)*envelope;
    const roundingShift=(centerY-y)*roundness*envelope;
    source.push([[x,t],[x,y],[x,b]]);
    upper.push([[x,t],[x+dx,y+roundingShift+bend-gap*.36]]);
    lower.push([[x+dx,y+roundingShift+bend+gap*.64],[x,b]]);
  }
  return {source,upper,lower,box,feature,open,form:shape};
}

// 広い切出し矩形の鼻下・顎を引かず、唇近傍だけで変位を滑らかにゼロへ戻す。
export function localLipStrips(mesh){
  const [,t,,b]=mesh.box,width=mesh.feature[2]-mesh.feature[0];
  const sourceUpper=[],targetUpper=[],sourceLower=[],targetLower=[];
  const smooth=value=>value*value*(3-2*value);
  for(let i=0;i<mesh.source.length;i++){
    const [x,y]=mesh.source[i][1],up=mesh.upper[i][1],down=mesh.lower[i][0];
    // 最大開口でも折り返さない範囲を確保する。原画外へアンカーを出さない。
    const top=Math.max(t,Math.min(mesh.feature[1]-width*.08,y-Math.max(0,y-up[1])*1.6-width*.02));
    const bottom=Math.min(b,Math.max(mesh.feature[3]+width*.12,y+width*.30,y+Math.max(0,down[1]-y)*1.6+width*.02));
    const su=[],tu=[],sl=[],tl=[];
    for(let row=0;row<=3;row++){
      const u=row/3,upperY=top+(y-top)*u,lowerY=y+(bottom-y)*u;
      const upperWeight=smooth(u),lowerWeight=1-smooth(u);
      su.push([x,upperY]);tu.push([x+(up[0]-x)*upperWeight,upperY+(up[1]-y)*upperWeight]);
      sl.push([x,lowerY]);tl.push([x+(down[0]-x)*lowerWeight,lowerY+(down[1]-y)*lowerWeight]);
    }
    sourceUpper.push(su);targetUpper.push(tu);sourceLower.push(sl);targetLower.push(tl);
  }
  return {sourceUpper,targetUpper,sourceLower,targetLower};
}

function texturedTriangle(ctx,image,source,target,origin) {
  const [[x0,y0],[x1,y1],[x2,y2]]=source.map(([x,y])=>[x-origin[0],y-origin[1]]);
  const [[u0,v0],[u1,v1],[u2,v2]]=target;
  const det=(x1-x0)*(y2-y0)-(x2-x0)*(y1-y0);
  if(Math.abs(det)<1e-8)return;
  const a=((u1-u0)*(y2-y0)-(u2-u0)*(y1-y0))/det;
  const c=((u2-u0)*(x1-x0)-(u1-u0)*(x2-x0))/det;
  const b=((v1-v0)*(y2-y0)-(v2-v0)*(y1-y0))/det;
  const d=((v2-v0)*(x1-x0)-(v1-v0)*(x2-x0))/det;
  ctx.save();ctx.beginPath();ctx.moveTo(u0,v0);ctx.lineTo(u1,v1);ctx.lineTo(u2,v2);ctx.closePath();ctx.clip();
  ctx.transform(a,b,c,d,u0-a*x0-c*y0,v0-b*x0-d*y0);ctx.drawImage(image,0,0);ctx.restore();
}

// 上歯は実測の開口に沿う帯として口内に置く。すぼめるほど幅と露出を減らす。
export function upperTeethGeometry(mesh){
  if(mesh.open===0)return null;
  const upper=mesh.upper.map(column=>column[1]),lower=mesh.lower.map(column=>column[0]);
  const left=upper[1][0],right=upper.at(-2)[0],width=right-left,roundness=Math.max(0,-mesh.form);
  const sample=(points,x)=>{
    for(let i=1;i<points.length;i++)if(x<=points[i][0]){
      const a=points[i-1],b=points[i],t=clamp((x-a[0])/Math.max(1e-8,b[0]-a[0]),0,1);
      return a[1]+(b[1]-a[1])*t;
    }
    return points.at(-1)[1];
  };
  const top=[],bottom=[],margin=.12+.16*roundness;
  for(let i=0;i<=16;i++){
    const u=i/16,x=left+width*(margin+(1-2*margin)*u),y=sample(upper,x),gap=Math.max(0,sample(lower,x)-y);
    const start=y+gap*.025,depth=Math.min(gap*.32,width*.115*(1-.75*roundness))*Math.sqrt(Math.max(0,Math.sin(Math.PI*u)));
    top.push([x,start]);bottom.push([x,start+depth]);
  }
  return {top,bottom,opacity:clamp(mesh.open/.16,0,1)*(.94-.24*roundness)};
}

export function drawTexturedMouth(ctx,image,layer,opening,form,color) {
  const mesh=lipMesh(layer,opening,form);
  if(mesh.open===0&&mesh.form===0) {
    ctx.drawImage(image,mesh.box[0],mesh.box[1]);return;
  }
  // 変形範囲の外は原画のまま残す。口内はこの後で上から描く。
  ctx.drawImage(image,mesh.box[0],mesh.box[1]);
  const upper=mesh.upper.map(points=>points[1]),lower=mesh.lower.map(points=>points[0]);
  ctx.save();ctx.beginPath();ctx.moveTo(...upper[0]);
  for(const point of upper.slice(1))ctx.lineTo(...point);
  for(const point of [...lower].reverse())ctx.lineTo(...point);
  ctx.closePath();
  const rgb=color?.slice(0,3);
  if(!rgb||!rgb.every(Number.isFinite)) {ctx.restore();throw new Error('口の原画色がありません');}
  const innerTop=Math.min(...upper.map(p=>p[1])),innerBottom=Math.max(...lower.map(p=>p[1]));
  const cavity=ctx.createLinearGradient(0,innerTop,0,Math.max(innerTop+1,innerBottom));
  cavity.addColorStop(0,'rgb(26,12,18)');
  cavity.addColorStop(.55,`rgb(${rgb.map((v,i)=>Math.round(v*.3+[48,16,26][i]*.7)).join(',')})`);
  cavity.addColorStop(1,'rgb(82,35,45)');ctx.fillStyle=cavity;ctx.fill();
  ctx.clip();
  const middle=lower[Math.floor(lower.length/2)],width=layer.feature_box[2]-layer.feature_box[0];
  ctx.fillStyle='rgb(176,88,102)';ctx.beginPath();
  ctx.ellipse(middle[0],middle[1],width*.26*(1+mesh.form*.3),width*.09*mesh.open,0,0,Math.PI*2);ctx.fill();
  const teeth=upperTeethGeometry(mesh);
  if(teeth){
    // 口内のクリップを保ち、最後に描く原画の上唇より奥へ置く。個別の歯線は付けない。
    const top=Math.min(...teeth.top.map(p=>p[1])),bottom=Math.max(...teeth.bottom.map(p=>p[1]));
    const shade=ctx.createLinearGradient(0,top,0,Math.max(top+1,bottom));
    shade.addColorStop(0,'rgb(143,130,122)');shade.addColorStop(.42,'rgb(239,230,214)');shade.addColorStop(1,'rgb(204,195,181)');
    ctx.globalAlpha*=teeth.opacity;ctx.fillStyle=shade;ctx.beginPath();ctx.moveTo(...teeth.top[0]);
    for(const point of teeth.top.slice(1))ctx.lineTo(...point);
    for(const point of [...teeth.bottom].reverse())ctx.lineTo(...point);
    ctx.closePath();ctx.fill();
  }
  ctx.restore();
  const strips=localLipStrips(mesh);
  for(const [source,target] of [[strips.sourceUpper,strips.targetUpper],[strips.sourceLower,strips.targetLower]])
    for(let i=0;i<source.length-1;i++)for(let row=0;row<3;row++){
      const s0=source[i],s1=source[i+1],d0=target[i],d1=target[i+1];
      texturedTriangle(ctx,image,[s0[row],s1[row],s1[row+1]],[d0[row],d1[row],d1[row+1]],mesh.box);
      texturedTriangle(ctx,image,[s0[row],s1[row+1],s0[row+1]],[d0[row],d1[row+1],d0[row+1]],mesh.box);
    }
}
