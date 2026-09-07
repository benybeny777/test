import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from PIL import Image


MODULE_PATH = Path(__file__).with_name("generate.py")
SPEC = importlib.util.spec_from_file_location("rig2d_generate", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class RigCreationTests(unittest.TestCase):
    def test_writes_draw_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "manifest.json"
            parts_dir = root / "parts"
            parts_dir.mkdir()
            parts = [
                {
                    "name": name,
                    "z_index": index,
                    "pivot": [0.5, 0.5],
                    "bbox": [0, 0, 80, 120],
                    "path": f"parts/{name}.png",
                    "lip_seam": [[12,20],[18,20],[24,20]] if name == 'mouth_closed' else None,
                    "eye_aperture": [[12,20,26,24],[24,20,26,24]] if name.endswith(('eye_base','eyelid_upper','eye_closed')) else None,
                }
                for index, name in enumerate(sorted(MODULE.REQUIRED_PARTS))
            ]
            for part in parts:
                image=Image.new("RGBA", (80, 120))
                image.paste((100,90,80,255),(11,17,25,32))
                image.save(root / part["path"])
            graph=[{'role':part['name'].removeprefix('scene_'),'layer':part['name'],'parent':None,'owned_pixels':1}
                   for part in parts if part['name'].startswith('scene_')]
            manifest.write_text(
                json.dumps({"schema_version": 2, "canvas": {"width": 80, "height": 120}, "parts": parts,'scene_graph':graph}),
                encoding="utf-8",
            )
            output = MODULE.create_rig(manifest, root / "output" / "rig.json")
            rig = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(rig["profile"], "lvs-anime25d-v1")
            self.assertEqual(rig['schema_version'],3)
            self.assertEqual(rig['layers']['neutral']['texture_box'],[11,17,25,32])
            with Image.open(root/'output/parts/neutral.png') as image:
                self.assertEqual(image.size,(14,15))
                self.assertEqual(image.getpixel((0,0)),(100,90,80,255))
            self.assertEqual(set(rig["draw_order"]), MODULE.REQUIRED_PARTS)
            self.assertFalse(Path(rig["layers_manifest"]).is_absolute())
            self.assertTrue((root / "output" / "parts" / "mouth_open.png").is_file())
            # 任意の襟sceneも構造検査・コピー・描画順へ引き継ぐ。
            collar=dict(parts[0],name='scene_collar',path='parts/scene_collar.png',z_index=36)
            Image.new('RGBA',(80,120),(100,90,80,255)).save(root/collar['path'])
            parts.append(collar)
            graph.append({'role':'collar','layer':'scene_collar','parent':'torso','owned_pixels':80*120})
            manifest.write_text(json.dumps({'schema_version':2,'canvas':{'width':80,'height':120},'parts':parts,'scene_graph':graph}),encoding='utf-8')
            output=MODULE.create_rig(manifest,root/'output'/'rig.json')
            rig=json.loads(output.read_text(encoding='utf-8'))
            self.assertIn('scene_collar',rig['layers'])
            self.assertIn('scene_collar',rig['draw_order'])
            self.assertIn(graph[-1],rig['scene_graph'])
            original = output.read_bytes()
            for defect in ("empty", "size", "corrupt", "duplicate", "outside", "seam", "eye", "eye_mismatch", "graph_missing", "graph_cycle"):
                with self.subTest(defect=defect):
                    data = json.loads(manifest.read_text(encoding="utf-8"))
                    target = root / parts[-1]["path"]
                    Image.new("RGBA", (80, 120), (100, 90, 80, 255)).save(target)
                    if defect == "empty":
                        Image.new("RGBA", (80, 120)).save(target)
                    elif defect == "size":
                        Image.new("RGBA", (40, 60), (1, 2, 3, 255)).save(target)
                    elif defect == "corrupt":
                        target.write_bytes(b"png")
                    elif defect == "duplicate":
                        data["parts"].append(data["parts"][0])
                    elif defect == "seam":
                        next(part for part in data['parts'] if part['name']=='mouth_closed')['lip_seam']=[[12,20],[11,20],[24,20]]
                    elif defect in ('eye', 'eye_mismatch'):
                        next(part for part in data['parts'] if part['name']=='left_eye_base')['eye_aperture'] = (
                            [[12,27,26,24],[24,20,26,24]] if defect == 'eye' else [[12,20,26,23],[24,20,26,24]])
                    elif defect=='graph_missing':data['scene_graph']=[]
                    elif defect=='graph_cycle':data['scene_graph'][0]['parent']=data['scene_graph'][0]['role']
                    else:
                        data["parts"][-1]["path"] = "../outside.png"
                    manifest.write_text(json.dumps(data), encoding="utf-8")
                    with self.assertRaises((ValueError, OSError)):
                        MODULE.create_rig(manifest, output)
                    self.assertEqual(output.read_bytes(), original)
                    manifest.write_text(json.dumps({"schema_version": 2, "canvas": {"width": 80, "height": 120}, "parts": parts,'scene_graph':graph}), encoding="utf-8")

    def test_rejects_missing_parts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "manifest.json"
            manifest.write_text(
                json.dumps({"schema_version": 2, "canvas": {"width": 1, "height": 1}, "parts": []}),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "必須部位"):
                MODULE.create_rig(manifest, root / "rig.json")


if __name__ == "__main__":
    unittest.main()
