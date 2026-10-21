import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  buildSceneGraph,
  collectKnownSceneIds,
  filterOwnedMovements,
  type SceneMovementEvent,
} from '../src/utils/sceneGraph.js';
import SceneGraphTab from '../src/components/Control/tabs/SceneGraphTab.svelte';
import App from '../src/components/App.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Scene Graph Algorithm & View', () => {
  const sceneNames = new Map<string, string>([
    ['scene-a', '街頭'],
    ['scene-b', '地下迷宮'],
    ['scene-c', '神殿'],
    ['scene-d', '独立した塔'], // Uncorrelated
  ]);

  const accessibleScenes = new Set<string>(['scene-a', 'scene-b', 'scene-c', 'scene-d']);

  // spec: alhalte-argenterie/scene-graph 初期メインシーン表示 (Issue #29 Problem 1)
  it('displays initial root node for "/" (メインシーン) even when movements is empty', () => {
    const rootSceneNames = new Map<string, string>([['/', 'メインシーン']]);
    const rootAccessibleScenes = new Set<string>(['/']);

    const result = buildSceneGraph([], rootSceneNames, rootAccessibleScenes);
    expect(result.nodes).toHaveLength(1);
    expect(result.nodes[0].sceneId).toBe('/');
    expect(result.nodes[0].sceneName).toBe('メインシーン');
    expect(result.nodes[0].branchIndex).toBe(0);
    expect(result.edges).toHaveLength(0);
    expect(result.branchCount).toBe(1);
  });

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

  // spec: alhalte-argenterie/scene-graph Y軸平行レイアウト (Issue #29 Problem 3)
  it('keeps single linear path A -> B -> C on branchIndex 0 (parallel to Y-axis)', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '勇者', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '勇者',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 200,
      },
      {
        characterId: 'c1',
        characterName: '勇者',
        fromSceneId: 'scene-b',
        toSceneId: 'scene-c',
        timestamp: 300,
      },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);
    expect(result.nodes).toHaveLength(3);
    expect(result.nodes.every((n) => n.branchIndex === 0)).toBe(true);
    expect(result.branchCount).toBe(1);

    // Edges are strictly vertical (both fromNode and toNode on branchIndex 0)
    for (const edge of result.edges) {
      const fromNode = result.nodes.find((n) => n.id === edge.fromNodeId);
      const toNode = result.nodes.find((n) => n.id === edge.toNodeId);
      expect(fromNode?.branchIndex).toBe(0);
      expect(toNode?.branchIndex).toBe(0);
    }
  });

  // spec: alhalte-argenterie/scene-graph 分岐と合流 (Issue #29 Problem 2 & 3)
  it('branches A -> B, A -> C and correctly merges B -> D, C -> D into a single D node on lane 0 without A -> D edge', () => {
    const scenesWithD = new Map([
      ['scene-a', '街頭'],
      ['scene-b', '地下迷宮'],
      ['scene-c', '神殿'],
      ['scene-d', '決戦場'],
    ]);
    const accessible = new Set(['scene-a', 'scene-b', 'scene-c', 'scene-d']);

    const movements: SceneMovementEvent[] = [
      {
        characterId: 'c1',
        characterName: '勇者',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 100,
      },
      {
        characterId: 'c2',
        characterName: '魔法使い',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-c',
        timestamp: 110,
      },
      {
        characterId: 'c1',
        characterName: '勇者',
        fromSceneId: 'scene-b',
        toSceneId: 'scene-d',
        timestamp: 120,
      },
      {
        characterId: 'c2',
        characterName: '魔法使い',
        fromSceneId: 'scene-c',
        toSceneId: 'scene-d',
        timestamp: 130,
      },
    ];

    const result = buildSceneGraph(movements, scenesWithD, accessible);

    // Should have nodes: scene-a, scene-b, scene-c, scene-d (exactly 1 node for scene-d)
    const nodeA = result.nodes.find((n) => n.sceneId === 'scene-a');
    const nodeB = result.nodes.find((n) => n.sceneId === 'scene-b');
    const nodeC = result.nodes.find((n) => n.sceneId === 'scene-c');
    const dNodes = result.nodes.filter((n) => n.sceneId === 'scene-d');

    expect(nodeA).toBeDefined();
    expect(nodeB).toBeDefined();
    expect(nodeC).toBeDefined();
    expect(dNodes).toHaveLength(1);
    const nodeD = dNodes[0];

    // Node A and B on lane 0, C branches to lane 1, D merges back onto lane 0
    expect(nodeA!.branchIndex).toBe(0);
    expect(nodeB!.branchIndex).toBe(0);
    expect(nodeC!.branchIndex).toBe(1);
    expect(nodeD.branchIndex).toBe(0);

    // Chronological order: A, B, C, D (D at the bottom)
    expect(result.nodes[result.nodes.length - 1].id).toBe(nodeD.id);

    // Verify edges: A -> B, A -> C, B -> D, C -> D
    const edgeAB = result.edges.find((e) => e.fromNodeId === nodeA!.id && e.toNodeId === nodeB!.id);
    const edgeAC = result.edges.find((e) => e.fromNodeId === nodeA!.id && e.toNodeId === nodeC!.id);
    const edgeBD = result.edges.find((e) => e.fromNodeId === nodeB!.id && e.toNodeId === nodeD.id);
    const edgeCD = result.edges.find((e) => e.fromNodeId === nodeC!.id && e.toNodeId === nodeD.id);

    expect(edgeAB).toBeDefined();
    expect(edgeAC).toBeDefined();
    expect(edgeBD).toBeDefined();
    expect(edgeCD).toBeDefined();

    // MUST NOT have direct edge A -> D!
    const edgeAD = result.edges.find((e) => e.fromNodeId === nodeA!.id && e.toNodeId === nodeD.id);
    expect(edgeAD).toBeUndefined();
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

  // spec: alhalte-argenterie/scene-graph Issue #263: 未知シーンは独立した「？」ノードと点線
  it('draws one independent "?" node per frontier edge without leaking the unknown scene', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '潜入者', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '潜入者',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-secret-1', // Not in V_known
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
        toSceneId: 'scene-secret-1', // Same actual scene, but still anonymous
        timestamp: 160,
      },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);

    // One independent "?" node per frontier edge: merging them would reveal
    // that both transitions lead to the same hidden scene.
    const unknownNodes = result.nodes.filter((n) => n.isUnknown);
    expect(unknownNodes).toHaveLength(2);
    expect(unknownNodes[0].id).not.toBe(unknownNodes[1].id);
    for (const node of unknownNodes) {
      expect(node.sceneName).toBe('？');
      expect(node.sceneId).toBe('');
      expect(result.nodes.some((n) => n !== node && n.sceneId === 'scene-secret-1')).toBe(false);
    }

    // Both transitions are dotted edges into distinct unknown nodes.
    const dottedEdges = result.edges.filter((e) => e.isDotted);
    expect(dottedEdges).toHaveLength(2);
    expect(new Set(dottedEdges.map((edge) => edge.toNodeId))).toEqual(
      new Set(unknownNodes.map((node) => node.id)),
    );
  });

  // spec: alhalte-argenterie/scene-graph Issue #263: 未知 scene からの退出は edge にしない
  it('does not invent a frontier edge when a movement leaves an unknown scene', () => {
    const movements: SceneMovementEvent[] = [
      {
        characterId: 'mine',
        characterName: '自キャラ',
        fromSceneId: 'scene-secret',
        toSceneId: 'scene-a',
        timestamp: 100,
      },
    ];

    const result = buildSceneGraph(movements, sceneNames, accessibleScenes);

    // Only the known destination is drawn; there is no known source to attach
    // a dotted edge to, so no orphan "?" node is created.
    expect(result.nodes.map((node) => node.sceneId)).toEqual(['scene-a']);
    expect(result.nodes.some((node) => node.isUnknown)).toBe(false);
    expect(result.edges).toHaveLength(0);
  });

  // spec: alhalte-argenterie/scene-graph Issue #263: 保有キャラクターの移動だけを残す
  it('filters movements to the characters owned by the caller', () => {
    const movements: SceneMovementEvent[] = [
      { characterId: 'mine', characterName: '自キャラ', toSceneId: 'scene-a', timestamp: 100 },
      { characterId: 'other', characterName: '他人', toSceneId: 'scene-b', timestamp: 110 },
    ];

    expect(filterOwnedMovements(movements, new Set(['mine']))).toEqual([movements[0]]);
    expect(filterOwnedMovements(movements, new Set())).toEqual([]);
  });

  // spec: alhalte-argenterie/scene-graph Issue #263: V_known は保有キャラクターの到達 scene + seed
  it('computes V_known from owned movements, current scene and present characters only', () => {
    const movements: SceneMovementEvent[] = [
      {
        characterId: 'mine',
        characterName: '自キャラ',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 100,
      },
      {
        characterId: 'mine',
        characterName: '自キャラ',
        fromSceneId: 'scene-b',
        toSceneId: 'deleted-scene',
        timestamp: 120,
      },
    ];
    const sceneIds = new Set(['/', 'scene-a', 'scene-b', 'scene-c']);

    const known = collectKnownSceneIds(movements, sceneIds, ['/', 'scene-c']);

    // scene-c is the seed (current scene / owned character present), while the
    // deleted destination is not a known scene and must stay a frontier node.
    expect([...known].sort()).toEqual(['/', 'scene-a', 'scene-b', 'scene-c']);
  });

  // spec: alhalte-argenterie/scene-graph Issue #263: App は保有キャラクターのグラフに絞る
  it('scopes the App scene graph to the owned characters and keeps frontier nodes anonymous', async () => {
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', '1001');

    const makeScene = (id: string, name: string, members: string[]): Scene => ({
      id,
      name,
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members,
    });
    const scenes = [
      makeScene('/', 'メインシーン', ['mine']),
      makeScene('scene-a', '街頭', ['mine']),
      makeScene('scene-b', '地下迷宮', ['other']),
    ];
    const characters: Character[] = [
      {
        id: 'mine',
        name: '自キャラ',
        initiative: 10,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
        ownerAccountId: '1001',
      },
      {
        id: 'other',
        name: '他人のキャラ',
        initiative: 10,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
        ownerAccountId: '2002',
      },
    ];
    const movements: SceneMovementEvent[] = [
      {
        characterId: 'mine',
        characterName: '自キャラ',
        fromSceneId: '/',
        toSceneId: 'scene-a',
        timestamp: 100,
      },
      {
        characterId: 'other',
        characterName: '他人のキャラ',
        fromSceneId: '/',
        toSceneId: 'scene-b',
        timestamp: 110,
      },
      {
        characterId: 'mine',
        characterName: '自キャラ',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-secret',
        timestamp: 120,
      },
    ];
    const room: VisitedRoom = {
      id: '0000000000000263',
      name: 'シーングラフ絞り込みテスト',
      url: 'https://alhalte.example/rooms/0000000000000263#key',
      lastVisited: Date.now(),
    };

    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: room,
        initialScenes: scenes,
        initialCharacters: characters,
        initialMovements: movements,
        userRole: 'participant',
      },
    });
    await new Promise((r) => setTimeout(r, 30));

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]')?.click();
    await new Promise((r) => setTimeout(r, 40));

    const labels = Array.from(target.querySelectorAll('[data-testid^="node-label-"]'))
      .map((element) => element.textContent ?? '')
      .join(' ');
    // The owned character reached scene-a; the other character's scene-b route
    // must not leak the scene name. It is shown only as a frontier "?" node
    // because scene-b is outside this participant's V_known.
    expect(labels).toContain('街頭');
    expect(labels).not.toContain('地下迷宮');

    // Two unreached destinations (scene-b and scene-secret) are anonymous,
    // independent frontier nodes drawn with dotted edges.
    const frontierDots = target.querySelectorAll('[data-testid^="node-dot-node-unknown-"]');
    expect(frontierDots.length).toBe(2);
    for (const dot of Array.from(frontierDots)) {
      expect(dot.textContent?.trim()).toBe('?');
    }
    const dottedEdge = Array.from(target.querySelectorAll('[data-testid^="edge-"]')).find(
      (element) => element.getAttribute('stroke-dasharray') === '4,4',
    );
    expect(dottedEdge).toBeTruthy();
    expect(labels).not.toContain('scene-secret');

    unmount(instance);
    target.remove();
    document.body.innerHTML = '';
    localStorage.clear();
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

  // spec: alhalte-argenterie/scene-graph SceneGraphTab コンポーネント描画 & キャラクター名非表示 (Issue #29 Problem 4)
  it('renders SceneGraphTab without character name and triggers scene navigation on clicking a node', async () => {
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

    // Check SVG lines (paths) and nodes are rendered
    expect(target.querySelector('[data-testid="graph-svg-layer"]')).toBeTruthy();

    // Check SVG path exists
    const svgPath = target.querySelector<SVGPathElement>('[data-testid^="edge-"]');
    expect(svgPath).toBeTruthy();
    expect(svgPath?.getAttribute('d')).toContain('M');

    // Issue #29 Problem 4: character name must NOT be rendered next to scene title
    expect(target.querySelector('.character-mention')).toBeNull();
    const nodeBtn = target.querySelector<HTMLButtonElement>('[data-testid^="node-label-"]');
    expect(nodeBtn).toBeTruthy();
    expect(nodeBtn?.textContent).not.toContain('探検家');

    // Navigation trigger
    nodeBtn?.click();
    expect(navigatedSceneId).toBe('scene-a');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-graph 空の履歴時のメインシーン表示 (Issue #29 Problem 1)
  it('renders SceneGraphTab with initial メインシーン when movements is empty', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(SceneGraphTab, {
      target,
      props: {
        movements: [],
        sceneNames: new Map([['/', 'メインシーン']]),
        accessibleSceneIds: new Set(['/']),
      },
    });

    // Empty notice must NOT be rendered because main scene exists
    expect(target.querySelector('[data-testid="empty-graph-notice"]')).toBeNull();

    // Main scene label should be rendered
    const nodeBtn = target.querySelector<HTMLButtonElement>('[data-testid^="node-label-"]');
    expect(nodeBtn).toBeTruthy();
    expect(nodeBtn?.textContent).toContain('メインシーン');

    // Clicking main scene "/" should NOT reveal a delete button
    nodeBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid^="node-delete-btn-"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-graph シーンタブ統合・左クリックで削除ボタン表示と全画面確認ダイアログ (Issue #30)
  it('reveals delete button on left-clicking a scene node and deletes after full-screen confirmation dialog', async () => {
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

    let deletedSceneId: string | null = null;
    const instance = mount(SceneGraphTab, {
      target,
      props: {
        movements,
        sceneNames,
        accessibleSceneIds: accessibleScenes,
        role: 'admin',
        onDeleteScene: (id) => {
          deletedSceneId = id;
        },
      },
    });

    // Initially delete button and confirmation dialog are not shown
    expect(target.querySelector('[data-testid="node-delete-btn-node-scene-a-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeNull();

    // Left-click node 0 ('scene-a' / '街頭')
    const nodeDot = target.querySelector<HTMLButtonElement>('[data-testid="node-dot-node-scene-a-1"]');
    expect(nodeDot).toBeTruthy();
    nodeDot?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Delete button appears on node-scene-a-1
    const deleteBtn = target.querySelector<HTMLElement>('[data-testid="node-delete-btn-node-scene-a-1"]');
    expect(deleteBtn).toBeTruthy();

    // Click delete button -> opens full-screen confirmation dialog
    deleteBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const dialog = target.querySelector('[data-testid="graph-delete-confirm-dialog"]');
    expect(dialog).toBeTruthy();
    expect(target.querySelector('[data-testid="graph-delete-dialog-title"]')?.textContent).toContain(
      'シーン「街頭」を削除しますか？',
    );

    // Confirm deletion
    const confirmBtn = target.querySelector<HTMLElement>('[data-testid="graph-confirm-delete-btn"]');
    expect(confirmBtn).toBeTruthy();
    confirmBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(deletedSceneId).toBe('scene-a');
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-graph シーンタブ統合・長押しで削除ボタン表示とキャンセル操作 (Issue #30)
  it('reveals delete button on long-pressing a scene node and allows canceling confirmation dialog', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const movements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '探検家', toSceneId: 'scene-b', timestamp: 100 },
    ];

    let deletedSceneId: string | null = null;
    const instance = mount(SceneGraphTab, {
      target,
      props: {
        movements,
        sceneNames,
        accessibleSceneIds: accessibleScenes,
        role: 'admin',
        onDeleteScene: (id) => {
          deletedSceneId = id;
        },
      },
    });

    const nodeLabel = target.querySelector<HTMLButtonElement>('[data-testid="node-label-node-scene-b-1"]');
    expect(nodeLabel).toBeTruthy();

    // Long-press via pointerdown for > 450ms
    nodeLabel?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 480));
    nodeLabel?.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    const deleteBtn = target.querySelector<HTMLElement>('[data-testid="node-delete-btn-node-scene-b-1"]');
    expect(deleteBtn).toBeTruthy();

    // Open confirmation dialog and cancel
    deleteBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeTruthy();

    const cancelBtn = target.querySelector<HTMLElement>('[data-testid="graph-cancel-delete-btn"]');
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(deletedSceneId).toBeNull();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
