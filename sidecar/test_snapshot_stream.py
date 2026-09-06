"""実OSleaseを使う逐次配信の容量検査・中断・GC保護をCPU検査する。"""
import json
from pathlib import Path
import tempfile
import unittest
from PIL import Image
import reference_store as store
from snapshot_stream import records


class StreamTests(unittest.TestCase):
    def test_stream_pins_generation_and_aborting_releases(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]/'temp') as temporary:
            root=Path(temporary)
            def build(path):
                (path/'parts').mkdir();Image.new('RGBA',(4,5),(1,2,3,255)).save(path/'parts/face.png')
                (path/'rig.json').write_text(json.dumps({'layers':{'face':{'url':'/assets/rig2d/parts/face.png','texture_box':[0,0,4,5]}}}))
                (path/'completion.json').write_text(json.dumps({'outputs':{'rig.json':store.sha(path/'rig.json'),'parts/face.png':store.sha(path/'parts/face.png')}}))
            def publish():return store.publish(root,build,lambda:None)
            original=publish()
            limits={'record_bytes':1024,'chunk_bytes':128,'part_bytes':2048,'total_bytes':4096,'parts':10,'dimension':100}
            stream=records(root,limits);header=json.loads(next(stream));self.assertEqual(header['generation'],original['generation'])
            publish();publish()
            self.assertTrue(store.generation_path(root,original['generation']).exists())
            stream.close()
            self.assertFalse(store.generation_path(root,original['generation']).exists())
            result=[json.loads(line) for line in records(root,limits)]
            self.assertEqual(result[-1]['type'],'complete')
            self.assertEqual([item['name'] for item in result if item['type']=='end'],['rig','face'])
            with self.assertRaises(ValueError):list(records(root,{**limits,'total_bytes':1}))
            self.assertFalse(list((root/'rig-leases').glob('*.lease')))
            current=store.generation_path(root,store.reference(root)['generation'])
            Image.new('RGBA',(4,5),(5,6,7,255)).save(current/'parts/face.png')
            with self.assertRaisesRegex(ValueError,'改変'):list(records(root,limits))
            self.assertFalse(list((root/'rig-leases').glob('*.lease')))


if __name__=='__main__':unittest.main()
