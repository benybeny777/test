"""通常解析の袖・手候補を意味領域との関係で選別する。品質判定は別途行う。"""
import numpy as np

OPTIONAL_ROLES={'left_sleeve','right_sleeve','left_hand','right_hand'}


def propose(candidates,masks):
    """検出語句だけで採用せず、所有競合と未観測を残す。任意の較正閾値は置かない。"""
    proposals={};records=[]
    for candidate in candidates:
        if candidate['role'] not in ('hand','sleeve'):raise ValueError('未対応の役割です')
        mask=candidate['mask']
        if mask.dtype!=bool or mask.shape!=masks['face'].shape:raise ValueError('候補マスクが不正です')
        counts={name:int((mask&masks[name]).sum()) for name in ('face','hair','clothes','left_arm','right_arm')}
        side='left' if counts['left_arm']>counts['right_arm'] else 'right'
        same=counts[side+'_arm'];opposite=counts[('right' if side=='left' else 'left')+'_arm']
        reasons=[]
        if not mask.any():reasons.append('empty')
        if same==0:reasons.append('not_observed_on_arm')
        if same==opposite:reasons.append('side_ambiguous')
        if counts['face']+counts['hair']>=same:reasons.append('face_hair_conflict')
        if candidate['role']=='sleeve' and counts['clothes']==0:reasons.append('no_clothing_evidence')
        # 顔髪や反対腕との少量の重なりも隠さず記録する。全画素が正しい保証ではない。
        record={'id':candidate['id'],'role':candidate['role'],'screen_side':side,'score':candidate['score'],
                'pixels':int(mask.sum()),'overlaps':counts,'rejections':reasons,'quality':'unverified'}
        records.append(record)
        if reasons:continue
        role=side+'_'+candidate['role']
        current=proposals.get(role)
        if current is None or (-candidate['score'],candidate['id'])<(-current['score'],current['id']):proposals[role]=candidate
    return proposals,{'candidates':records,'roles':{role:'candidate_needs_review' if role in proposals else 'not_observed' for role in sorted(OPTIONAL_ROLES)},'independent_joints':False}
