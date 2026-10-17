/**
 * Scene settings and Juices logic.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
 */

import type { CropCalculationResult, Juices, Scene } from '../types/index.js';

/**
 * Creates a default Juices object.
 */
export function createDefaultJuices(): Juices {
  return {
    bgImage: undefined,
    bgWidth: undefined,
    bgHeight: undefined,
    bgAspectRatio: undefined,
    bgm1: undefined,
    bgm1Volume: 0.8,
    bgm2: undefined,
    bgm2Volume: 0.8,
    bgm3: undefined,
    bgm3Volume: 0.8,
  };
}

/**
 * Inherits Juices from a parent / source scene.
 *
 * ARCHITECTURE.md: "トグル”Juicesを継承”" / "存在しないシーン名が割り振られていた場合、Juicesを継承して新規作成する。"
 */
export function inheritJuices(source: Juices): Juices {
  return {
    bgImage: source.bgImage,
    bgWidth: source.bgWidth,
    bgHeight: source.bgHeight,
    bgAspectRatio: source.bgAspectRatio,
    bgm1: source.bgm1,
    bgm1Volume: source.bgm1Volume,
    bgm2: source.bgm2,
    bgm2Volume: source.bgm2Volume,
    bgm3: source.bgm3,
    bgm3Volume: source.bgm3Volume,
  };
}

/**
 * Calculates dimensions and edge crop for a background image.
 *
 * ARCHITECTURE.md:
 * "画像は一方を指定すればもう一方を自動で計算します。もう一方も入力した場合、端を切り落として揃えます。"
 */
export function calculateDimensionsAndCrop(params: {
  naturalWidth: number;
  naturalHeight: number;
  width?: number;
  height?: number;
}): CropCalculationResult {
  const { naturalWidth, naturalHeight, width, height } = params;

  if (naturalWidth <= 0 || naturalHeight <= 0) {
    return {
      width: width ?? 0,
      height: height ?? 0,
      aspectRatio: 1,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: 0,
      cropHeight: 0,
    };
  }

  const naturalAspect = naturalWidth / naturalHeight;

  // Case 1: Only width is provided -> auto calculate height from natural aspect ratio
  if (width !== undefined && width > 0 && (height === undefined || height <= 0)) {
    const computedHeight = Math.round(width / naturalAspect);
    return {
      width,
      height: computedHeight,
      aspectRatio: naturalAspect,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: naturalWidth,
      cropHeight: naturalHeight,
    };
  }

  // Case 2: Only height is provided -> auto calculate width from natural aspect ratio
  if (height !== undefined && height > 0 && (width === undefined || width <= 0)) {
    const computedWidth = Math.round(height * naturalAspect);
    return {
      width: computedWidth,
      height,
      aspectRatio: naturalAspect,
      cropped: false,
      cropX: 0,
      cropY: 0,
      cropWidth: naturalWidth,
      cropHeight: naturalHeight,
    };
  }

  // Case 3: Both width and height are provided -> crop edges to fit requested aspect ratio
  if (width !== undefined && width > 0 && height !== undefined && height > 0) {
    const targetAspect = width / height;
    let cropWidth = naturalWidth;
    let cropHeight = naturalHeight;
    let cropX = 0;
    let cropY = 0;
    let isCropped = false;

    if (Math.abs(targetAspect - naturalAspect) > 1e-4) {
      isCropped = true;
      if (targetAspect > naturalAspect) {
        // Target is wider: crop top and bottom edges
        cropHeight = Math.round(naturalWidth / targetAspect);
        cropY = Math.round((naturalHeight - cropHeight) / 2);
      } else {
        // Target is taller: crop left and right edges
        cropWidth = Math.round(naturalHeight * targetAspect);
        cropX = Math.round((naturalWidth - cropWidth) / 2);
      }
    }

    return {
      width,
      height,
      aspectRatio: targetAspect,
      cropped: isCropped,
      cropX,
      cropY,
      cropWidth,
      cropHeight,
    };
  }

  // Case 4: Neither is provided -> use natural dimensions
  return {
    width: naturalWidth,
    height: naturalHeight,
    aspectRatio: naturalAspect,
    cropped: false,
    cropX: 0,
    cropY: 0,
    cropWidth: naturalWidth,
    cropHeight: naturalHeight,
  };
}

/**
 * Moves a member to target scene. If the target scene name does not exist,
 * automatically creates the scene inheriting Juices from the current scene.
 */
export function moveMemberToScene(
  characterId: string,
  targetSceneName: string,
  currentScene: Scene,
  existingScenes: Scene[],
): {
  updatedScenes: Scene[];
  targetScene: Scene;
  isNewSceneCreated: boolean;
} {
  const trimmedName = targetSceneName.trim();
  const scenes = existingScenes.map((s) => ({
    ...s,
    members: s.members.filter((id) => id !== characterId),
  }));

  let targetScene = scenes.find((s) => s.name === trimmedName);
  let isNewSceneCreated = false;

  if (!targetScene) {
    // Scene does not exist: create new scene inheriting Juices
    isNewSceneCreated = true;
    const newId = `scene-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    targetScene = {
      id: newId,
      name: trimmedName,
      juices: inheritJuices(currentScene.juices),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    scenes.push(targetScene);
  }

  // Add character to target scene members
  if (!targetScene.members.includes(characterId)) {
    targetScene.members.push(characterId);
  }

  return {
    updatedScenes: scenes,
    targetScene,
    isNewSceneCreated,
  };
}
