import copy,json,tempfile,unittest,os,stat,subprocess
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path
from raw_reuse import SOURCES,sha,open_raw,semantic_identity,read_plain,pin_generated

def identity(kind):
    common={'version':3,'source':{name:sha(name.encode()) for name in SOURCES},'models':{'fixed':'model'},
        'runtime':{'torch':'fixed'},'workflow':sha(b'workflow'),'source_region':[0,0,64,64],
        'parameters':{'steps':50,'seed':777,'fast_disk':True,'prompt':'fixture'}}
    if kind=='eye':
        common.update(comfy_code={'main.py':'fixed'},overlay=sha(b'overlay'),inputs={'input.png':sha(b'input'),'eye-mask.png':sha(b'mask')},resolved_workflow={'node':'fixed'})
        common['parameters'].update(resolution=1024,mask_margin_ratio=.2,mask_core_ratio=.5)
    else:
        common.update(version=1,job=kind,prepared_input_sha256=sha(b'input'),mask_sha256=sha(b'mask'),
            overlay_sha256=sha(b'overlay'),masked_generation_version=3 if kind=='hidden-face' else 2,upscaled=False,output_adoption='measured-hidden-or-ear-region-only')
    return common

class ReuseTests(unittest.TestCase):
    def setUp(self):
        directory=tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parent)
        self.addCleanup(directory.cleanup);self.root=Path(directory.name)
    def save(self,kind):
        cache=self.root/kind;cache.mkdir();(cache/'edited.png').write_bytes(b'raw fixture')
        key='edited_sha256' if kind=='eye' else 'image_sha256'
        record={'identity':identity(kind),key:sha(b'raw fixture'),'status':'generated'}
        (cache/'manifest.json').write_text(json.dumps(record));return cache,record['identity']
    def test_analysis_only_reuse_preserves_original_manifest_and_records_both_origins(self):
        for kind in ('eye','hidden-face','side-ears'):
            cache,wanted=self.save(kind);before=(cache/'manifest.json').read_bytes()
            wanted['source']['analysis/analysis.json']=sha(b'new collar code')
            wanted['source']['analysis/masks.npz']=sha(b'new arms')
            lease=open_raw(cache,wanted,kind);self.assertIsNotNone(lease)
            self.assertEqual((cache/'manifest.json').read_bytes(),before)
            self.assertNotEqual(lease.origin()['generated_from']['source'],wanted['source'])
            self.assertEqual(lease.image_bytes(),b'raw fixture');lease.recheck()
    def test_each_actual_input_change_invalidates_raw(self):
        for kind in ('eye','hidden-face','side-ears'):
            cache,wanted=self.save(kind)
            variants=[]
            value=copy.deepcopy(wanted);value['source_region']=[1,0,65,64];variants.append(value)
            value=copy.deepcopy(wanted);value['parameters']['prompt']='new';variants.append(value)
            value=copy.deepcopy(wanted);value['parameters']['fast_disk']=False;variants.append(value)
            value=copy.deepcopy(wanted);value['models']['fixed']='changed';variants.append(value)
            value=copy.deepcopy(wanted);value['source']['source/input.png']=sha(b'new original');variants.append(value)
            value=copy.deepcopy(wanted)
            if kind=='eye':value['inputs']['eye-mask.png']=sha(b'one pixel')
            else:value['mask_sha256']=sha(b'one pixel')
            variants.append(value)
            for value in variants:self.assertIsNone(open_raw(cache,value,kind))
    def test_unknown_or_incomplete_contract_is_rejected(self):
        cache,wanted=self.save('eye')
        for key in ('inputs','overlay','runtime'):
            value=copy.deepcopy(wanted);value.pop(key)
            with self.assertRaises(ValueError):open_raw(cache,value,'eye')
        value=copy.deepcopy(wanted);value['version']=999
        with self.assertRaises(ValueError):open_raw(cache,value,'eye')
    def test_saved_legacy_shapes_regenerate_without_rewriting_provenance(self):
        shapes=json.loads((Path(__file__).parent/'legacy_eye_contract_shapes.json').read_text())
        cache,wanted=self.save('eye')
        for shape in shapes:
            previous=copy.deepcopy(wanted)
            previous['version']=shape['version'];previous['parameters'].pop('mask_core_ratio')
            previous['runtime']={key:'fixture-version' for key in shape['runtime_keys']}
            self.assertEqual(sorted(previous),shape['identity_keys'])
            self.assertEqual(sorted(previous['parameters']),shape['parameter_keys'])
            self.assertEqual(sorted(previous['source']),shape['source_keys'])
            self.assertEqual(sorted(previous['inputs']),shape['input_keys'])
            record={'identity':previous,'edited_sha256':sha(b'raw fixture'),'status':'generated'}
            before=json.dumps(record).encode();(cache/'manifest.json').write_bytes(before)
            self.assertIsNone(open_raw(cache,wanted,'eye'))
            self.assertEqual((cache/'manifest.json').read_bytes(),before)
            with self.assertRaises(ValueError):semantic_identity(previous,'eye',requested=True)
    def test_legacy_corruption_is_rejected_before_version_cache_miss(self):
        cache,wanted=self.save('eye')
        for version in (1,2):
            previous=copy.deepcopy(wanted);previous['version']=version
            previous['parameters'].pop('mask_core_ratio')
            record={'identity':previous,'edited_sha256':sha(b'raw fixture')}
            (cache/'manifest.json').write_text(json.dumps(record))
            (cache/'edited.png').write_bytes(b'corrupt raw')
            with self.assertRaises(ValueError):open_raw(cache,wanted,'eye')
            (cache/'edited.png').write_bytes(b'raw fixture')
            record['identity']['parameters'].pop('prompt')
            (cache/'manifest.json').write_text(json.dumps(record))
            with self.assertRaises(ValueError):open_raw(cache,wanted,'eye')
    def test_long_hidden_wait_cannot_rebaseline_mutated_eye_image_or_manifest(self):
        cache,wanted=self.save('eye');lease=open_raw(cache,wanted,'eye')
        # 長い後工程の間に画像と元manifestを同時変更しても、取得時の出自を維持する。
        record=json.loads((cache/'manifest.json').read_bytes())
        (cache/'edited.png').write_bytes(b'changed');record['edited_sha256']=sha(b'changed')
        (cache/'manifest.json').write_text(json.dumps(record))
        with self.assertRaises(ValueError):lease.recheck()
        with self.assertRaises(ValueError):lease.image_bytes()
    def test_current_source_guard_still_blocks_publication_after_reuse(self):
        cache,wanted=self.save('eye');lease=open_raw(cache,wanted,'eye');current=copy.deepcopy(wanted['source'])
        def guard():
            if current!=wanted['source']:raise ValueError('今回の解析が実行中に変更されました')
            lease.recheck()
        guard();current['analysis/analysis.json']=sha(b'mid run')
        with self.assertRaises(ValueError):guard()

    def test_reparse_attribute_on_ancestor_is_rejected(self):
        cache,wanted=self.save('eye')
        real=Path.lstat
        def attributes(path):
            if path==cache:return SimpleNamespace(st_mode=stat.S_IFDIR,st_file_attributes=stat.FILE_ATTRIBUTE_REPARSE_POINT)
            return real(path)
        with patch.object(Path,'lstat',attributes):
            with self.assertRaisesRegex(ValueError,'reparse'):open_raw(cache,wanted,'eye')

    def test_hidden_v2_regenerates_but_side_v2_keeps_its_contract(self):
        cache,wanted=self.save('hidden-face');marker=cache/'manifest.json'
        record=json.loads(marker.read_bytes());record['identity']['masked_generation_version']=2
        marker.write_text(json.dumps(record));before=marker.read_bytes()
        self.assertIsNone(open_raw(cache,wanted,'hidden-face'))
        self.assertEqual(marker.read_bytes(),before)
        (cache/'edited.png').write_bytes(b'corrupt')
        with self.assertRaises(ValueError):open_raw(cache,wanted,'hidden-face')
        side,requested=self.save('side-ears');self.assertIsNotNone(open_raw(side,requested,'side-ears'))
        requested['masked_generation_version']=3
        with self.assertRaises(ValueError):open_raw(side,requested,'side-ears')
        wanted['masked_generation_version']=4
        with self.assertRaises(ValueError):open_raw(cache,wanted,'hidden-face')

    @unittest.skipUnless(os.name=='nt','Windowsの実junction検査')
    def test_real_junction_ancestor_rejects_existing_and_new_raw(self):
        cache,wanted=self.save('eye');junction=self.root/'junction'
        # 自分の一時fixtureだけをjunction先とし、製品データには触れない。
        command="New-Item -ItemType Junction -Path $env:RAW_TEST_JUNCTION -Target $env:RAW_TEST_TARGET -ErrorAction Stop | Out-Null"
        environment=dict(os.environ,RAW_TEST_JUNCTION=str(junction),RAW_TEST_TARGET=str(cache))
        result=subprocess.run(['pwsh','-NoProfile','-Command',command],env=environment,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.addCleanup(lambda:junction.rmdir() if junction.exists() else None)
        self.assertTrue(junction.is_junction())
        manifest=(cache/'manifest.json').read_bytes()
        with self.assertRaisesRegex(ValueError,'reparse'):read_plain(junction/'edited.png')
        with self.assertRaisesRegex(ValueError,'reparse'):open_raw(junction,wanted,'eye')
        with self.assertRaisesRegex(ValueError,'reparse'):pin_generated(junction,manifest,wanted,'eye')

if __name__=='__main__':unittest.main()
