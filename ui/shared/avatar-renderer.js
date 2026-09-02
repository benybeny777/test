import * as THREE from "/shared/vendor/three/three.module.min.js";
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
  let modelRoot;
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
  const resizeObserver = new ResizeObserver(resize);
  let revision = 0;
  const poseBones = new Map();
  const baseBoneRotations = new Map();
  const textureCache = new Map();

  function resize() {
    const width = Math.max(1, canvas.clientWidth || innerWidth);
    const height = Math.max(1, canvas.clientHeight || innerHeight);
    const aspect = width / height;
    camera.left = -0.7 * aspect;
    camera.right = 0.7 * aspect;
    camera.top = 0.7;
    camera.bottom = -0.7;
    camera.updateProjectionMatrix();
    renderer.setSize(width, height, false);
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
        texture.flipY = false;
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
      if (modelRoot) scene.remove(modelRoot);
      model = loaded.scene;
      modelUrl = nextState.modelUrl;
      mixer = loaded.animations.length ? new THREE.AnimationMixer(model) : undefined;
      if (mixer) mixer.clipAction(loaded.animations[0]).play();
      poseBones.clear();
      baseBoneRotations.clear();
      model.traverse((object) => {
        if (object.isBone && object.name) {
          poseBones.set(object.name, object);
          baseBoneRotations.set(object.name, object.quaternion.clone());
        }
        if (!object.isMesh) return;
        object.material?.dispose();
        object.material = materialFor(baseTexture);
      });
      const bounds = new THREE.Box3().setFromObject(model);
      const center = bounds.getCenter(new THREE.Vector3());
      const size = bounds.getSize(new THREE.Vector3());
      const focusY = bounds.min.y + size.y * 0.62;
      model.position.set(-center.x, -focusY, -center.z);
      modelRoot = new THREE.Group();
      modelRoot.add(model);
      scene.add(modelRoot);
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
    applyArmPose(state.armPose);
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
    if (modelRoot) {
      const period = Math.max(100, state.idleSwayPeriodMs ?? 4200);
      const sway = Math.sin((now / period) * Math.PI * 2);
      const degrees = state.idleSwayDegrees ?? 0.7;
      modelRoot.rotation.z = THREE.MathUtils.degToRad(sway * degrees);
      modelRoot.rotation.y = THREE.MathUtils.degToRad((state.yaw ?? 0) + sway * degrees * 0.35);
      modelRoot.rotation.x = THREE.MathUtils.degToRad(state.pitch ?? 0);
      modelRoot.scale.setScalar(state.scale ?? 1);
      modelRoot.position.set(state.offsetX ?? 0, state.offsetY ?? 0, 0);
    }
    renderer.render(scene, camera);
    animationFrame = requestAnimationFrame(render);
  }

  function applyArmPose(pose) {
    if (!pose) return;
    for (const [name, degrees] of Object.entries(pose)) {
      const bone = poseBones.get(name);
      const base = baseBoneRotations.get(name);
      if (!bone || !base || !Array.isArray(degrees) || degrees.length !== 3) continue;
      const isZero = degrees.every((value) => Math.abs(value) < 0.0001);
      if (isZero && name === "head") continue;
      if (isZero) {
        bone.quaternion.copy(base);
        continue;
      }
      const offset = new THREE.Quaternion().setFromEuler(
        new THREE.Euler(...degrees.map(THREE.MathUtils.degToRad), "XYZ"),
      );
      bone.quaternion.copy(base).multiply(offset);
    }
  }

  function dispose() {
    disposed = true;
    cancelAnimationFrame(animationFrame);
    removeEventListener("resize", resize);
    resizeObserver.disconnect();
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
  resizeObserver.observe(canvas);
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
