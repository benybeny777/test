import * as THREE from "three";
import { GLTFLoader } from "/ui/shared/vendor/three/addons/loaders/GLTFLoader.js";

const parameters = new URLSearchParams(location.search);
const modelUrl = parameters.get("model");
const textureUrl = parameters.get("texture");
const status = document.querySelector("#status");

if (!modelUrl || !textureUrl) {
  status.dataset.state = "error";
  status.textContent = "model と texture が必要です";
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
camera.position.set(0, 0, 2);
camera.lookAt(0, 0, 0);

const [gltf, texture] = await Promise.all([
  new GLTFLoader().loadAsync(modelUrl),
  new THREE.TextureLoader().loadAsync(textureUrl),
]);
texture.colorSpace = THREE.SRGBColorSpace;
texture.flipY = true;
texture.needsUpdate = true;

let meshCount = 0;
gltf.scene.traverse((object) => {
  if (!object.isMesh) return;
  object.material.dispose();
  object.material = new THREE.MeshBasicMaterial({
    map: texture,
    side: THREE.DoubleSide,
    toneMapped: false,
  });
  meshCount += 1;
});
scene.add(gltf.scene);

renderer.render(scene, camera);
status.dataset.state = "ready";
status.textContent = `Three.js 表示完了 · mesh ${meshCount}`;

addEventListener("resize", () => {
  renderer.setSize(innerWidth, innerHeight);
  renderer.render(scene, camera);
});
