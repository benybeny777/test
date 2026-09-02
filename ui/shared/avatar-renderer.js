import * as THREE from "three";
import { GLTFLoader } from "/shared/vendor/three/addons/loaders/GLTFLoader.js";

const vertexShader = `
  varying vec2 vUv;
  #include <common>
  #include <skinning_pars_vertex>
  void main() {
    vUv = uv;
    vec3 transformed = vec3(position);
    #include <skinbase_vertex>
    #include <skinning_vertex>
    gl_Position = projectionMatrix * modelViewMatrix * vec4(transformed, 1.0);
  }
`;

const fragmentShader = `
  uniform sampler2D fromMap;
  uniform sampler2D toMap;
  uniform float blendAmount;
  varying vec2 vUv;
  void main() {
    gl_FragColor = mix(texture2D(fromMap, vUv), texture2D(toMap, vUv), blendAmount);
  }
`;

export function createAvatarRenderer(canvas) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setClearColor(0x000000, 0);
  renderer.setPixelRatio(devicePixelRatio);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  const scene = new THREE.Scene();
  const camera = new THREE.OrthographicCamera(-0.7, 0.7, 0.7, -0.7, 0.01, 100);
  camera.position.set(0, 0, 3);
  camera.lookAt(0, 0, 0);
  const clock = new THREE.Clock();
  let model;
  let modelUrl;
  let mixer;
  let baseTexture;
  let blinkTexture;
  let textureUrl;
  let blinkTextureUrl;
  let transition;
  let blinkStart = 0;
  let nextBlink = Number.POSITIVE_INFINITY;
  let state = {};
  let disposed = false;
  let animationFrame;
  let revision = 0;
  const textureCache = new Map();

  function resize() {
    renderer.setSize(innerWidth, innerHeight, false);
  }

  function materialFor(texture) {
    return new THREE.ShaderMaterial({
      uniforms: {
        fromMap: { value: texture },
        toMap: { value: texture },
        blendAmount: { value: 1 },
      },
      vertexShader,
      fragmentShader,
      transparent: true,
      side: THREE.DoubleSide,
      toneMapped: false,
    });
  }

  function setMaterialTextures(from, to, amount) {
    model?.traverse((object) => {
      if (!object.isMesh) return;
      object.material.uniforms.fromMap.value = from;
      object.material.uniforms.toMap.value = to;
      object.material.uniforms.blendAmount.value = amount;
    });
  }

  async function loadTexture(url) {
    if (!textureCache.has(url)) {
      const pending = new THREE.TextureLoader().loadAsync(url).then((texture) => {
        texture.colorSpace = THREE.SRGBColorSpace;
        texture.flipY = true;
        texture.needsUpdate = true;
        return texture;
      });
      textureCache.set(url, pending);
      pending.catch(() => textureCache.delete(url));
    }
    return textureCache.get(url);
  }

  async function applyState(nextState) {
    const currentRevision = ++revision;
    state = nextState;
    if (!model || modelUrl !== nextState.modelUrl) {
      const loaded = await new GLTFLoader().loadAsync(nextState.modelUrl);
      if (currentRevision !== revision) {
        disposeModel(loaded.scene);
        return;
      }
      disposeModel(model);
      if (model) scene.remove(model);
      model = loaded.scene;
      modelUrl = nextState.modelUrl;
      mixer = loaded.animations.length ? new THREE.AnimationMixer(model) : undefined;
      if (mixer) mixer.clipAction(loaded.animations[0]).play();
      model.traverse((object) => {
        if (!object.isMesh) return;
        object.material?.dispose();
        object.material = materialFor(baseTexture);
      });
      scene.add(model);
    }
    if (!baseTexture || textureUrl !== nextState.textureUrl) {
      const nextTexture = await loadTexture(nextState.textureUrl);
      if (currentRevision !== revision) return;
      const previous = baseTexture;
      baseTexture = nextTexture;
      textureUrl = nextState.textureUrl;
      transition = previous ? { from: previous, to: nextTexture, start: performance.now() } : undefined;
      setMaterialTextures(previous ?? nextTexture, nextTexture, previous ? 0 : 1);
    }
    if (blinkTextureUrl !== nextState.blinkTextureUrl) {
      blinkTexture = nextState.blinkTextureUrl
        ? await loadTexture(nextState.blinkTextureUrl)
        : undefined;
      if (currentRevision !== revision) return;
      blinkTextureUrl = nextState.blinkTextureUrl;
      nextBlink = performance.now() + randomBlinkDelay(state);
    }
  }

  function render(now) {
    if (disposed) return;
    mixer?.update(clock.getDelta());
    if (transition) {
      const duration = Math.max(1, state.crossfadeMs ?? 160);
      const amount = Math.min(1, (now - transition.start) / duration);
      setMaterialTextures(transition.from, transition.to, amount);
      if (amount >= 1) {
        transition = undefined;
      }
    } else if (blinkTexture && now >= nextBlink) {
      if (!blinkStart) blinkStart = now;
      const duration = Math.max(1, state.blinkDurationMs ?? 140);
      const progress = (now - blinkStart) / duration;
      if (progress >= 1) {
        setMaterialTextures(baseTexture, baseTexture, 1);
        blinkStart = 0;
        nextBlink = now + randomBlinkDelay(state);
      } else {
        const amount = 1 - Math.abs(progress * 2 - 1);
        setMaterialTextures(baseTexture, blinkTexture, amount);
      }
    }
    if (model) {
      const period = Math.max(100, state.idleSwayPeriodMs ?? 4200);
      const sway = Math.sin((now / period) * Math.PI * 2);
      const degrees = state.idleSwayDegrees ?? 0.7;
      model.rotation.z = THREE.MathUtils.degToRad(sway * degrees);
      model.rotation.y = THREE.MathUtils.degToRad((state.yaw ?? 0) + sway * degrees * 0.35);
      model.rotation.x = THREE.MathUtils.degToRad(state.pitch ?? 0);
      model.scale.setScalar(state.scale ?? 1);
      model.position.set(state.offsetX ?? 0, state.offsetY ?? 0, 0);
    }
    renderer.render(scene, camera);
    animationFrame = requestAnimationFrame(render);
  }

  function dispose() {
    disposed = true;
    cancelAnimationFrame(animationFrame);
    removeEventListener("resize", resize);
    mixer?.stopAllAction();
    disposeModel(model);
    for (const pending of textureCache.values()) {
      pending
        .then((texture) => texture.dispose())
        .catch((error) => console.error("テクスチャ解放前の読み込みに失敗しました", error));
    }
    textureCache.clear();
    renderer.dispose();
    renderer.forceContextLoss();
  }

  addEventListener("resize", resize);
  resize();
  animationFrame = requestAnimationFrame(render);
  return { applyState, dispose };
}

export function randomBlinkDelay(state, random = Math.random) {
  const minimum = state.blinkMinMs ?? 2800;
  const maximum = Math.max(minimum, state.blinkMaxMs ?? 6500);
  return minimum + random() * (maximum - minimum);
}

function disposeModel(model) {
  model?.traverse((object) => {
    if (!object.isMesh) return;
    object.geometry?.dispose();
    object.material?.dispose();
  });
}
