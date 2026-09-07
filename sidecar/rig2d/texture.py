"""透明な画素だけへ隣接色を延長し、縮小描画時の黒い縁を防ぐ。"""
import numpy as np
from PIL import Image
from scipy import ndimage


def bleed_transparent_rgb(image):
    """可視RGBAと全アルファを維持する。解像度も輪郭も拡大しない。"""
    pixels=np.array(image.convert('RGBA'))
    visible=pixels[:,:,3]>0
    if not visible.any():raise ValueError('透明色補完の参照画素がありません')
    if not visible.all():
        nearest=ndimage.distance_transform_edt(~visible,return_distances=False,return_indices=True)
        pixels[~visible,:3]=pixels[tuple(nearest)][~visible,:3]
    return Image.fromarray(pixels)
