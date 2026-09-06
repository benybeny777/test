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
