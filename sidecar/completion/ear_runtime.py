"""既存DINO/SAMを逐次ロードする耳解析実装。importだけではGPUを使わない。"""
import gc
import os
import numpy as np
from scipy import ndimage


def choose_ears(detections, source_reference):
    if not detections.get('face'):
        raise ValueError('耳解析対象の顔を検出できません')
    face = max(detections['face'], key=lambda pair: pair[1])[0]
    cx = (face[0]+face[2])/2
    boxes = []
    # 幅0.6は既存比較segment_ears.pyの顔外誤検出除外と同じ条件。
    for left in (True, False):
        candidates = [(box, score) for box, score in detections.get('ear', [])
                      if ((box[0]+box[2])/2 < cx) == left and 0 < box[2]-box[0] < (face[2]-face[0])*.6]
        if not candidates:
            if not source_reference:
                raise ValueError('生成耳を左右とも検出できません。固定座標で補いません')
            boxes.append(None)
        else:
            boxes.append(max(candidates, key=lambda pair: pair[1]))
    return boxes


def segment_ear_image(image, detector_path, sam_path, threshold, context, source_reference, emit):
    """戻る前にCUDA参照を解放する。呼出側はComfyUI停止を先に確認する。"""
    if not 0 <= threshold <= 1 or not 0 < context <= 2:
        raise ValueError('耳解析の閾値または余白が不正です')
    for path in (detector_path, sam_path):
        if not path.is_dir():
            raise ValueError('既存解析モデルがありません。cargo xtask setup grounding / sam2 を実行してください')
    os.environ['HF_HUB_OFFLINE'] = '1'
    os.environ['TRANSFORMERS_OFFLINE'] = '1'
    import torch
    from transformers import AutoProcessor, AutoModelForZeroShotObjectDetection, Sam2Processor, Sam2VideoModel
    if not torch.cuda.is_available():
        raise RuntimeError('既存DINO/SAMのCUDA実行環境がありません')
    model = processor = inputs = outputs = result = raw = None
    detections = {}
    try:
        emit('ear_detector_loading')
        processor = AutoProcessor.from_pretrained(detector_path, local_files_only=True)
        model, info = AutoModelForZeroShotObjectDetection.from_pretrained(detector_path, local_files_only=True, output_loading_info=True)
        if any(info.get(key) for key in ('missing_keys', 'mismatched_keys', 'error_msgs')):
            raise ValueError('DINOの既存重みが不完全です')
        model = model.to('cuda').eval()
        for label in ('face', 'ear'):
            inputs = processor(images=image, text=label+'.', return_tensors='pt').to('cuda')
            with torch.inference_mode():
                outputs = model(**inputs)
            result = processor.post_process_grounded_object_detection(outputs, inputs.input_ids,
                threshold=threshold, text_threshold=threshold, target_sizes=[(image.height, image.width)])[0]
            detections[label] = list(zip(result['boxes'].cpu().tolist(), result['scores'].cpu().tolist()))
    finally:
        model = processor = inputs = outputs = result = raw = None
        gc.collect(); torch.cuda.empty_cache()
        emit('ear_detector_released')
    boxes = choose_ears(detections, source_reference)
    masks = [np.zeros((image.height, image.width), bool) for _ in boxes]
    if not any(boxes):
        return masks, {'status': 'partial', 'reason': 'source_ears_not_detected', 'boxes': boxes, 'detections': detections}
    try:
        emit('ear_sam_loading')
        processor = Sam2Processor.from_pretrained(sam_path, local_files_only=True)
        model, info = Sam2VideoModel.from_pretrained(sam_path, local_files_only=True, output_loading_info=True)
        if any(info.get(key) for key in ('missing_keys', 'unexpected_keys', 'mismatched_keys', 'error_msgs')):
            raise ValueError('SAMの既存重みが不完全です')
        model = model.to('cuda').eval()
        for index, entry in enumerate(boxes):
            if entry is None:
                continue
            box, _ = entry
            l, t, r, b = box; mx = (r-l)*context; my = (b-t)*context
            region = [max(0, int(l-mx)), max(0, int(t-my)), min(image.width, int(np.ceil(r+mx))), min(image.height, int(np.ceil(b+my)))]
            if region[2] <= region[0] or region[3] <= region[1]:
                raise ValueError('耳の解析範囲が原画外です')
            sample = image.crop(region); local = [value-region[i % 2] for i, value in enumerate(box)]
            inputs = processor(images=sample, input_boxes=[[local]], return_tensors='pt').to('cuda')
            with torch.inference_mode():
                outputs = model._single_frame_forward(**inputs)
            raw = processor.post_process_masks(outputs.pred_masks.cpu().unsqueeze(0), inputs['original_sizes'].cpu())[0]
            raw = raw.reshape(-1, sample.height, sample.width)[0].numpy().astype(bool)
            components, count = ndimage.label(raw)
            if not count:
                raise ValueError('検出した耳の原寸SAMマスクが空です')
            sizes = np.bincount(components.ravel()); sizes[0] = 0
            masks[index][region[1]:region[3], region[0]:region[2]] = components == sizes.argmax()
    finally:
        model = processor = inputs = outputs = result = raw = None
        gc.collect(); torch.cuda.empty_cache()
        emit('ear_sam_released')
    return masks, {'status': 'complete' if all(boxes) else 'partial', 'boxes': boxes, 'detections': detections}
