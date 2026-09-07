"""保存済み袖・手候補を既存腕の所有内で分割する。"""
import numpy as np

OPTIONAL_ROLES = ('left_sleeve','left_hand','right_sleeve','right_hand')


def select(candidates,masks):
    from limbs import propose
    chosen,report=propose(candidates,masks)
    return {role:item['mask'] for role,item in chosen.items()},report


def partition(owners, masks):
    """同側の腕だけを移管する。未知の曖昧候補は明示状態で既存所有を保つ。"""
    result={key:value.copy() for key,value in owners.items()};status={}
    for side in ('left','right'):
        parent=side+'_arm';parts={}
        for kind in ('sleeve','hand'):
            role=side+'_'+kind
            if role not in masks:status[role]='not_observed';continue
            allowed=masks[role]&owners[parent]
            if not allowed.any():status[role]='not_observed_on_arm';continue
            parts[role]=allowed
        if len(parts)==2 and np.logical_and.reduce(list(parts.values())).any():
            status.update({role:'ambiguous_sleeve_hand_overlap' for role in parts});continue
        consumed=np.zeros_like(owners[parent])
        for mask in parts.values():consumed|=mask
        if parts and not (owners[parent]&~consumed).any():
            status.update({role:'would_empty_parent_arm' for role in parts});continue
        for role,mask in parts.items():
            result[parent]&=~mask;result[role]=mask;status[role]='visible_partition_unverified'
    subject=np.logical_or.reduce(list(owners.values()))
    if not np.array_equal(sum(mask.astype(np.uint8) for mask in result.values()),subject.astype(np.uint8)):
        raise ValueError('可視所有に欠落または重複があります')
    return result,{'roles':status,'independent_joints':False,'motion':'inherit_parent_arm'}


def validate_graph(parts,graph):
    for role in OPTIONAL_ROLES:
        entries=[item for item in graph if item['role']==role]
        if len(entries)>1 or ('scene_'+role in parts)!=(len(entries)==1):
            raise ValueError('任意部位と構造が一致しません')
        if entries and (entries[0]['parent']!=role.split('_')[0]+'_arm' or entries[0].get('motion')!='inherit_parent_arm'):
            raise ValueError('未補完の袖・手は腕の変位を継承してください')
