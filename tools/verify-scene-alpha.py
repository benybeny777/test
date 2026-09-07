"""正規部位PNGの原寸合成アルファを背景除去原画と比較する。画像は変更しない。"""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def verify(character):
    manifest=json.loads((character/'layers/manifest.json').read_text(encoding='utf-8'))
    with Image.open(character/'source/isolated.png') as image:
        original=np.asarray(image.convert('RGBA'))[:,:,3].astype(np.float64)/255
    composite=np.zeros_like(original)
    parts=[part for part in manifest['parts'] if part['name'].startswith('scene_')]
    if not parts:raise ValueError('独立描画部位がありません')
    for part in sorted(parts,key=lambda part:part['z_index']):
        with Image.open(character/'layers'/part['path']) as image:
            alpha=np.asarray(image.convert('RGBA'))[:,:,3].astype(np.float64)/255
        if alpha.shape!=original.shape:raise ValueError('正規レイヤーのキャンバス寸法が一致しません')
        target=composite
        target[:]=alpha+target*(1-alpha)
    delta=np.abs(composite-original)
    result={'character':character.name,'scene_parts':len(parts),'changed_pixels':int(np.count_nonzero(delta>1e-12)),'max_alpha_delta':float(delta.max())}
    print(json.dumps(result),flush=True)
    if result['changed_pixels']:raise ValueError('原寸合成アルファが原画と一致しません')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('characters',nargs='+',type=Path)
    args=parser.parse_args()
    for character in args.characters:verify(character)
