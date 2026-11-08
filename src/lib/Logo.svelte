<script lang="ts">
  let { size = 20 }: { size?: number } = $props();
</script>

<img src="/logo.png" width={size} height={size} alt="Hermio" draggable="false" style:border-radius="{Math.round(size * 0.22)}px" />

<style>
  img {
    display: block;
    flex: none;
    user-select: none;
  }
</style>
