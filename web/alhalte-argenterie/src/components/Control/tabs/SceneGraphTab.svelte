<script lang="ts">
  /**
   * Scene Graph Tab (VSCode Git Graph-like view).
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーングラフタブ`
   */
  import type { SceneGraphResult } from '../../../types/index.js';
  import { buildSceneGraph, type SceneMovementEvent } from '../../../utils/sceneGraph.js';

  let {
    movements = [],
    sceneNames = new Map<string, string>(),
    accessibleSceneIds = new Set<string>(),
    onNavigateToScene,
  }: {
    movements?: SceneMovementEvent[];
    sceneNames?: Map<string, string>;
    accessibleSceneIds?: Set<string>;
    onNavigateToScene?: (sceneId: string) => void;
  } = $props();

  let graphData = $derived.by<SceneGraphResult>(() => {
    return buildSceneGraph(movements, sceneNames, accessibleSceneIds);
  });

  const LANE_WIDTH = 32;
  const ROW_HEIGHT = 44;

  // Colors for branches
  const BRANCH_COLORS = ['#6750a4', '#006a6a', '#b3261e', '#795548', '#2e7d32', '#c2185b'];

  function getBranchColor(branchIndex: number): string {
    return BRANCH_COLORS[branchIndex % BRANCH_COLORS.length];
  }

  function handleNodeClick(sceneId: string, isUnknown?: boolean) {
    if (isUnknown) return; // Cannot navigate to inaccessible scenes
    onNavigateToScene?.(sceneId);
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
          {#each graphData.edges as edge (edge.fromNodeId + '-' + edge.toNodeId)}
            {@const fromIdx = graphData.nodes.findIndex((n) => n.id === edge.fromNodeId)}
            {@const toIdx = graphData.nodes.findIndex((n) => n.id === edge.toNodeId)}
            {#if fromIdx !== -1 && toIdx !== -1}
              {@const fromNode = graphData.nodes[fromIdx]}
              {@const toNode = graphData.nodes[toIdx]}
              {@const x1 = fromNode.branchIndex * LANE_WIDTH + 20}
              {@const y1 = fromIdx * ROW_HEIGHT + 22}
              {@const x2 = toNode.branchIndex * LANE_WIDTH + 20}
              {@const y2 = toIdx * ROW_HEIGHT + 22}
              <line
                {x1}
                {y1}
                {x2}
                {y2}
                stroke={edge.isDotted ? '#999' : getBranchColor(fromNode.branchIndex)}
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
                style="left: {x - 8}px; border-color: {node.isUnknown ? '#999' : getBranchColor(node.branchIndex)};"
                onclick={() => handleNodeClick(node.sceneId, node.isUnknown)}
                data-testid="node-dot-{node.id}"
                title={node.isUnknown ? 'アクセス権のないシーン' : `${node.sceneName} へ移動`}
              >
                {#if node.isUnknown}
                  ?
                {/if}
              </button>

              <!-- Scene name label and character indicator -->
              <div
                class="node-labels"
                style="left: {graphData.branchCount * LANE_WIDTH + 24}px;"
              >
                <button
                  type="button"
                  class="scene-name-button"
                  class:disabled-label={node.isUnknown}
                  onclick={() => handleNodeClick(node.sceneId, node.isUnknown)}
                  data-testid="node-label-{node.id}"
                >
                  <span class="scene-title">{node.sceneName}</span>
                  <span class="character-mention">({node.characterName})</span>
                  <span class="node-time">{new Date(node.timestamp * 1000).toLocaleTimeString()}</span>
                </button>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .scene-graph-container {
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface, #fbf8fd);
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
    color: var(--md-sys-color-on-surface-variant, #49454f);
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
    border-radius: 50%;
    background: #fff;
    border: 3px solid #6750a4;
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
    background: #e0e0e0;
    font-size: 10px;
    font-weight: 700;
    color: #666;
    border-style: dashed;
    cursor: default;
  }
  .graph-commit-dot.unknown-dot:hover {
    transform: none;
  }
  .node-labels {
    position: absolute;
    white-space: nowrap;
  }
  .scene-name-button {
    background: none;
    border: none;
    padding: 2px 6px;
    border-radius: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.88rem;
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .scene-name-button:hover:not(.disabled-label) {
    background: rgba(103, 80, 164, 0.08);
  }
  .scene-name-button.disabled-label {
    cursor: default;
    color: #888;
  }
  .scene-title {
    font-weight: 600;
  }
  .character-mention {
    font-size: 0.78rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
  }
  .node-time {
    font-size: 0.72rem;
    color: #888;
  }
</style>
