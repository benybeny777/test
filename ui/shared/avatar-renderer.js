import * as THREE from "three";
import { GLTFLoader } from "/shared/vendor/three/addons/loaders/GLTFLoader.js";

export function createAvatarRenderer(canvas) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setClearColor(0x000000, 0);
  renderer.setPixelRatio(devicePixelRatio);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  const scene = new THREE.Scene();
  const camera = new THREE.OrthographicCamera(-0.7, 0.7, 0.7, -0.7, 0.01, 100);
  camera.position.set(0, 0, 3);
  camera.lookAt(0, 0, 0);
  let model;
  let modelUrl;
  let texture;

  function resize() {
    renderer.setSize(innerWidth, innerHeight, false);
    renderer.render(scene, camera);
  }

  async function applyState(state) {
    if (!model || modelUrl !== state.modelUrl) {
      disposeModel(model);
      if (model) scene.remove(model);
      const loaded = await new GLTFLoader().loadAsync(state.modelUrl);
      model = loaded.scene;
      modelUrl = state.modelUrl;
      scene.add(model);
    }
    const nextTexture = await new THREE.TextureLoader().loadAsync(state.textureUrl);
    nextTexture.colorSpace = THREE.SRGBColorSpace;
    nextTexture.flipY = true;
    nextTexture.needsUpdate = true;
    texture?.dispose();
    texture = nextTexture;
    model.traverse((object) => {
      if (!object.isMesh) return;
      object.material?.dispose();
      object.material = new THREE.MeshBasicMaterial({
        map: texture,
        side: THREE.DoubleSide,
        toneMapped: false,
        transparent: true,
      });
    });
    model.rotation.y = THREE.MathUtils.degToRad(state.yaw);
    model.rotation.x = THREE.MathUtils.degToRad(state.pitch);
    model.scale.setScalar(state.scale);
    model.position.set(state.offsetX, state.offsetY, 0);
    renderer.render(scene, camera);
  }

  function dispose() {
    removeEventListener("resize", resize);
    disposeModel(model);
    texture?.dispose();
    renderer.dispose();
    renderer.forceContextLoss();
  }

  addEventListener("resize", resize);
  resize();
  return { applyState, dispose };
}

function disposeModel(model) {
  model?.traverse((object) => {
    if (!object.isMesh) return;
    object.geometry?.dispose();
    object.material?.dispose();
  });
}
