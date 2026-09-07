"""二候補の実測値と原寸マスク差を集計し、確認用一覧を作る。"""
import json
from pathlib import Path
import numpy as np
from PIL import Image,ImageDraw
root=Path(__file__).resolve().parents[2]
backends=['Florence-2-large-ft','grounding-dino-base']
ids=['c_2700e1166676','c_190454c86edb','c_828ead7c98ab']
stats=[]
for backend in backends:
    reports=[json.loads(p.read_text(encoding='utf-8')) for p in (root/'temp/semantic-evaluation'/backend).glob('c_*.json')]
    print(backend,'seconds=',[round(x['seconds'],2) for x in reports],'peak GB=',max(x['peak_allocated_gb'] for x in reports))
    for original in (root/'temp/semantic-evaluation'/backend).glob('c_*.json'):
        repeated=original.parent/'repeat'/original.name
        if repeated.exists():
            first=json.loads(original.read_text(encoding='utf-8'))
            second=json.loads(repeated.read_text(encoding='utf-8'))
            print('反復一致',backend,original.name,first['records']==second['records'])
for cid in ids:
    for role in ['face','eyes','mouth','neck','collar']:
        images=[]
        for backend in backends:
            path=root/'temp/grounded-masks-v2'/backend/cid/f'{role}-0-0-connected.png'
            if not path.exists():
                images=[];break
            im=Image.open(path).convert('RGBA');images.append(im)
        if not images:
            stats.append({'character':cid,'role':role,'status':'missing'});continue
        masks=[np.asarray(im)[:,:,3]>0 for im in images]
        iou=float((masks[0]&masks[1]).sum()/max(1,(masks[0]|masks[1]).sum()))
        stats.append({'character':cid,'role':role,'mask_iou':iou})
    sheet=Image.new('RGB',(1200,850),(75,75,75));draw=ImageDraw.Draw(sheet)
    for row,backend in enumerate(backends):
        draw.text((10,row*420+5),backend,fill='white')
        for col,role in enumerate(['face','eyes','mouth','neck','collar']):
            part_path=root/'temp/grounded-masks-v2'/backend/cid/f'{role}-0-0-connected.png'
            if not part_path.exists():
                draw.text((col*240+5,row*420+40),role+': MISSING',fill='white');continue
            im=Image.open(part_path).convert('RGBA')
            im=im.crop(im.getbbox());im.thumbnail((225,375))
            sheet.paste(im,(col*240+5,row*420+40),im)
            draw.text((col*240+5,row*420+22),role,fill='white')
    sheet.save(root/'temp/semantic-evaluation'/f'{cid}-comparison.jpg')
print(json.dumps(stats,indent=2))
(root/'temp/semantic-evaluation/comparison.json').write_text(json.dumps(stats,indent=2),encoding='utf-8')
