"""接続候補の実コードをCPU fixtureで検査する。推論・HTTP待受は起動しない。"""
import ast
import io
import json
import os
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from PIL import Image
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'sidecar'))
import reference_store as store
import preview_server as server

REPO=Path(__file__).resolve().parents[1]
HERE=REPO/'temp/generation-reference'
HERE.mkdir(parents=True,exist_ok=True)


class IntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=HERE);self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.character=self.root/'temp/t7-characters/c_123456789abc'
        self.character.mkdir(parents=True)

    def build(self,path):
        (path/'parts').mkdir();Image.new('RGBA',(4,5)).save(path/'parts/face.png')
        (path/'rig.json').write_text(json.dumps({'layers':{'face':{'url':'/assets/rig2d/parts/face.png','texture_box':[0,0,4,5]}}}))
        (path/'completion.json').write_text(json.dumps({'outputs':{'rig.json':store.sha(path/'rig.json'),'parts/face.png':store.sha(path/'parts/face.png')}}))

    def test_http_handler_disconnect_closes_actual_disk_lease(self):
        store.publish(self.character,self.build,lambda:None)
        handler=object.__new__(server.PreviewHandler)
        handler.path='/api/characters/c_123456789abc/snapshot';handler.responses=[]
        handler.send_response=lambda value:handler.responses.append(value)
        handler.send_header=lambda *a:None;handler.end_headers=lambda:None;handler.log_error=lambda *a:None
        class Broken:
            def write(self,data):raise BrokenPipeError('fixture')
        handler.wfile=Broken()
        with patch.object(server,'ROOT',self.root),patch.object(server,'snapshot_limits',return_value=server.snapshot_limits(REPO,self.root/'missing.json')):
            handler.do_GET()
        self.assertEqual(handler.responses,[200])
        self.assertEqual(list((self.character/'rig-leases').iterdir()),[])

    def test_running_character_with_published_generation_remains_in_index(self):
        store.publish(self.character,self.build,lambda:None)
        for status in ('running','failed'):
            (self.character/'character.json').write_text(json.dumps({'displayName':'fixture','model':{'rig2d_base':'rig2d-base/rig.json'},'stages':{'complete':{'status':status}}}))
            self.assertEqual(len(server.normal_characters(self.root)),1)

    def test_settings_are_file_over_environment_over_rust_default_and_reload(self):
        config=self.root/'config.json'
        defaults=server.snapshot_limits(REPO,config)
        with patch.dict(os.environ,{'LVS_DISPLAY_SNAPSHOT_PARTS':'99'}):
            self.assertEqual(server.snapshot_limits(REPO,config)['parts'],99)
            config.write_text('{"display":{"snapshot_parts":77}}')
            self.assertEqual(server.snapshot_limits(REPO,config)['parts'],77)
            config.write_text('{"display":{"snapshot_parts":78}}')
            self.assertEqual(server.snapshot_limits(REPO,config)['parts'],78)
        self.assertEqual(defaults['parts'],256)

    def test_completion_actual_publish_callbacks_preserve_source_gate(self):
        # GPU等をimportせず、適用候補中の実build/verify関数を同じfixture変数で実行する。
        tree=ast.parse((REPO/'sidecar/completion/generate.py').read_text(encoding='utf-8'))
        definitions=[node for node in ast.walk(tree) if isinstance(node,ast.FunctionDef) and node.name in ('build_final','verify_final_sources')]
        self.assertEqual(len(definitions),2)
        base=self.root/'base';base.mkdir();self.build(base);(base/'completion.json').unlink()
        run=self.root/'run';(run/'input').mkdir(parents=True)
        code=self.root/'code';code.mkdir();edited=self.root/'edited.png';edited.write_bytes(b'fixture')
        hashes=lambda directory:{str(p.relative_to(directory)).replace('\\','/'):store.sha(p) for p in directory.rglob('*') if p.is_file()}
        source={'input':'a','source':'b','analysis':'c','masks':'d'}
        actual=dict(source)
        values={'args':SimpleNamespace(character=self.character),'base':base,'rig':json.loads((base/'rig.json').read_text()),'materials':{},'measurements':{},'VERSION':3,'hidden_report':{},'identity':{'edited_sha256':store.sha(edited)},'baseline':hashes(base),'tree_hashes':hashes,'digest':store.sha,'source':source,'source_hashes':lambda _:dict(actual),'run':run,'inputs':hashes(run/'input'),'HERE':code,'extraction_code':{},'edited_path':edited,'prepared_hidden':{},'guard':{},'assert_hidden_sources':lambda *a:None,'bounds':[0,0,4,5],'shutil':shutil,'json':json}
        values['eye_lease']=SimpleNamespace(image_sha256=store.sha(edited),recheck=lambda:None)
        exec(compile(ast.Module(body=definitions,type_ignores=[]),str(REPO/'sidecar/completion/generate.py'),'exec'),values)
        first=store.publish(self.character,values['build_final'],values['verify_final_sources'])
        for key in source:
            actual[key]='changed'
            with self.assertRaises(ValueError):store.publish(self.character,values['build_final'],values['verify_final_sources'])
            self.assertEqual(store.reference(self.character),first)
            actual[key]=source[key]

    def test_cached_published_rig_keeps_hidden_partial_warning(self):
        def build(path):
            self.build(path)
            rig=json.loads((path/'rig.json').read_text());rig['local_completion']={'hidden':{'warning':'耳未検出・部分補完'}}
            (path/'rig.json').write_text(json.dumps(rig))
        store.publish(self.character,build,lambda:None)
        tree=ast.parse((REPO/'sidecar/completion/generate.py').read_text(encoding='utf-8'))
        branch=next(n for n in ast.walk(tree) if isinstance(n,ast.If) and isinstance(n.test,ast.Call) and isinstance(n.test.func,ast.Attribute) and n.test.func.attr=='lexists')
        wrapper=ast.parse('def cached():\n    pass')
        wrapper.body[0].body=[branch]
        events=[]
        values={'os':os,'args':SimpleNamespace(character=self.character),'acquire_rig':store.acquire,'cache_valid':lambda *a:True,'identity':{},'assert_hidden_sources':lambda *a:None,'prepared_hidden':{},'guard':{},'json':json,'emit':lambda event,**kw:events.append((event,kw)),'print':lambda *a,**kw:None}
        values['eye_lease']=SimpleNamespace(recheck=lambda:None)
        exec(compile(ast.fix_missing_locations(wrapper),str(REPO/'sidecar/completion/generate.py'),'exec'),values)
        values['cached']()
        self.assertEqual(events,[('completion_warning',{'message':'耳未検出・部分補完'})])


if __name__=='__main__':unittest.main()
