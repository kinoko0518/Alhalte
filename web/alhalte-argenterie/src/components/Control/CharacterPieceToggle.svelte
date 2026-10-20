<script lang="ts">
  /**
   * Piece placement toggle for one character row (Issue #228).
   *
   * `piece::<id>::placed` is server-authoritative (Issue #160), so the visual
   * toggle state must not flip before the acknowledgement:
   *
   * - `selected` mirrors `character.placedAsPiece` and is re-synced whenever the
   *   parent applies an acknowledged update,
   * - noph-ui `IconButton` toggles its own `selected` on click, so the click
   *   handler restores the current server value right away and only asks the
   *   parent to start the request.
   *
   * The icon, label and tooltip come from `character.placedAsPiece`, so they
   * switch together with the acknowledged state:
   * 駒として配置 / `person_pin_circle` -> 盤面から削除 / `remove_circle`.
   */
  import { IconButton } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character } from '../../types/index.js';

  let {
    character,
    disabled = false,
    onToggle,
  }: {
    character: Character;
    disabled?: boolean;
    onToggle?: (characterId: string) => void;
  } = $props();

  let selected = $state(Boolean(character.placedAsPiece));

  $effect(() => {
    selected = Boolean(character.placedAsPiece);
  });

  let label = $derived(character.placedAsPiece ? '盤面から削除' : '駒として配置');
  let icon = $derived(character.placedAsPiece ? 'remove_circle' : 'person_pin_circle');

  function handleClick(): void {
    // Undo IconButton's optimistic toggle; the parent commits after the ack.
    selected = Boolean(character.placedAsPiece);
    onToggle?.(character.id);
  }
</script>

<IconButton
  variant="tonal"
  toggle
  bind:selected
  onclick={handleClick}
  {disabled}
  aria-label={label}
  title={label}
  data-testid="piece-char-btn-{character.id}"
>
  <Icon>{icon}</Icon>
</IconButton>
