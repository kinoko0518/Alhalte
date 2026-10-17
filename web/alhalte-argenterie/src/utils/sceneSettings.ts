/**
 * Scene settings and Juices logic.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーンを新規作成 / メンバーを移動`
 */

import type { Juices, Scene } from '../types/index.js';

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

export {
  calculateBackgroundDimensions,
  calculateDimensionsAndCrop,
  type BackgroundDimensionParams,
  type BackgroundDimensionResult,
} from './backgroundDimension.js';


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
