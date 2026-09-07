"""補完の署名、入力保持、破損拒否、排他をCPUだけで検証する。"""
import json
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from PIL import Image
import generate

from generate import cache_valid, tree_hashes, generation_lock, source_hashes
from reference_store import reference, generation_path
from regions import eye_edit_mask, measured_head_region
import numpy as np


class CompletionTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parents[2]/'temp'
        root.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=root)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_cache_rejects_missing_or_modified_materials(self):
        output = self.root/'result'; output.mkdir()
        (output/'rig.json').write_text('{}', encoding='utf-8')
        identity = {'version': 1, 'source': 'first'}
        record = {'identity': identity, 'outputs': tree_hashes(output)}
        (output/'completion.json').write_text(json.dumps(record), encoding='utf-8')
        self.assertTrue(cache_valid(output, identity))
        self.assertFalse(cache_valid(output, {'version': 1, 'source': 'different'}))
        (output/'rig.json').write_text('{"changed":true}', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, '改変'):
            cache_valid(output, identity)
        (output/'rig.json').unlink()
        with self.assertRaisesRegex(ValueError, '欠落'):
            cache_valid(output, identity)

    def test_input_identity_includes_analysis_not_only_original(self):
        for name in ('source/input.png', 'source/isolated.png', 'analysis/analysis.json', 'analysis/masks.npz'):
            path = self.root/name; path.parent.mkdir(exist_ok=True)
            path.write_bytes(b'original')
        before = source_hashes(self.root)
        (self.root/'analysis/masks.npz').write_bytes(b'new mask')
        after = source_hashes(self.root)
        self.assertEqual(before['source/input.png'], after['source/input.png'])
        self.assertNotEqual(before, after)

    def test_two_characters_cannot_generate_simultaneously(self):
        a = self.root/'a'; b = self.root/'b'
        a.mkdir(); b.mkdir()
        with generation_lock(a):
            with self.assertRaises(OSError):
                with generation_lock(b):
                    self.fail('補完の排他が効いていません')
        with generation_lock(b):
            pass

    def test_eye_core_is_full_strength_even_next_to_protected_hair(self):
        eye = np.zeros((64, 96), bool); eye[20:40, 30:65] = True
        hair = np.zeros_like(eye); hair[:, 28:32] = True
        alpha = np.full(eye.shape, 255, np.uint8); alpha[25:27, 40:44] = 0
        mask = eye_edit_mask([eye], hair, alpha, .2)
        core = eye & ~hair & (alpha > 0)
        self.assertTrue(np.all(mask[core] == 255))
        self.assertTrue(np.all(mask[hair | (alpha == 0)] == 0))
        self.assertTrue(np.any((mask > 0) & (mask < 255) & ~eye))
        self.assertEqual(mask.shape, eye.shape)

    def test_edit_mask_translation_preserves_native_pixels(self):
        eye = np.zeros((64, 96), bool); eye[20:40, 30:65] = True
        hair = np.zeros_like(eye); hair[:, 28:32] = True
        alpha = np.full(eye.shape, 255, np.uint8)
        original = eye_edit_mask([eye], hair, alpha, .2)
        pad = ((7, 9), (11, 13))
        shifted = eye_edit_mask([np.pad(eye, pad)], np.pad(hair, pad), np.pad(alpha, pad), .2)
        self.assertTrue(np.array_equal(original, shifted[7:71, 11:107]))

    def test_plateau_preserves_outer_extent_and_protected_pixels(self):
        eye=np.zeros((64,96),bool);eye[20:40,30:65]=True
        hair=np.zeros_like(eye);hair[:,28:32]=True
        alpha=np.full(eye.shape,254,np.uint8);alpha[:4]=0
        old=eye_edit_mask([eye],hair,alpha,.2)
        new=eye_edit_mask([eye],hair,alpha,.2,.5)
        self.assertTrue(np.array_equal(old>0,new>0))
        self.assertTrue(np.all(new[hair|(alpha==0)]==0))
        self.assertGreater(int((new==255).sum()),int((old==255).sum()))
        self.assertTrue(np.all(new>=old))
        shifted=eye_edit_mask([np.pad(eye,((7,9),(11,13)))],np.pad(hair,((7,9),(11,13))),np.pad(alpha,((7,9),(11,13))),.2,.5)
        self.assertTrue(np.array_equal(new,shifted[7:71,11:107]))
        for invalid in (-.01,1,float('nan')):
            with self.assertRaises(ValueError):eye_edit_mask([eye],hair,alpha,.2,invalid)

    def test_native_roi_never_resizes_to_fit_limit(self):
        bounds = measured_head_region([300, 250, 500, 450], [340, 440, 470, 510], (1000, 1200), 1024)
        self.assertEqual((bounds[2]-bounds[0]) % 16, 0)
        with self.assertRaisesRegex(ValueError, '原寸'):
            measured_head_region([300, 250, 500, 450], [340, 440, 470, 510], (1000, 1200), 256)

    def test_edit_mask_preserves_hair_and_transparency(self):
        eye = np.zeros((40, 60), bool); eye[15:25, 20:40] = True
        hair = np.zeros_like(eye); hair[:, 20] = True
        alpha = np.full(eye.shape, 255, np.uint8); alpha[:, 39] = 0
        mask = eye_edit_mask([eye], hair, alpha, .2)
        self.assertTrue(np.any(mask > 0))
        self.assertTrue(np.all(mask[hair | (alpha == 0)] == 0))


class CompletionOrchestrationTests(unittest.TestCase):
    """GPU推論と素材抽出だけを代替し、署名・入力検査・公開処理は実行する。"""

    def setUp(self):
        root = Path(__file__).resolve().parents[2]/'temp'
        root.mkdir(exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=root)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        character = self.root/'character'
        for folder in ('source', 'analysis', 'rig2d-base/parts', 'rig2d', 'comfy', 'models', 'implementation'):
            (character/folder).mkdir(parents=True, exist_ok=True)
        image = Image.new('RGBA', (256, 256), (230, 190, 170, 255))
        image.save(character/'source/input.png')
        image.save(character/'source/isolated.png')
        metadata = {'analysis': {'selected': {'face': {'box': [80, 64, 176, 160]},
                                             'neck': {'box': [100, 160, 156, 190]}}}}
        (character/'analysis/analysis.json').write_text(json.dumps(metadata), encoding='utf-8')
        left = np.zeros((256, 256), bool); left[90:100, 90:110] = True
        right = np.zeros_like(left); right[90:100, 145:165] = True
        np.savez(character/'analysis/masks.npz', left_eye=left, right_eye=right, hair=np.zeros_like(left))
        (character/'rig2d-base/rig.json').write_text('{"layers":{}}', encoding='utf-8')
        image.save(character/'rig2d-base/parts/mouth_closed.png')
        image.save(character/'rig2d-base/parts/left_eyelid_upper.png')
        (character/'rig2d/old-generation.txt').write_text('承認済み旧世代', encoding='utf-8')
        (character/'comfy/main.py').write_text('# 合成テスト用', encoding='utf-8')
        model = character/'models/fixture.bin'; model.write_bytes(b'fixed model')
        implementation = character/'implementation'
        (implementation/'models.json').write_text(json.dumps({'files': [
            {'path': 'split_files/fixture.bin', 'sha256': generate.digest(model)}]}), encoding='utf-8')
        (implementation/'generate.py').write_text('# 合成テストのコード署名', encoding='utf-8')
        workflow = character/'workflow.json'
        workflow.write_text(json.dumps({key: {'inputs': {}} for key in ('7', '10', '13')}), encoding='utf-8')
        overlay = character/'overlay.json'; overlay.write_text('{}', encoding='utf-8')
        self.args = SimpleNamespace(character=character, base_rig=character/'rig2d-base/rig.json',
                                    output=character/'rig2d', comfy=character/'comfy', models=character/'models',
                                    workflow=workflow, overlay=overlay, port=58120, steps=50, seed=777,
                                    resolution=256, mask_margin_ratio=.2, mask_core_ratio=0, startup_timeout=600,
                                    generation_timeout=14400, prompt='Close eyes', fast_disk=True,
                                    grounding_model=character/'models',sam_model=character/'models',
                                    hidden_prompt='Remove hair',side_prompt='Reveal ears',grounding_threshold=.3,
                                    ear_context=.5,hidden_band_ratio=.08,hidden_motion_ratio=.35,
                                    hair_edge_band_ratio=.015,hair_edge_gain=40)
        self.original = source_hashes(character)
        self.baseline = tree_hashes(self.args.base_rig.parent)
        self.previous = tree_hashes(self.final_output())
        self.enterContext(patch.object(generate, 'HERE', implementation))
        self.enterContext(patch.object(generate.importlib.metadata, 'version', return_value='fixture-runtime'))
        self.inference = self.enterContext(patch.object(generate, 'generate_image', side_effect=self.render))
        self.extract = self.enterContext(patch.object(generate, 'apply_closed_eyes', return_value=(
            {'left_eyelid_upper': Image.new('RGBA', (16, 16), (70, 40, 20, 255))}, {'fixture': True})))
        self.hidden = self.enterContext(patch.object(generate,'prepare_hidden',return_value={'identity':{'fixture':1}}))
        self.hidden_apply = self.enterContext(patch.object(generate,'apply_hidden',side_effect=lambda args,rig,base,parts,*rest:(rig,parts,{'fixture':True})))
        self.enterContext(patch.object(generate,'assert_hidden_sources',side_effect=lambda args,prepared,guard:guard()))

    def test_hidden_failure_preserves_eye_cache_and_prior_rig(self):
        self.hidden.side_effect=RuntimeError('耳生成失敗')
        with self.assertRaisesRegex(RuntimeError,'耳生成失敗'):generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()),self.previous)
        self.assertTrue((self.args.character/'completion-source/edited.png').exists())
        self.hidden.side_effect=None
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count,1)

    def test_cached_final_reemits_persisted_partial_warning(self):
        warning='原画の片耳が未検出のため隠れ素材のみを補完しました'
        self.hidden_apply.side_effect=lambda args,rig,base,parts,*rest:(rig,parts,{'warning':warning})
        output=StringIO()
        with redirect_stdout(output):
            generate.complete_locked(self.args)
            output.seek(0);output.truncate()
            generate.complete_locked(self.args)
        events=[json.loads(line) for line in output.getvalue().splitlines()]
        self.assertIn({'event':'completion_warning','message':warning},events)
        self.assertTrue(any(event['event']=='completion_cached' for event in events))
        self.assertEqual(self.inference.call_count,1)
        self.assertEqual(self.hidden_apply.call_count,1)

    def test_analysis_only_change_reuses_eye_and_keeps_original_manifest_bytes(self):
        generate.complete_locked(self.args)
        marker=self.args.character/'completion-source/manifest.json';before=marker.read_bytes()
        old_origin=json.loads(before)['identity']['source']
        analysis=self.args.character/'analysis/analysis.json'
        record=json.loads(analysis.read_text());record['collar_parser_version']=2
        analysis.write_text(json.dumps(record))
        archive=self.args.character/'analysis/masks.npz'
        with np.load(archive) as data:masks={key:data[key].copy() for key in data.files}
        masks['new_arm_mask']=np.zeros_like(masks['hair']);np.savez(archive,**masks)
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count,1);self.assertEqual(self.extract.call_count,2)
        self.assertEqual(marker.read_bytes(),before)
        final=json.loads((self.final_output()/'completion.json').read_text())['identity']
        self.assertEqual(final['raw_origin']['generated_from']['source'],old_origin)
        self.assertEqual(final['source'],source_hashes(self.args.character));self.assertNotEqual(final['source'],old_origin)
        self.assertEqual(final['raw_origin']['manifest_sha256'],generate.digest(marker))

    def test_hidden_wait_cannot_rebaseline_eye_image_mutation(self):
        def mutate(*args,**kwargs):
            path=self.args.character/'completion-source/edited.png'
            with Image.open(path) as opened:image=opened.convert('RGBA')
            image.putpixel((96,96),(0,0,0,255));image.save(path)
            return {'identity':{'fixture':1}}
        self.hidden.side_effect=mutate
        with self.assertRaisesRegex(ValueError,'SHA不一致'):generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()),self.previous)

    def test_hidden_wait_cannot_change_original_eye_manifest(self):
        def mutate(*args,**kwargs):
            marker=self.args.character/'completion-source/manifest.json'
            record=json.loads(marker.read_text());record['material_quality']='tampered'
            marker.write_text(json.dumps(record));return {'identity':{'fixture':1}}
        self.hidden.side_effect=mutate
        with self.assertRaisesRegex(ValueError,'SHA不一致'):generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()),self.previous)

    def test_publish_guard_rechecks_eye_manifest_after_material_application(self):
        def mutate(args,rig,base,parts,*rest):
            marker=args.character/'completion-source/manifest.json'
            record=json.loads(marker.read_text());record['material_quality']='tampered'
            marker.write_text(json.dumps(record));return rig,parts,{}
        self.hidden_apply.side_effect=mutate
        with self.assertRaisesRegex(ValueError,'SHA不一致'):generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()),self.previous)

    def test_hidden_extraction_failure_preserves_previous_output(self):
        self.hidden_apply.side_effect=ValueError('耳組立失敗')
        with self.assertRaisesRegex(ValueError,'耳組立失敗'):generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()),self.previous)
        self.assertTrue((self.args.character/'completion-source/edited.png').exists())

    def final_output(self):
        if (self.args.character/'rig-current.json').exists():
            return generation_path(self.args.character,reference(self.args.character)['generation'])
        return self.args.output

    @staticmethod
    def render(args, run, workflow):
        result = run/'output/edit.png'
        with Image.open(run/'input/input.png') as source:
            source.convert('RGBA').save(result)
        return result

    def test_success_preserves_sources_unrelated_material_and_reuses_cache(self):
        generate.complete_locked(self.args)
        self.assertEqual(source_hashes(self.args.character), self.original)
        self.assertEqual(tree_hashes(self.args.base_rig.parent), self.baseline)
        final = tree_hashes(self.final_output())
        self.assertEqual(final['parts/mouth_closed.png'], self.baseline['parts/mouth_closed.png'])
        self.assertNotEqual(final['parts/left_eyelid_upper.png'], self.baseline['parts/left_eyelid_upper.png'])
        self.assertNotIn('old-generation.txt', final)
        self.assertIn('completion.json', final)
        self.assertFalse([path for path in (self.args.character/'temp').glob('completion-*') if path.is_dir()])
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 1)
        self.assertEqual(self.extract.call_count, 1)
        self.assertEqual(tree_hashes(self.final_output()), final)

    def test_inference_failure_preserves_previous_final(self):
        self.inference.side_effect = RuntimeError('合成推論失敗')
        with self.assertRaisesRegex(RuntimeError, '合成推論失敗'):
            generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()), self.previous)
        self.assertEqual(source_hashes(self.args.character), self.original)
        self.extract.assert_not_called()
        self.assertEqual(len(list((self.args.character/'temp').glob('completion-*/failure.json'))), 1)

    def test_source_and_setting_changes_invalidate_completed_cache(self):
        generate.complete_locked(self.args)
        self.args.seed += 1
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 2)
        self.args.mask_core_ratio = .5
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 3)
        Image.new('RGBA', (256, 256), (230, 191, 170, 255)).save(self.args.character/'source/input.png')
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 4)
        record = json.loads((self.final_output()/'completion.json').read_text(encoding='utf-8'))
        self.assertEqual(record['identity']['source'], source_hashes(self.args.character))

    def test_mid_generation_input_change_rejects_publication(self):
        def change_source(args, run, workflow):
            result = self.render(args, run, workflow)
            Image.new('RGBA', (256, 256), (200, 190, 170, 255)).save(args.character/'source/input.png')
            return result
        self.inference.side_effect = change_source
        with self.assertRaisesRegex(ValueError, '変更されました'):
            generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.final_output()), self.previous)
        self.assertEqual(tree_hashes(self.args.base_rig.parent), self.baseline)

    def test_extraction_code_change_reuses_gpu_image(self):
        generate.complete_locked(self.args)
        before = tree_hashes(self.args.character/'completion-source')
        (generate.HERE/'materials.py').write_text('# 抽出だけの修正', encoding='utf-8')
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 1)
        self.assertEqual(self.extract.call_count, 2)
        self.assertEqual(tree_hashes(self.args.character/'completion-source'), before)

    def test_extraction_failure_keeps_raw_image_for_cpu_retry(self):
        self.extract.side_effect = ValueError('抽出失敗')
        with self.assertRaisesRegex(ValueError, '抽出失敗'):
            generate.complete_locked(self.args)
        self.assertTrue((self.args.character/'completion-source/edited.png').is_file())
        self.assertEqual(tree_hashes(self.final_output()), self.previous)
        self.extract.side_effect = None
        generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 1)
        self.assertEqual(self.extract.call_count, 2)

    def test_new_generation_failure_preserves_prior_raw_cache(self):
        generate.complete_locked(self.args)
        previous = tree_hashes(self.args.character/'completion-source')
        self.args.seed += 1
        self.inference.side_effect = RuntimeError('新条件の推論失敗')
        with self.assertRaisesRegex(RuntimeError, '新条件'):
            generate.complete_locked(self.args)
        self.assertEqual(tree_hashes(self.args.character/'completion-source'), previous)

    def test_corrupt_raw_cache_is_not_silently_used_or_regenerated(self):
        generate.complete_locked(self.args)
        (self.args.character/'completion-source/edited.png').write_bytes(b'broken')
        with self.assertRaisesRegex(ValueError, 'SHA不一致'):
            generate.complete_locked(self.args)
        self.assertEqual(self.inference.call_count, 1)

    def test_missing_model_reports_setup_command_before_inference(self):
        (self.args.models/'fixture.bin').unlink()
        with self.assertRaisesRegex(ValueError, 'cargo xtask setup completion'):
            generate.complete_locked(self.args)
        self.inference.assert_not_called()


if __name__ == '__main__':
    unittest.main()
