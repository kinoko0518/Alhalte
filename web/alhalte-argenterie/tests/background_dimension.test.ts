import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  calculateBackgroundDimensions,
  calculateDimensionsAndCrop,
} from '../src/utils/backgroundDimension.js';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import FieldPanel from '../src/components/Field/FieldPanel.svelte';
import type { Scene } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Background dimension & aspect ratio calculation (Issue #51)', () => {
  // spec: alhalte-argenterie/scene-settings どちらも空のときは画像のオリジナルサイズ (origW, origH) をそのまま使用する
  it('uses original image dimensions (origW, origH) as is when both width and height are empty', () => {
    // undefined for both
    const resUndefined = calculateBackgroundDimensions({
      origW: 1920,
      origH: 1080,
    });
    expect(resUndefined.width).toBe(1920);
    expect(resUndefined.height).toBe(1080);
    expect(resUndefined.scale).toBe(1);
    expect(resUndefined.cropped).toBe(false);
    expect(resUndefined.cropWidth).toBe(1920);
    expect(resUndefined.cropHeight).toBe(1080);
    expect(resUndefined.cropX).toBe(0);
    expect(resUndefined.cropY).toBe(0);

    // null for both
    const resNull = calculateBackgroundDimensions({
      origW: 1200,
      origH: 800,
      width: null,
      height: null,
    });
    expect(resNull.width).toBe(1200);
    expect(resNull.height).toBe(800);
    expect(resNull.scale).toBe(1);
    expect(resNull.cropped).toBe(false);

    // 0 or negative numbers are treated as empty
    const resZero = calculateBackgroundDimensions({
      origW: 800,
      origH: 600,
      width: 0,
      height: -100,
    });
    expect(resZero.width).toBe(800);
    expect(resZero.height).toBe(600);
    expect(resZero.cropped).toBe(false);
  });

  // spec: alhalte-argenterie/scene-settings 一方が空のときは A軸の拡大率 N = valA / origA で B軸を origB * N とする (横幅指定)
  it('calculates empty height from width using scale factor N = width / origW (maintaining aspect ratio)', () => {
    // 1920x1080 original (16:9)
    const origW = 1920;
    const origH = 1080;

    // Downscale: width = 960 (N = 960 / 1920 = 0.5) -> height = 1080 * 0.5 = 540
    const resDown = calculateBackgroundDimensions({
      origW,
      origH,
      width: 960,
    });
    expect(resDown.width).toBe(960);
    expect(resDown.height).toBe(540);
    expect(resDown.scale).toBe(0.5);
    expect(resDown.cropped).toBe(false);
    expect(resDown.cropWidth).toBe(1920);
    expect(resDown.cropHeight).toBe(1080);

    // Upscale: width = 3840 (N = 3840 / 1920 = 2) -> height = 1080 * 2 = 2160
    const resUp = calculateBackgroundDimensions({
      origW,
      origH,
      width: 3840,
    });
    expect(resUp.width).toBe(3840);
    expect(resUp.height).toBe(2160);
    expect(resUp.scale).toBe(2);
    expect(resUp.cropped).toBe(false);

    // Square image: 500x500, width = 750 (N = 1.5) -> height = 750
    const resSquare = calculateBackgroundDimensions({
      origW: 500,
      origH: 500,
      width: 750,
      height: undefined,
    });
    expect(resSquare.width).toBe(750);
    expect(resSquare.height).toBe(750);
    expect(resSquare.scale).toBe(1.5);
  });

  // spec: alhalte-argenterie/scene-settings 一方が空のときは A軸の拡大率 N = valA / origA で B軸を origB * N とする (縦幅指定)
  it('calculates empty width from height using scale factor N = height / origH (maintaining aspect ratio)', () => {
    // 1920x1080 original (16:9)
    const origW = 1920;
    const origH = 1080;

    // Downscale: height = 720 (N = 720 / 1080 = 2/3) -> width = 1920 * (2/3) = 1280
    const resDown = calculateBackgroundDimensions({
      origW,
      origH,
      height: 720,
    });
    expect(resDown.width).toBe(1280);
    expect(resDown.height).toBe(720);
    expect(resDown.scale).toBeCloseTo(2 / 3);
    expect(resDown.cropped).toBe(false);
    expect(resDown.cropWidth).toBe(1920);
    expect(resDown.cropHeight).toBe(1080);

    // Upscale: height = 2160 (N = 2) -> width = 1920 * 2 = 3840
    const resUp = calculateBackgroundDimensions({
      origW,
      origH,
      width: undefined,
      height: 2160,
    });
    expect(resUp.width).toBe(3840);
    expect(resUp.height).toBe(2160);
    expect(resUp.scale).toBe(2);
    expect(resUp.cropped).toBe(false);
  });

  // spec: alhalte-argenterie/scene-settings 両方指定時は XY軸のうちより大きい軸 C の大きさで拡大し Crop (object-fit: cover 相当)
  it('scales by larger axis scale C and crops overflowing edges (object-fit: cover) when both are specified', () => {
    // Case A: Target aspect ratio is taller than original aspect ratio
    // orig: 1920x1080 (16:9 = 1.777...), target: 1000x1000 (1:1 = 1)
    // scaleX = 1000 / 1920 = 0.5208..., scaleY = 1000 / 1080 = 0.9259...
    // Axis C is Y: scale = scaleY = 1000 / 1080.
    // Width overflows and is cropped horizontally (cropX > 0, cropWidth = 1080).
    const resTaller = calculateBackgroundDimensions({
      origW: 1920,
      origH: 1080,
      width: 1000,
      height: 1000,
    });
    expect(resTaller.width).toBe(1000);
    expect(resTaller.height).toBe(1000);
    expect(resTaller.scale).toBeCloseTo(1000 / 1080);
    expect(resTaller.cropped).toBe(true);
    expect(resTaller.cropWidth).toBe(1080);
    expect(resTaller.cropX).toBe(420); // (1920 - 1080) / 2
    expect(resTaller.cropHeight).toBe(1080);
    expect(resTaller.cropY).toBe(0);

    // Case B: Target aspect ratio is wider than original aspect ratio
    // orig: 1000x1000 (1:1), target: 1600x900 (16:9)
    // scaleX = 1600 / 1000 = 1.6, scaleY = 900 / 1000 = 0.9
    // Axis C is X: scale = scaleX = 1.6.
    // Height overflows and is cropped vertically (cropY > 0, cropHeight = Math.round(900 / 1.6) = 563).
    const resWider = calculateBackgroundDimensions({
      origW: 1000,
      origH: 1000,
      width: 1600,
      height: 900,
    });
    expect(resWider.width).toBe(1600);
    expect(resWider.height).toBe(900);
    expect(resWider.scale).toBe(1.6);
    expect(resWider.cropped).toBe(true);
    expect(resWider.cropWidth).toBe(1000);
    expect(resWider.cropX).toBe(0);
    expect(resWider.cropHeight).toBe(563);
    expect(resWider.cropY).toBe(219); // (1000 - 563) / 2

    // Case C: Target aspect ratio matches original aspect ratio exactly -> no crop
    const resMatch = calculateBackgroundDimensions({
      origW: 1920,
      origH: 1080,
      width: 960,
      height: 540,
    });
    expect(resMatch.width).toBe(960);
    expect(resMatch.height).toBe(540);
    expect(resMatch.scale).toBe(0.5);
    expect(resMatch.cropped).toBe(false);
    expect(resMatch.cropWidth).toBe(1920);
    expect(resMatch.cropHeight).toBe(1080);
  });

  // spec: alhalte-argenterie/scene-settings calculateDimensionsAndCrop との下位互換性
  it('maintains compatibility with calculateDimensionsAndCrop using naturalWidth/naturalHeight', () => {
    const res = calculateDimensionsAndCrop({
      naturalWidth: 1920,
      naturalHeight: 1080,
      width: 960,
    });
    expect(res.width).toBe(960);
    expect(res.height).toBe(540);
    expect(res.cropped).toBe(false);
  });

  // spec: alhalte-argenterie/scene-settings ユーザー入力欄がおせっかいを焼かず愚直に入力欄として振る舞う (Issue #51)
  it('does NOT rewrite or auto-complete the other dimension input when entering width or height in SceneSettingsTab', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-no-meddle',
      name: '入力欄テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let updatedScene: Scene | null = null;
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: testScene,
        allScenes: [testScene],
        characters: [],
        onUpdateScene: (sc) => {
          updatedScene = sc;
        },
      },
    });

    const widthInput = target.querySelector<HTMLInputElement>('[data-testid="bg-width-input"]');
    const heightInput = target.querySelector<HTMLInputElement>('[data-testid="bg-height-input"]');
    expect(widthInput).toBeTruthy();
    expect(heightInput).toBeTruthy();

    // 1. Initially both inputs are empty
    expect(widthInput?.value).toBe('');
    expect(heightInput?.value).toBe('');

    // 2. User types 1280 into width input
    if (widthInput) {
      widthInput.value = '1280';
      widthInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    // Width is updated in scene state
    expect(updatedScene?.juices.bgWidth).toBe(1280);
    // Height input MUST NOT be auto-filled or overwritten! It remains empty.
    expect(heightInput?.value).toBe('');
    expect(updatedScene?.juices.bgHeight).toBeUndefined();

    // 3. User types 720 into height input
    if (heightInput) {
      heightInput.value = '720';
      heightInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    // Both are now present as user explicitly typed them
    expect(updatedScene?.juices.bgWidth).toBe(1280);
    expect(updatedScene?.juices.bgHeight).toBe(720);

    // 4. User clears width input
    if (widthInput) {
      widthInput.value = '';
      widthInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    // Width becomes undefined, height remains untouched at 720
    expect(updatedScene?.juices.bgWidth).toBeUndefined();
    expect(updatedScene?.juices.bgHeight).toBe(720);
    expect(heightInput?.value).toBe('720');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/field 盤面描画における背景画像サイズ計算 (Issue #51)
  it('renders FieldPanel / BoardCanvas background with correctly calculated dimensions', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const bgImage = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080"/>';

    // 1. Both empty -> uses origW (1920) and origH (1080)
    const sceneBothEmpty: Scene = {
      id: 'sc-1',
      name: 'Default Size',
      juices: { ...createDefaultJuices(), bgImage },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const inst1 = mount(FieldPanel, {
      target,
      props: { scene: sceneBothEmpty, origW: 1920, origH: 1080 },
    });
    const bg1 = target.querySelector<HTMLElement>('[data-testid="field-background"]');
    expect(bg1?.style.width).toBe('1920px');
    expect(bg1?.style.height).toBe('1080px');
    unmount(inst1);

    // 2. Width only (960) -> height = 540
    const sceneWidthOnly: Scene = {
      id: 'sc-2',
      name: 'Width Only',
      juices: { ...createDefaultJuices(), bgImage, bgWidth: 960 },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const inst2 = mount(FieldPanel, {
      target,
      props: { scene: sceneWidthOnly, origW: 1920, origH: 1080 },
    });
    const bg2 = target.querySelector<HTMLElement>('[data-testid="field-background"]');
    expect(bg2?.style.width).toBe('960px');
    expect(bg2?.style.height).toBe('540px');
    unmount(inst2);

    // 3. Height only (720) -> width = 1280
    const sceneHeightOnly: Scene = {
      id: 'sc-3',
      name: 'Height Only',
      juices: { ...createDefaultJuices(), bgImage, bgHeight: 720 },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const inst3 = mount(FieldPanel, {
      target,
      props: { scene: sceneHeightOnly, origW: 1920, origH: 1080 },
    });
    const bg3 = target.querySelector<HTMLElement>('[data-testid="field-background"]');
    expect(bg3?.style.width).toBe('1280px');
    expect(bg3?.style.height).toBe('720px');
    unmount(inst3);

    // 4. Both specified (1000x1000) -> width = 1000px, height = 1000px, background-size: cover
    const sceneBothSpecified: Scene = {
      id: 'sc-4',
      name: 'Both Specified',
      juices: { ...createDefaultJuices(), bgImage, bgWidth: 1000, bgHeight: 1000 },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const inst4 = mount(FieldPanel, {
      target,
      props: { scene: sceneBothSpecified, origW: 1920, origH: 1080 },
    });
    const bg4 = target.querySelector<HTMLElement>('[data-testid="field-background"]');
    expect(bg4?.style.width).toBe('1000px');
    expect(bg4?.style.height).toBe('1000px');
    expect(bg4?.style.backgroundSize).toBe('cover');
    unmount(inst4);

    target.remove();
  });
});
