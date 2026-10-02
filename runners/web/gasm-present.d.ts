/** Filter names, as `gasm-run --filter` takes them. */
export declare const FILTERS: readonly ['nearest', 'sharp', 'xbr', 'fsr', 'crt'];
export type Filter = (typeof FILTERS)[number];

export interface PresentOptions {
  /** default 'sharp' (exactly 'nearest' at whole factors, even pixels otherwise) */
  filter?: Filter;
  /** whole multiples only (black border around), when the frame fits */
  integerScale?: boolean;
  /** the frame's display aspect [num, den] (video_set_aspect); null = square pixels */
  aspect?: [number, number] | null;
}

/** The filter that actually runs at a scale factor (shrinking is linear; xBR needs 1.5x). */
export declare function effectiveFilter(filter: Filter, scale: number): Filter;

/**
 * Draws video_present frames onto a canvas with WebGL 2: letterboxed, with an
 * upscaling filter. The canvas is resized to the output size given to draw().
 */
export declare class GlPresenter {
  /** null if the canvas can't get a WebGL 2 context (or already has another context). */
  static create(canvas: HTMLCanvasElement | OffscreenCanvas): GlPresenter | null;
  /** Draw a width×height RGBA frame into an output of `size` device pixels. */
  draw(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number, size: [number, number], options?: PresentOptions): void;
  /** The canvas as RGBA rows, top to bottom; call right after draw(). */
  read(): Uint8Array;
}
