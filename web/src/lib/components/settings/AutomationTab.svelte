<script lang="ts">
  import InfoBubble from '$lib/components/InfoBubble.svelte';
</script>

<section class="panel">
  <div class="section-head">
    <h2>Automation</h2>
    <p>Filter rules.</p>
  </div>
  <div class="row">
    <div class="label">
      <strong class="inline">
        Message Rules
        <InfoBubble text="Auto-sort incoming mail by sender, subject, or keywords — move to folders, mark as read, or flag as spam." />
      </strong>
    </div>
    <a class="btn" href="/settings/rules">Manage →</a>
  </div>
</section>
