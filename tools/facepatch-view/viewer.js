import * as THREE from "three";
import { GLTFLoader } from "/ui/shared/vendor/three/addons/loaders/GLTFLoader.js";

const parameters = new URLSearchParams(location.search);
const modelUrl = parameters.get("model");
const textureUrl = parameters.get("texture");
const status = document.querySelector("#status");

if (!modelUrl) {
  status.dataset.state = "error";
  status.textContent = "model が必要です";
  throw new Error(status.textContent);
}

const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
renderer.setPixelRatio(devicePixelRatio);
renderer.setSize(innerWidth, innerHeight);
renderer.outputColorSpace = THREE.SRGBColorSpace;
document.body.append(renderer.domElement);

const scene = new THREE.Scene();
scene.background = new THREE.Color(0x171c27);
const camera = new THREE.OrthographicCamera(-0.7, 0.7, 0.7, -0.7, 0.01, 10);
const cameraAxis = parameters.get("axis") ?? "z";
const cameraSide = parameters.get("side") === "opposite" ? -2 : 2;
if (cameraAxis === "x") {
  camera.position.set(cameraSide, 0, 0);
  camera.up.set(0, 0, 1);
} else {
  camera.position.set(0, 0, cameraSide);
}
camera.lookAt(0, 0, 0);

const gltf = await new GLTFLoader().loadAsync(modelUrl);
const texture = textureUrl
  ? await new THREE.TextureLoader().loadAsync(textureUrl)
  : null;
if (texture) {
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.flipY = true;
  texture.needsUpdate = true;
}

let meshCount = 0;
gltf.scene.traverse((object) => {
  if (!object.isMesh) return;
  if (texture) {
    object.material.dispose();
    object.material = new THREE.MeshBasicMaterial({
      map: texture,
      side: THREE.DoubleSide,
      toneMapped: false,
    });
  } else {
    const map = object.material.map;
    object.material.dispose();
    object.material = new THREE.MeshBasicMaterial({
      map,
      vertexColors: !map,
      side: THREE.DoubleSide,
      toneMapped: false,
    });
  }
  meshCount += 1;
});
gltf.scene.rotation.set(
  THREE.MathUtils.degToRad(Number(parameters.get("rx") ?? 0)),
  THREE.MathUtils.degToRad(Number(parameters.get("ry") ?? 0)),
  THREE.MathUtils.degToRad(Number(parameters.get("rz") ?? 0)),
);
const bounds = new THREE.Box3().setFromObject(gltf.scene);
const center = bounds.getCenter(new THREE.Vector3());
const size = bounds.getSize(new THREE.Vector3());
gltf.scene.position.sub(center);
const half =
  (cameraAxis === "x" ? Math.max(size.y, size.z) : Math.max(size.x, size.y)) *
  0.58;
camera.left = -half;
camera.right = half;
camera.top = half;
camera.bottom = -half;
camera.updateProjectionMatrix();
scene.add(gltf.scene);
scene.add(new THREE.HemisphereLight(0xffffff, 0x303040, 2.2));

const mixer = gltf.animations.length > 0 ? new THREE.AnimationMixer(gltf.scene) : null;
if (mixer) {
  mixer.clipAction(gltf.animations[0]).play();
  const fixedTime = Number(parameters.get("time"));
  if (parameters.has("time") && Number.isFinite(fixedTime)) {
    mixer.setTime(fixedTime);
  }
}
const clock = new THREE.Clock();
function render() {
  if (mixer && !parameters.has("time")) {
    mixer.update(clock.getDelta());
  }
  renderer.render(scene, camera);
  requestAnimationFrame(render);
}
render();
status.dataset.state = "ready";
status.textContent = `Three.js 表示完了 · mesh ${meshCount} · animation ${gltf.animations.length}`;

addEventListener("resize", () => {
  renderer.setSize(innerWidth, innerHeight);
});
