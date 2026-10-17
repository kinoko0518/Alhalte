import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import { buildSceneGraph, type SceneMovementEvent } from '../src/utils/sceneGraph.js';
import SceneGraphTab from '../src/components/Control/tabs/SceneGraphTab.svelte';

describe('Scene Graph Algorithm & View', () => {
  const sceneNames = new Map<string, string>([
    ['scene-a', '街頭'],
    ['scene-b', '地下迷宮'],
    ['scene-c', '神殿'],
    ['scene-d', '独立した塔'], // Uncorrelated
  ]);

  const accessibleScenes = new Set<string>(['scene-a', 'scene-b', 'scene-c', 'scene-d']);

  // spec: alhalte-argenterie/scene-graph 時系列早い順（上→下）
  it('orders nodes chronologically with earlier at top and later at bottom', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '勇者', toSceneId: 'scene-b', timestamp: 200 },
      { characterId: 'c1', characterName: '勇者', toSceneId: 'scene-a', timestamp: 100 },
      { characterId: 'c1', characterName: '勇者', toSceneId: 'scene-c', timestamp: 300 },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);
    expect(result.nodes).toHaveLength(3);
    expect(result.nodes[0].sceneId).toBe('scene-a');
    expect(result.nodes[1].sceneId).toBe('scene-b');
    expect(result.nodes[2].sceneId).toBe('scene-c');
    expect(result.nodes[0].timestamp).toBeLessThan(result.nodes[1].timestamp);
    expect(result.nodes[1].timestamp).toBeLessThan(result.nodes[2].timestamp);
  });

  // spec: alhalte-argenterie/scene-graph 無相関シーン別ブランチ
  it('draws uncorrelated scenes on separate branch lanes', () => {
    const movements: SceneMovementEvent[] = [
      // Component 1: Scene A <-> Scene B
      { characterId: 'c1', characterName: '戦士', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '戦士',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 120,
      },
      // Component 2: Scene D (塔) - uncorrelated with A/B
      { characterId: 'c2', characterName: '魔法使い', toSceneId: 'scene-d', timestamp: 110 },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);
    expect(result.branchCount).toBeGreaterThanOrEqual(2);

    const nodeA = result.nodes.find((n) => n.sceneId === 'scene-a');
    const nodeB = result.nodes.find((n) => n.sceneId === 'scene-b');
    const nodeD = result.nodes.find((n) => n.sceneId === 'scene-d');

    // A and B should share the same branch lane
    expect(nodeA?.branchIndex).toBe(nodeB?.branchIndex);
    // D should be on a different branch lane
    expect(nodeD?.branchIndex).not.toBe(nodeA?.branchIndex);
  });

  // spec: alhalte-argenterie/scene-graph 未知シーン「？」点線
  it('draws dotted edge to a single "?" node per inaccessible unknown scene', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '潜入者', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '潜入者',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-secret-1', // Inaccessible
        timestamp: 150,
      },
      {
        characterId: 'c2',
        characterName: '別の潜入者',
        toSceneId: 'scene-a',
        timestamp: 110,
      },
      {
        characterId: 'c2',
        characterName: '別の潜入者',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-secret-1', // Same inaccessible scene
        timestamp: 160,
      },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);

    // Only one "?" node for scene-secret-1
    const unknownNodes = result.nodes.filter((n) => n.sceneId === 'scene-secret-1');
    expect(unknownNodes).toHaveLength(1);
    expect(unknownNodes[0].sceneName).toBe('？');
    expect(unknownNodes[0].isUnknown).toBe(true);

    // Both transitions should be dotted edges
    const dottedEdges = result.edges.filter((e) => e.isDotted);
    expect(dottedEdges).toHaveLength(2);
    expect(dottedEdges[0].toNodeId).toBe(unknownNodes[0].id);
    expect(dottedEdges[1].toNodeId).toBe(unknownNodes[0].id);
  });

  // spec: alhalte-argenterie/scene-graph 再訪時の点複製
  it('duplicates nodes when a character returns to a previously visited scene', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '旅人', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '旅人',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 150,
      },
      {
        characterId: 'c1',
        characterName: '旅人',
        fromSceneId: 'scene-b',
        toSceneId: 'scene-a', // Re-visit scene-a!
        timestamp: 200,
      },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);
    expect(result.nodes).toHaveLength(3);

    const aNodes = result.nodes.filter((n) => n.sceneId === 'scene-a');
    expect(aNodes).toHaveLength(2);
    expect(aNodes[0].id).not.toBe(aNodes[1].id); // Distinct duplicated node
    expect(aNodes[1].isDuplicate).toBe(true);
  });

  // spec: alhalte-argenterie/scene-graph SceneGraphTab コンポーネント描画
  it('renders SceneGraphTab and triggers scene navigation on clicking a node', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '探検家', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '探検家',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 120,
      },
    ];

    let navigatedSceneId: string | null = null;
    const instance = mount(SceneGraphTab, {
      target,
      props: {
        movements,
        sceneNames,
        accessibleSceneIds: accessibleScenes,
        onNavigateToScene: (id) => {
          navigatedSceneId = id;
        },
      },
    });

    // Check SVG lines and nodes are rendered
    expect(target.querySelector('[data-testid="graph-svg-layer"]')).toBeTruthy();

    const nodeBtn = target.querySelector<HTMLButtonElement>('[data-testid^="node-label-"]');
    expect(nodeBtn).toBeTruthy();
    nodeBtn?.click();

    expect(navigatedSceneId).toBe('scene-a');

    unmount(instance);
    target.remove();
  });
});
