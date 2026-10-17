<script lang="ts">
  /**
   * Forwarding component for BoardCanvas / FieldPanel.
   */
  import FieldPanel from '../../components/Field/FieldPanel.svelte';
  let props = $props();
</script>

<FieldPanel {...props} />
