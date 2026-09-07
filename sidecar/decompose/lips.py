"""原画の口の合わせ目を追跡し、上下唇の分割境界を測る。"""

import numpy as np
from scipy import ndimage


def trace_lip_seam(rgba, box):
    """局所コントラストと連続性から、各列の唇境界を原画座標で返す。"""
    left, top, right, bottom = box
    gray = rgba[top:bottom, left:right, :3].astype(float).mean(axis=2)
    height, width = gray.shape
    if height < 3 or width < 3 or np.ptp(gray) < 1:
        raise ValueError("原画から唇の合わせ目を測定できません")
    contrast = ndimage.gaussian_filter(gray, (max(1, height / 6), max(1, width / 12))) - gray
    cost = -contrast
    cost[[0, -1]] += max(1, float(np.ptp(contrast)))
    score = cost[:, 0].copy()
    parents = np.zeros((height, width), dtype=int)
    for x in range(1, width):
        choices = np.full((3, height), np.inf)
        choices[0, 1:] = score[:-1] + .5
        choices[1] = score
        choices[2, :-1] = score[1:] + .5
        best = choices.argmin(axis=0)
        parents[:, x] = np.arange(height) + best - 1
        score = cost[:, x] + choices[best, np.arange(height)]
    rows = np.zeros(width, dtype=float)
    row = int(score.argmin())
    for x in range(width - 1, -1, -1):
        rows[x] = row + .5
        row = parents[row, x]
    rows = ndimage.gaussian_filter1d(rows, .75)
    # 画素中心を頂点にする。左右端は隣の測定値を延長し、閉じた分割線にする。
    points = [[left, float(top + rows[0])]]
    points.extend([left + x + .5, float(top + y)] for x, y in enumerate(rows))
    points.append([right, float(top + rows[-1])])
    return points
