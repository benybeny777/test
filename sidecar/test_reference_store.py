"""Windowsの実ファイル/実OSロックで公開参照を検査する。"""
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch

import reference_store as store

ROOT=Path(__file__).resolve().parents[1]/'temp'


class StoreTests(unittest.TestCase):
    def test_empty_or_partial_stale_lease_does_not_block_collection(self):
        self.publish('first')
        leases=self.root/'rig-leases';leases.mkdir()
        for index,data in enumerate((b'',b'0g_12',b'invalid-content')):
            (leases/f'l_{index:032x}.lease').write_bytes(data)
        store.collect(self.root)
        self.assertFalse(list(leases.iterdir()))

    def test_pending_is_not_collected_before_atomic_publication(self):
        self.publish('first')
        def build(path):
            (path/'rig.json').write_text('{}');(path/'completion.json').write_text('{}')
            store.collect(self.root)
            self.assertTrue(path.exists())
        store.publish(self.root,build,lambda:None)

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=ROOT);self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)

    def publish(self,value):
        def build(path):
            (path/'rig.json').write_text(json.dumps({'value':value}))
            (path/'completion.json').write_text('{}')
            (path/'parts').mkdir();(path/'parts/test.png').write_bytes(value.encode())
        return store.publish(self.root,build,lambda:None)

    def test_live_reader_pins_old_generation_and_release_collects_it(self):
        first=self.publish('first')
        with store.acquire(self.root) as (_,fixed):
            self.publish('second');self.publish('third');store.collect(self.root)
            self.assertEqual((fixed/'parts/test.png').read_bytes(),b'first')
            with store.acquire(self.root) as (_,current):self.assertEqual((current/'parts/test.png').read_bytes(),b'third')
        self.assertFalse(store.generation_path(self.root,first['generation']).exists())
        self.assertEqual(len(list((self.root/'rig-generations').iterdir())),2)

    def test_failed_reference_replace_preserves_current(self):
        first=self.publish('first')
        with patch.object(store.os,'replace',side_effect=PermissionError('fixture')):
            with self.assertRaises(PermissionError):self.publish('failed')
        self.assertEqual(store.reference(self.root),first)
        with store.acquire(self.root) as (_,path):self.assertEqual((path/'parts/test.png').read_bytes(),b'first')

    def test_windows_generation_rename_retries_only_temporary_access_denial(self):
        source=self.root/'source';target=self.root/'target';source.mkdir()
        original=Path.rename;attempts=[]
        def flaky(path,destination):
            attempts.append((path,destination))
            if len(attempts)<3:raise PermissionError('fixture')
            return original(path,destination)
        with patch.object(store.os,'name','nt'),patch.object(Path,'rename',flaky),patch.object(store.time,'sleep') as sleep:
            store.rename_generation(source,target)
        self.assertTrue(target.is_dir());self.assertEqual(len(attempts),3)
        self.assertEqual([call.args[0] for call in sleep.call_args_list],[.02,.04])

    def test_windows_generation_rename_keeps_persistent_access_denial(self):
        source=self.root/'source';source.mkdir()
        with patch.object(store.os,'name','nt'),patch.object(Path,'rename',side_effect=PermissionError('fixture')),patch.object(store.time,'sleep') as sleep:
            with self.assertRaises(PermissionError):store.rename_generation(source,self.root/'target')
        self.assertEqual(sleep.call_count,4)

    def test_crashed_reader_lease_is_reclaimed_by_os_lock_probe(self):
        self.publish('first')
        code="import sys,os;sys.path.insert(0,sys.argv[1]);import reference_store as s;ctx=s.acquire(sys.argv[2]);ctx.__enter__();os._exit(0)"
        subprocess.run([sys.executable,'-c',code,str(Path(store.__file__).parent),str(self.root)],check=True,timeout=15)
        self.assertEqual(len(list((self.root/'rig-leases').glob('*.lease'))),1)
        store.collect(self.root)
        self.assertFalse(list((self.root/'rig-leases').glob('*.lease')))

    def test_failed_source_verification_never_publishes(self):
        first=self.publish('first')
        def build(path):
            (path/'rig.json').write_text('{}');(path/'completion.json').write_text('{}')
        def invalid():raise ValueError('source changed')
        with self.assertRaisesRegex(ValueError,'source changed'):store.publish(self.root,build,invalid)
        self.assertEqual(store.reference(self.root),first)

    def test_current_corruption_and_path_escape_are_errors(self):
        current=self.publish('first')
        current['generation']='../rig2d'
        (self.root/'rig-current.json').write_text(json.dumps(current))
        with self.assertRaises(ValueError):
            with store.acquire(self.root):pass
        with self.assertRaises(ValueError):store.decode_reference(b' '*1025)


if __name__=='__main__':unittest.main()
