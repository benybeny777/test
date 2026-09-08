// 実測された顔・首の接続で頭の回転を連続的に胴体へ減衰させる。
const clamp=(v,a,b)=>Math.max(a,Math.min(b,v));
const smooth=(a,b,x)=>{const t=clamp((x-a)/(b-a),0,1);return t*t*(3-2*t);};
export function armDisplacement(x,y,arm,cx,width,degrees){
  const length=arm[3]-arm[1],shoulder=arm[1]+length*.1;
  const margin=Math.max(1,length*.1);
  const outside=Math.max(arm[0]-x,0,x-arm[2]);
  const weight=smooth(width*.06,width*.26,Math.abs(x-cx))*smooth(shoulder,shoulder+(arm[3]-shoulder)*.45,y)
    *(1-smooth(arm[3],arm[3]+margin,y))*(1-smooth(0,margin,outside));
  if(weight===0)return [0,0];
  const angle=clamp(degrees,-30,30)*Math.PI/180;
  return [-(y-shoulder)*Math.sin(angle)*weight,(y-shoulder)*(Math.cos(angle)-1)*weight];
}
export function headDisplacement(x,y,face,neck,width,height,yaw,pitch,roll){
  const start=face[3],end=Math.max(start+1,neck[3]);
  const t=clamp((y-start)/(end-start),0,1),weight=1-t*t*(3-2*t);
  if(weight===0)return [0,0];
  const cx=(face[0]+face[2])/2,cy=end;
  const angle=clamp(roll,-30,30)*Math.PI/180,cos=Math.cos(angle),sin=Math.sin(angle);
  return [weight*((x-cx)*(cos-1)-(y-cy)*sin+clamp(yaw,-30,30)*width*.00055),
          weight*((x-cx)*sin+(y-cy)*(cos-1)+clamp(pitch,-30,30)*height*.00035)];
}
// 肩から胴体下部へ連続的に減衰する呼吸変位。頭も同量だけ持ち上げて首の裂けを防ぐ。
export function bodyBreathDisplacement(x,y,torso,neck,width,height,amount){
  if(!Array.isArray(torso)||torso.length!==4||!torso.every(Number.isFinite)||
     !Array.isArray(neck)||neck.length!==4||!neck.every(Number.isFinite)||!Number.isFinite(amount))
    throw new Error('呼吸動作に必要な胴体範囲が不正です');
  const top=Math.min(neck[1],torso[1]),bottom=torso[3],fadeStart=top+(bottom-top)*.28;
  const weight=1-smooth(fadeStart,bottom,y),cx=(torso[0]+torso[2])/2;
  if(weight===0)return [0,0];
  const strength=clamp(amount,-2,2);
  return [(x-cx)*strength*.0012*weight,-height*strength*.0012*weight];
}
export function validateSecondaryMotion(profile){
  if(profile===undefined)return;
  const hair=profile?.hair;
  if(profile===null||hair?.role!=='hair'||
     !Number.isFinite(hair.frequency_hz)||hair.frequency_hz<.2||hair.frequency_hz>8||
     !Number.isFinite(hair.damping_ratio)||hair.damping_ratio<.2||hair.damping_ratio>2||
     !Number.isFinite(hair.strength)||hair.strength<0||hair.strength>2)
    throw new Error('局所物理の定義が不正です');
}
export function springStep(current,target,elapsedSeconds,frequencyHz,dampingRatio){
  if(!current||![current.value,current.velocity,target,elapsedSeconds,frequencyHz,dampingRatio].every(Number.isFinite)||
     elapsedSeconds<0||frequencyHz<=0||dampingRatio<=0)throw new Error('局所物理の状態が不正です');
  let value=current.value,velocity=current.velocity,remaining=Math.min(elapsedSeconds,.1);
  const omega=2*Math.PI*frequencyHz;
  // 長い停止後も発散させないため、最大10msずつ半陰解法で積分する。
  while(remaining>0){const dt=Math.min(remaining,.01);
    velocity+=((target-value)*omega*omega-2*dampingRatio*omega*velocity)*dt;
    value+=velocity*dt;remaining-=dt;
  }
  return {value,velocity};
}
export function hairSecondaryDisplacement(x,y,face,width,height,value,strength){
  if(![x,y,width,height,value,strength].every(Number.isFinite)||!Array.isArray(face)||face.length!==4||!face.every(Number.isFinite))
    throw new Error('髪物理に必要な範囲が不正です');
  const cx=(face[0]+face[2])/2,side=smooth(width*.035,width*.24,Math.abs(x-cx));
  const vertical=smooth(face[1],Math.max(face[1]+1,face[3]),y);
  const weight=Math.max(side,vertical*.45);
  return [value*strength*width*.0022*weight,Math.abs(value)*strength*height*.00035*vertical];
}
// 比較用の局所髪移動。未補完の外周・首肩には変位を加えない。
export function validateHiddenMotion(profile,count){
  if(profile===undefined)return;
  if(profile.version!==1||!Array.isArray(profile.weights)||profile.weights.length!==count||
     (profile.repair_layer!==undefined&&profile.repair_layer!=='scene_ear_repair')||
     profile.weights.some(v=>!Number.isFinite(v)||v<0||v>1)||
     !['angle_limit','x_limit_px','y_limit_px'].every(k=>Number.isFinite(profile[k])&&profile[k]>0))
    throw new Error('補完比較の変位定義が不正です');
}
export function hiddenRepairAmount(profile,yaw,pitch,visible=true){
  return profile?.repair_layer&&visible?clamp(Math.max(Math.abs(yaw),Math.abs(pitch))/profile.angle_limit,0,1):0;
}
export function hiddenOffset(profile,index,yaw,pitch){
  const bound=value=>Math.max(-1,Math.min(1,value/profile.angle_limit));
  return [profile.weights[index]*profile.x_limit_px*bound(yaw),profile.weights[index]*profile.y_limit_px*bound(pitch)];
}
