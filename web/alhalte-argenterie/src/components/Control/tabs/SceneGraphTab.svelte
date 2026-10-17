<script lang="ts">
  /**
   * Scene Tab (VSCode Git Graph-like view with scene management actions).
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーングラフタブ`
   * - Issue #30: シーン管理タブをシーングラフタブに統合（シーンタブへ改名、左クリック・長押しで削除ボタン表示＋全画面警告ダイアログ）
   */
  import { Button } from 'noph-ui';
  import type { SceneGraphNode, SceneGraphResult } from '../../../types/index.js';
  import { buildSceneGraph, type SceneMovementEvent } from '../../../utils/sceneGraph.js';

  let {
    movements = [],
    sceneNames = new Map<string, string>(),
    accessibleSceneIds = new Set<string>(),
    isAdmin = true,
    onNavigateToScene,
    onOpenSceneLog,
    onDeleteScene,
    onRequestDeleteScene,
  }: {
    movements?: SceneMovementEvent[];
    sceneNames?: Map<string, string>;
    accessibleSceneIds?: Set<string>;
    isAdmin?: boolean;
    onNavigateToScene?: (sceneId: string) => void;
    onOpenSceneLog?: (sceneId: string) => void;
    onDeleteScene?: (sceneId: string) => void;
    onRequestDeleteScene?: (scene: { id: string; name: string }) => void;
  } = $props();

  let graphData = $derived.by<SceneGraphResult>(() => {
    return buildSceneGraph(movements, sceneNames, accessibleSceneIds);
  });

  let activeActionNodeId = $state<string | null>(null);
  let sceneToDelete = $state<{ sceneId: string; sceneName: string } | null>(null);

  const LONG_PRESS_DURATION_MS = 450;
  let longPressTimer: ReturnType<typeof setTimeout> | null = null;

  const LANE_WIDTH = 32;
  const ROW_HEIGHT = 44;

  // Colors for branches (M3 Expressive compatible)
  const BRANCH_COLORS = [
    'var(--np-color-primary, #6750a4)',
    'var(--np-color-secondary, #625b71)',
    'var(--np-color-tertiary, #7d5260)',
    'var(--np-color-primary-container, #4f378b)',
    'var(--np-color-error, #b3261e)',
    'var(--np-color-secondary-container, #4a4458)',
  ];

  function getBranchColor(branchIndex: number): string {
    return BRANCH_COLORS[branchIndex % BRANCH_COLORS.length];
  }

  function getEdgePath(x1: number, y1: number, x2: number, y2: number): string {
    if (x1 === x2) {
      return `M ${x1} ${y1} L ${x2} ${y2}`;
    }
    const midY = (y1 + y2) / 2;
    return `M ${x1} ${y1} C ${x1} ${midY}, ${x2} ${midY}, ${x2} ${y2}`;
  }

  function clearLongPressTimer() {
    if (longPressTimer !== null) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function handleNodeClick(node: SceneGraphNode) {
    clearLongPressTimer();
    if (node.isUnknown) return; // Cannot navigate to or manage inaccessible scenes
    activeActionNodeId = node.id;
    onNavigateToScene?.(node.sceneId);
  }

  function handleNodeDoubleClick(node: SceneGraphNode) {
    clearLongPressTimer();
    if (node.isUnknown) return;
    onNavigateToScene?.(node.sceneId);
    onOpenSceneLog?.(node.sceneId);
  }

  function handleNodePointerDown(node: SceneGraphNode) {
    clearLongPressTimer();
    if (node.isUnknown) return;
    longPressTimer = setTimeout(() => {
      activeActionNodeId = node.id;
      longPressTimer = null;
    }, LONG_PRESS_DURATION_MS);
  }

  function handleNodeContextMenu(e: MouseEvent, node: SceneGraphNode) {
    e.preventDefault();
    clearLongPressTimer();
    if (node.isUnknown) return;
    activeActionNodeId = node.id;
  }

  function openDeleteConfirm(e: MouseEvent, node: SceneGraphNode) {
    e.stopPropagation();
    if (onRequestDeleteScene) {
      activeActionNodeId = null;
      onRequestDeleteScene({
        id: node.sceneId,
        name: node.sceneName,
      });
    } else {
      sceneToDelete = {
        sceneId: node.sceneId,
        sceneName: node.sceneName,
      };
    }
  }

  function handleConfirmDelete() {
    if (!sceneToDelete) return;
    const targetId = sceneToDelete.sceneId;
    sceneToDelete = null;
    activeActionNodeId = null;
    onDeleteScene?.(targetId);
  }

  function handleCancelDelete() {
    sceneToDelete = null;
  }
</script>

<div class="scene-graph-container" data-testid="scene-graph-tab">
  <div class="graph-scroll-area">
    {#if graphData.nodes.length === 0}
      <div class="empty-graph-notice" data-testid="empty-graph-notice">
        シーン移動履歴がありません
      </div>
    {:else}
      <div
        class="graph-canvas-container"
        style="min-height: {graphData.nodes.length * ROW_HEIGHT + 20}px;"
        data-testid="graph-canvas-container"
      >
        <!-- SVG layer for Git Graph branch connection lines -->
        <svg
          class="graph-svg-layer"
          style="width: {Math.max(120, graphData.branchCount * LANE_WIDTH + 20)}px; height: {graphData.nodes.length * ROW_HEIGHT + 20}px;"
          data-testid="graph-svg-layer"
        >
          {#each graphData.edges as edge (edge.fromNodeId + '-' + edge.toNodeId + '-' + edge.characterId)}
            {@const fromIdx = graphData.nodes.findIndex((n) => n.id === edge.fromNodeId)}
            {@const toIdx = graphData.nodes.findIndex((n) => n.id === edge.toNodeId)}
            {#if fromIdx !== -1 && toIdx !== -1}
              {@const fromNode = graphData.nodes[fromIdx]}
              {@const toNode = graphData.nodes[toIdx]}
              {@const x1 = fromNode.branchIndex * LANE_WIDTH + 20}
              {@const y1 = fromIdx * ROW_HEIGHT + 22}
              {@const x2 = toNode.branchIndex * LANE_WIDTH + 20}
              {@const y2 = toIdx * ROW_HEIGHT + 22}
              {@const edgeColor = edge.isDotted
                ? 'var(--np-color-outline, #79747e)'
                : getBranchColor(
                    fromNode.branchIndex === toNode.branchIndex
                      ? fromNode.branchIndex
                      : Math.max(fromNode.branchIndex, toNode.branchIndex),
                  )}
              <path
                d={getEdgePath(x1, y1, x2, y2)}
                fill="none"
                stroke={edgeColor}
                stroke-width="2.5"
                stroke-dasharray={edge.isDotted ? '4,4' : undefined}
                data-testid="edge-{edge.fromNodeId}-{edge.toNodeId}"
              />
            {/if}
          {/each}
        </svg>

        <!-- HTML Nodes List (Earliest on top, latest at bottom) -->
        <div class="nodes-list">
          {#each graphData.nodes as node, idx (node.id)}
            {@const x = node.branchIndex * LANE_WIDTH + 20}
            {@const y = idx * ROW_HEIGHT + 22}
            <div
              class="graph-node-row"
              style="top: {y - 14}px;"
              data-testid="graph-row-{node.id}"
            >
              <!-- Commit-like dot -->
              <button
                type="button"
                class="graph-commit-dot"
                class:unknown-dot={node.isUnknown}
                style="left: {x - 8}px; border-color: {node.isUnknown ? 'var(--np-color-outline, #79747e)' : getBranchColor(node.branchIndex)};"
                onclick={() => handleNodeClick(node)}
                ondblclick={() => handleNodeDoubleClick(node)}
                onpointerdown={() => handleNodePointerDown(node)}
                onpointerup={clearLongPressTimer}
                onpointerleave={clearLongPressTimer}
                onpointercancel={clearLongPressTimer}
                oncontextmenu={(e) => handleNodeContextMenu(e, node)}
                data-testid="node-dot-{node.id}"
                title={node.isUnknown ? 'アクセス権のないシーン' : `${node.sceneName} へ移動`}
              >
                {#if node.isUnknown}
                  ?
                {/if}
              </button>

              <!-- Scene name label and revealed node actions -->
              <div
                class="node-labels"
                style="left: {graphData.branchCount * LANE_WIDTH + 24}px;"
              >
                <button
                  type="button"
                  class="scene-name-button"
                  class:disabled-label={node.isUnknown}
                  class:active-node={activeActionNodeId === node.id}
                  onclick={() => handleNodeClick(node)}
                  ondblclick={() => handleNodeDoubleClick(node)}
                  onpointerdown={() => handleNodePointerDown(node)}
                  onpointerup={clearLongPressTimer}
                  onpointerleave={clearLongPressTimer}
                  onpointercancel={clearLongPressTimer}
                  oncontextmenu={(e) => handleNodeContextMenu(e, node)}
                  data-testid="node-label-{node.id}"
                >
                  <span class="scene-title">{node.sceneName}</span>
                  <span class="node-time">{new Date(node.timestamp * 1000).toLocaleTimeString()}</span>
                </button>

                {#if activeActionNodeId === node.id && !node.isUnknown && isAdmin && node.sceneId !== '/'}
                  <div class="node-actions" data-testid="node-actions-{node.id}">
                    <div class="delete-btn-wrapper">
                      <Button
                        variant="tonal"
                        size="xs"
                        onclick={(e: MouseEvent) => openDeleteConfirm(e, node)}
                        data-testid="node-delete-btn-{node.id}"
                      >
                        削除
                      </Button>
                    </div>
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <!-- Full-Screen Confirmation Dialog for Scene Deletion -->
  {#if sceneToDelete}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="graph-delete-confirm-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) handleCancelDelete();
      }}
    >
      <div class="dialog-box warning-box">
        <h2 class="dialog-title-centered warning-text" data-testid="graph-delete-dialog-title">
          シーン「{sceneToDelete.sceneName}」を削除しますか？
        </h2>
        <p class="warning-body" data-testid="graph-delete-dialog-warning">
          このシーンを削除すると、配置されたログやJuices設定は元に戻せません。
        </p>

        <div class="dialog-footer-right">
          <Button
            variant="text"
            onclick={handleCancelDelete}
            data-testid="graph-cancel-delete-btn"
          >
            キャンセル
          </Button>
          <div class="confirm-delete-wrapper">
            <Button
              variant="filled"
              onclick={handleConfirmDelete}
              data-testid="graph-confirm-delete-btn"
            >
              削除する
            </Button>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .scene-graph-container {
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--np-color-surface, #fbf8fd);
    display: flex;
    flex-direction: column;
  }
  .graph-scroll-area {
    flex: 1;
    overflow: auto;
    padding: 16px;
    position: relative;
  }
  .empty-graph-notice {
    padding: 48px;
    text-align: center;
    color: var(--np-color-on-surface-variant, #49454f);
    font-size: 0.95rem;
  }
  .graph-canvas-container {
    position: relative;
    width: 100%;
  }
  .graph-svg-layer {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    z-index: 1;
  }
  .nodes-list {
    position: relative;
    width: 100%;
    z-index: 2;
  }
  .graph-node-row {
    position: absolute;
    left: 0;
    right: 0;
    height: 28px;
    display: flex;
    align-items: center;
  }
  .graph-commit-dot {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: var(--np-shape-corner-full, 50%);
    background: var(--np-color-surface, #fff);
    border: 3px solid var(--np-color-primary, #6750a4);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    box-sizing: border-box;
    transition: transform 0.15s;
  }
  .graph-commit-dot:hover {
    transform: scale(1.3);
  }
  .graph-commit-dot.unknown-dot {
    background: var(--np-color-surface-container-high, #e6e0e9);
    font-size: 10px;
    font-weight: 700;
    color: var(--np-color-on-surface-variant, #49454f);
    border-color: var(--np-color-outline, #79747e) !important;
    border-style: dashed;
    cursor: default;
  }
  .graph-commit-dot.unknown-dot:hover {
    transform: none;
  }
  .node-labels {
    position: absolute;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .scene-name-button {
    background: none;
    border: none;
    padding: 2px 8px;
    border-radius: var(--np-shape-corner-small, 8px);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.88rem;
    color: var(--np-color-on-surface, #1d1b20);
    transition: background-color 0.15s;
  }
  .scene-name-button:hover:not(.disabled-label),
  .scene-name-button.active-node:not(.disabled-label) {
    background: var(--np-color-surface-container-high, #e6e0e9);
  }
  .scene-name-button.disabled-label {
    cursor: default;
    color: var(--np-color-on-surface-variant, #79747e);
  }
  .scene-title {
    font-weight: 600;
  }
  .node-time {
    font-size: 0.72rem;
    color: var(--np-color-on-surface-variant, #79747e);
  }
  .node-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .delete-btn-wrapper {
    --np-button-container-color: var(--np-color-error-container, #f9dedc);
    --np-button-label-text-color: var(--np-color-on-error-container, #410e0b);
  }
  .fullscreen-dialog-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dialog-box {
    background: var(--np-color-surface-container-high, #ece6f0);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3, 0 8px 24px rgba(0, 0, 0, 0.2));
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface, #1d1b20);
  }
  .warning-text {
    color: var(--np-color-error, #b3261e);
  }
  .warning-body {
    margin: 0;
    text-align: center;
    font-size: 0.92rem;
    color: var(--np-color-on-surface-variant, #49454f);
    line-height: 1.5;
  }
  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .confirm-delete-wrapper {
    --np-button-container-color: var(--np-color-error, #b3261e);
    --np-button-label-text-color: var(--np-color-on-error, #ffffff);
  }
</style>
