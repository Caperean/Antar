/**
 * antar-shim.js
 *
 * Thin JS helper around the Antar WASM module (object-based API).
 *
 * Usage:
 *   import { initAntar } from "./antar-shim.js";
 *   const ctx = await initAntar(canvas);
 *
 *   ctx.clear("#1a1a2e");
 *   ctx.setFillStyle("#fff");
 *   ctx.setGlobalAlpha(0.5);
 *   ctx.translate(10, 20);
 *   ctx.drawImage({
 *     texture: texId,
 *     src: { x: 0, y: 0, w: 32, h: 32 },
 *     dst: { x: 100, y: 200, w: 64, h: 64 },
 *   });
 *   ctx.fillRect({ rect: { x: 0, y: 0, w: 50, h: 50 }, color: { r: 1, g: 0, b: 0, a: 1 } });
 *   ctx.present();
 */

import init, { Antar } from "../pkg/antar.js";

export async function initAntar(canvas) {
  await init();
  return await Antar.init(canvas);
}

export { Antar };
