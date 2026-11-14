<script lang="ts">
  import InfoBubble from '$lib/components/InfoBubble.svelte';
</script>

<section class="panel">
  <div class="section-head">
    <h2>Automation</h2>
    <p>Filter rules, trusted senders and email templates.</p>
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
  <div class="row">
    <div class="label">
      <strong class="inline">
        Trusted Senders
        <InfoBubble text="Addresses you've marked as 'Not spam' bypass the spam folder forever — Postern auto-rescues their mail back to your inbox during sync." />
      </strong>
    </div>
    <a class="btn" href="/settings/trusted-senders">Manage →</a>
  </div>
  <div class="row">
    <div class="label">
      <strong class="inline">
        Email Templates
        <InfoBubble text="Save a subject and body you send often. Pick it from the Template row when composing — your signature is kept." />
      </strong>
    </div>
    <a class="btn" href="/settings/templates">Manage →</a>
  </div>
</section>
