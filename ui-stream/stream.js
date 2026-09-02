import { createAvatarRenderer } from "/shared/avatar-renderer.js";

const status = document.querySelector("#status");
const viewer = createAvatarRenderer(document.querySelector("#avatar"));
const protocol = location.protocol === "https:" ? "wss:" : "ws:";
const socket = new WebSocket(`${protocol}//${location.host}/state`);

socket.addEventListener("message", async (event) => {
  try {
    const state = JSON.parse(event.data);
    await viewer.applyState(state);
    document.body.dataset.expression = state.expressionKey;
    document.body.dataset.mouth = state.mouthKey;
    status.dataset.state = "ready";
    status.textContent = `${state.expressionKey}/${state.mouthKey}`;
  } catch (error) {
    status.dataset.state = "error";
    status.textContent = String(error);
  }
});

socket.addEventListener("close", () => {
  status.dataset.state = "error";
  status.textContent = "状態配信が切断されました";
});

addEventListener("beforeunload", () => {
  socket.close();
  viewer.dispose();
});
