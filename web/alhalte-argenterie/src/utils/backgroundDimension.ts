/**
 * Background dimension and crop calculation for Scene Juices.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
 * - GitHub Issue #51: シーン設定の縦幅と横幅のユーザー入力に対しておせっかいを焼くべきではない
 */

export interface BackgroundDimensionParams {
  origW?: number;
  origH?: number;
  naturalWidth?: number;
  naturalHeight?: number;
  width?: number | null;
  height?: number | null;
}

export interface BackgroundDimensionResult {
  width: number;
  height: number;
  aspectRatio: number;
  scale: number;
  cropped: boolean;
  cropX: number;
  cropY: number;
  cropWidth: number;
  cropHeight: number;
}

/**
 * Calculates background dimensions and crop parameters.
 *
 * Rules (Issue #51):
 * - Unit is px.
 * - If both width and height are empty/unspecified, use original image size (origW, origH).
 * - If one axis A is specified (valA) and axis B is empty:
 *   magnification N = valA / origA, axis B size = origB * N (maintaining aspect ratio).
 * - If both axes are specified:
 *   scale by the axis C requiring larger magnification (Math.max(width / origW, height / origH)),
 *   and crop the overflowing edges (equivalent to object-fit: cover).
 */
export function calculateBackgroundDimensions(
  params: BackgroundDimensionParams,
): BackgroundDimensionResult {
  const origW = params.origW ?? params.naturalWidth ?? 0;
  const origH = params.origH ?? params.naturalHeight ?? 0;
  const width = params.width;
  const height = params.height;

  const hasWidth = typeof width === 'number' && !Number.isNaN(width) && width > 0;
  const hasHeight = typeof height === 'number' && !Number.isNaN(height) && height > 0;

  // Edge case: invalid original dimensions
  if (origW <= 0 || origH <= 0) {
    const finalW = hasWidth ? (width as number) : 0;
    const finalH = hasHeight ? (height as number) : 0;
    return {
      width: finalW,
      height: finalH,
      aspectRatio: finalH > 0 ? finalW / finalH : 1,
      scale: 1,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: 0,
      cropHeight: 0,
    };
  }

  const naturalAspect = origW / origH;

  // Case 1: Neither width nor height specified -> use original image dimensions
  if (!hasWidth && !hasHeight) {
    return {
      width: origW,
      height: origH,
      aspectRatio: naturalAspect,
      scale: 1,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: origW,
      cropHeight: origH,
    };
  }

  // Case 2: Only width specified -> A is width, B is height. N = width / origW, height = origH * N
  if (hasWidth && !hasHeight) {
    const targetW = width as number;
    const N = targetW / origW;
    const computedH = Math.round(origH * N);
    return {
      width: targetW,
      height: computedH,
      aspectRatio: naturalAspect,
      scale: N,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: origW,
      cropHeight: origH,
    };
  }

  // Case 3: Only height specified -> A is height, B is width. N = height / origH, width = origW * N
  if (!hasWidth && hasHeight) {
    const targetH = height as number;
    const N = targetH / origH;
    const computedW = Math.round(origW * N);
    return {
      width: computedW,
      height: targetH,
      aspectRatio: naturalAspect,
      scale: N,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: origW,
      cropHeight: origH,
    };
  }

  // Case 4: Both width and height specified -> scale by larger magnification axis C and crop (object-fit: cover)
  const targetW = width as number;
  const targetH = height as number;
  const targetAspect = targetW / targetH;

  const scaleX = targetW / origW;
  const scaleY = targetH / origH;
  const scale = Math.max(scaleX, scaleY);

  let cropWidth = origW;
  let cropHeight = origH;
  let cropX = 0;
  let cropY = 0;
  let cropped = false;

  if (Math.abs(targetAspect - naturalAspect) > 1e-4) {
    cropped = true;
    if (scaleX > scaleY) {
      // C is X axis: scale by scaleX. Scaled height overflows targetH, crop top/bottom.
      // Visible height in original image coordinates:
      cropHeight = Math.round(targetH / scale);
      cropY = Math.round((origH - cropHeight) / 2);
    } else {
      // C is Y axis: scale by scaleY. Scaled width overflows targetW, crop left/right.
      // Visible width in original image coordinates:
      cropWidth = Math.round(targetW / scale);
      cropX = Math.round((origW - cropWidth) / 2);
    }
  }

  return {
    width: targetW,
    height: targetH,
    aspectRatio: targetAspect,
    scale,
    cropped,
    cropX,
    cropY,
    cropWidth,
    cropHeight,
  };
}

/**
 * Backward compatibility alias for calculateBackgroundDimensions.
 */
export const calculateDimensionsAndCrop = calculateBackgroundDimensions;
