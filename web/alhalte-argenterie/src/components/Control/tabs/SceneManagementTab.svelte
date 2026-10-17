<script lang="ts">
  /**
   * Scene Management Tab.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
   */
  import { Button, Card } from 'noph-ui';
  import type { Scene } from '../../../types/index.js';

  let {
    scenes = [],
    currentSceneId,
    isAdmin = false,
    onSelectScene,
    onCreateScene,
    onDeleteScene,
  }: {
    scenes?: Scene[];
    currentSceneId?: string;
    isAdmin?: boolean;
    onSelectScene?: (sceneId: string) => void;
    onCreateScene?: () => void;
    onDeleteScene?: (sceneId: string) => void;
  } = $props();
</script>

<div class="scene-management-container" data-testid="scene-management-tab">
  <div class="scene-management-header">
    <h3 class="tab-title">シーン一覧</h3>
    {#if isAdmin}
      <Button variant="filled" onclick={onCreateScene} data-testid="create-scene-btn-mgmt">
        新規シーン
      </Button>
    {/if}
  </div>

  <div class="scenes-list" data-testid="scene-list">
    {#each scenes as sc (sc.id)}
      <div class="scene-card-item" data-testid="scene-item-{sc.id}">
        <Card variant={sc.id === currentSceneId ? 'filled' : 'outlined'}>
          <div class="scene-card-content">
            <div class="scene-meta">
              <span class="scene-name">{sc.name}</span>
              <span class="scene-members-count">メンバー: {sc.members.length}人</span>
            </div>
            <div class="scene-actions">
              <Button
                variant={sc.id === currentSceneId ? 'tonal' : 'outlined'}
                onclick={() => onSelectScene?.(sc.id)}
                data-testid="select-scene-btn-{sc.id}"
              >
                {sc.id === currentSceneId ? '現在地' : '移動'}
              </Button>
              {#if isAdmin && sc.id !== '/'}
                <Button
                  variant="text"
                  onclick={() => onDeleteScene?.(sc.id)}
                  data-testid="delete-scene-btn-{sc.id}"
                >
                  削除
                </Button>
              {/if}
            </div>
          </div>
        </Card>
      </div>
    {/each}
  </div>
</div>

<style>
  .scene-management-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    box-sizing: border-box;
    overflow-y: auto;
    background: var(--md-sys-color-surface, #fbf8fd);
    gap: 16px;
  }
  .scene-management-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .tab-title {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
  }
  .scenes-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .scene-card-content {
    padding: 12px 16px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .scene-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .scene-name {
    font-weight: 600;
    font-size: 1rem;
  }
  .scene-members-count {
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
  }
  .scene-actions {
    display: flex;
    gap: 8px;
  }
</style>
