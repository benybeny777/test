"""Qwen比較の再合成誤差と、変更しない目口の画素差を記録する。品質合否は推定しない。"""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image
from run import ROOT,verify_source


def white(image):
    return np.asarray(Image.alpha_composite(Image.new('RGBA',image.size,'white'),image.convert('RGBA')))[:,:,:3].astype(np.float64)


def statistics(delta,mask=None):
    values=delta if mask is None else delta[mask]
    if not values.size:raise ValueError('評価領域が空です')
    return {'mean_absolute_rgb_0_255':round(float(np.abs(values).mean()),4),'maximum_absolute_rgb_0_255':float(np.abs(values).max())}


def analyze(directory,character):
    directory=directory.resolve()
    if not directory.is_relative_to(ROOT/'temp'):raise ValueError('比較解析の書き込み先はtemp内に限定します')
    report=json.loads((directory/'report.json').read_text(encoding='utf-8'))
    if report['status']!='complete':raise ValueError('完了した生成のみ解析できます')
    verify_source(character,report['source'])
    with Image.open(directory/'input/input.png') as image:reference=white(image)
    images=[]
    for name in report['images']:
        path=(directory/name).resolve()
        if not path.is_relative_to(directory.resolve()):raise ValueError('比較外の画像を参照しています')
        with Image.open(path) as image:images.append(image.convert('RGBA'))
    metrics={'mode':report['mode'],'quality':'unverified','product_adopted':False,'full_image_vs_input':statistics(white(images[0])-reference)}
    if report['mode']=='layered':
        composite=Image.new('RGBA',images[0].size)
        records=[]
        for index,image in enumerate(images[1:],start=1):
            alpha=np.asarray(image)[:,:,3]
            records.append({'index':index,'nonzero_alpha_pixels':int((alpha>0).sum()),'opaque_pixels':int((alpha==255).sum()),'bbox':image.getbbox()})
            composite=Image.alpha_composite(composite,image)
        composite.save(directory/'recomposed.png')
        metrics['layers']=records
        metrics['recomposed_vs_full_image']=statistics(white(composite)-white(images[0]))
        metrics['recomposed_vs_input']=statistics(white(composite)-reference)
    if report['source']['view']=='head':
        left,top,right,bottom=report['source']['source_region']
        measured={}
        with np.load(character/'analysis/masks.npz',allow_pickle=False) as masks:
            for role in ('left_eye','right_eye','mouth','neck','collar'):
                region=masks[role][top:bottom,left:right]
                if not region.any():continue
                padded=np.zeros(reference.shape[:2],dtype=bool);padded[:region.shape[0],:region.shape[1]]=region
                measured[role]={'pixels':int(region.sum()),**statistics(white(images[0])-reference,padded)}
        metrics['visible_feature_changes']=measured
    samples=report['resources']
    if samples:
        metrics['resources']={'process_rss_peak_gb':max(sample['rss_gb'] for sample in samples),'gpu_total_used_peak_gb':max(sample['gpu_total_used_gb'] for sample in samples),'system_available_min_gb':min(sample['system_available_gb'] for sample in samples)}
    metrics['seconds']=report['seconds']
    (directory/'metrics.json').write_text(json.dumps(metrics,indent=2),encoding='utf-8')
    print(json.dumps(metrics),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--character',type=Path,required=True)
    args=parser.parse_args();analyze(args.directory,args.character)
