"""動かす機能単位で素材分割の充足を検査する。画像枚数では判定しない。"""

PROFILE = "articulated-materials-v1"
CORE_ROLES = frozenset({
    "face_base", "neck", "torso", "mouth_upper", "mouth_lower", "mouth_cavity",
    "hair_front", "hair_back", "hair_side_left", "hair_side_right",
    *{f"{side}_{part}" for side in ("left", "right") for part in (
        "eyebrow", "eye_sclera", "eye_iris", "eyelid_upper", "eyelid_lower",
        "upper_arm", "forearm", "hand",
    )},
})
OPTIONAL_ROLES = frozenset({"nose", "neck_shadow", "collar", "mouth_teeth", "mouth_tongue",
    "left_ear", "right_ear", "left_eye_highlight", "right_eye_highlight"})


def assess_materials(materials):
    """初期全身プロファイルのメタデータだけを診断する。画像品質は保証しない。"""
    roles = {}
    issues = []
    for item in materials:
        role = item.get("role")
        if not role:
            continue
        if role in roles:
            issues.append(f"機能単位が重複しています: {role}")
        roles[role] = item
        if not item.get("path") or not item.get("source_region"):
            issues.append(f"実素材または元画像の領域がありません: {role}")
        if item.get("status") != "verified":
            issues.append(f"分離・補完を未検証です: {role}")
        if role.endswith("eye_iris") and item.get("clip_to") != role.replace("eye_iris", "eye_sclera"):
            issues.append(f"瞳のクリッピング先が不正です: {role}")
        if role == "neck" and item.get("parent") != "torso":
            issues.append("首と胴体の接続が未定義です")
    missing = sorted(CORE_ROLES - roles.keys())
    return {
        "profile": PROFILE,
        "status": "metadata_ready" if not missing and not issues else "incomplete",
        "visual_status": "unverified",
        "missing_roles": missing,
        "issues": issues,
        "note": "工程の正常終了は素材品質の合格を意味しません",
    }
