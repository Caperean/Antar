/* tslint:disable */
/* eslint-disable */

export class Antar {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Clear the canvas. `color` is a CSS hex string, e.g. `"#1a1a2e"`.
     */
    clear(color: string): void;
    createTextureAtlas(keys: string[], widths: Uint32Array, heights: Uint32Array, data_flat: Uint8Array, atlas_size: number): any;
    drawImage(js_params: any): void;
    fillRect(js_params: any): void;
    /**
     * Initialise Antar on a canvas element.
     *
     * ```js
     * const ctx = await Antar.init(document.getElementById("canvas"));
     * ```
     */
    static init(canvas: HTMLCanvasElement): Promise<Antar>;
    /**
     * Upload RGBA8 pixel data as a GPU texture. Returns a texture handle (u32).
     */
    loadTexture(rgba_data: Uint8Array, width: number, height: number): number;
    /**
     * Flush all queued draw commands to the GPU and present the frame.
     */
    present(): void;
    restore(): void;
    rotate(angle: number): void;
    save(): void;
    scale(sx: number, sy: number): void;
    setFillStyle(css: string): void;
    setGlobalAlpha(alpha: number): void;
    translate(x: number, y: number): void;
    unloadTexture(texture_id: number): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_antar_free: (a: number, b: number) => void;
    readonly antar_clear: (a: number, b: number, c: number) => void;
    readonly antar_createTextureAtlas: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number];
    readonly antar_drawImage: (a: number, b: any) => [number, number];
    readonly antar_fillRect: (a: number, b: any) => [number, number];
    readonly antar_init: (a: any) => any;
    readonly antar_loadTexture: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly antar_present: (a: number) => [number, number];
    readonly antar_restore: (a: number) => void;
    readonly antar_rotate: (a: number, b: number) => [number, number];
    readonly antar_save: (a: number) => void;
    readonly antar_scale: (a: number, b: number, c: number) => [number, number];
    readonly antar_setFillStyle: (a: number, b: number, c: number) => void;
    readonly antar_setGlobalAlpha: (a: number, b: number) => [number, number];
    readonly antar_translate: (a: number, b: number, c: number) => [number, number];
    readonly antar_unloadTexture: (a: number, b: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h84d0a4314cfcc573: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h84d0a4314cfcc573_1: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h84d0a4314cfcc573_2: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h2f1bdec577821531: (a: number, b: number, c: any, d: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
