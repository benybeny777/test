// 完全透明な画素の色だけを近傍から延長する。アルファと可視画素は保持する。
export function bleedTransparentRgb(data,width,height){
  if(data.length!==width*height*4)throw new Error('透明色補完の寸法が一致しません');
  const count=width*height,queue=new Int32Array(count),known=new Uint8Array(count);
  let read=0,write=0;
  for(let i=0;i<count;i++)if(data[i*4+3]){known[i]=1;queue[write++]=i;}
  if(!write)throw new Error('透明色補完の参照画素がありません');
  while(read<write){
    const i=queue[read++],x=i%width;
    for(const n of [x>0?i-1:-1,x<width-1?i+1:-1,i-width,i+width]){
      if(n<0||n>=count||known[n])continue;
      known[n]=1;queue[write++]=n;
      data[n*4]=data[i*4];data[n*4+1]=data[i*4+1];data[n*4+2]=data[i*4+2];
    }
  }
  return data;
}
