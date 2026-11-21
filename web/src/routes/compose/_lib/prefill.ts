import { api, type Account, type MessageDetail } from '$lib/api';
import { formatSender } from '$lib/format';
import { dedupeAddrs, splitAddrs } from './addresses';

export type PrefillMode = 'reply' | 'reply_all' | 'forward';

export interface PrefillResult {
  fromAccountId: number;
  to?: string;
  cc?: string;
  showCcBcc: boolean;
  subject: string;
  body: string;
  inReplyTo: string | null;
  references: string | null;
}

/// Build a reply/forward draft from an existing message. Returns a
/// pure result the caller assigns onto its $state cells.
export async function computePrefill(
  id: number,
  mode: PrefillMode,
  accounts: Account[]
): Promise<PrefillResult> {
  const m: MessageDetail = await api.getMessage(id);
  const selfEmail = accounts.find((a) => a.id === m.account_id)?.email ?? '';

  let to: string | undefined;
  let cc: string | undefined;
  let showCcBcc = false;
  if (mode === 'reply' && m.from_addr) {
    to = m.from_addr;
  } else if (mode === 'reply_all') {
    // To = sender + the original To recipients, minus self.
    const senderList = m.from_addr ? [m.from_addr] : [];
    const toList = splitAddrs(m.to_addrs ?? '');
    const ccList = splitAddrs(m.cc_addrs ?? '');
    const dedupedTo = dedupeAddrs([...senderList, ...toList], selfEmail);
    const dedupedCc = dedupeAddrs(ccList, selfEmail, dedupedTo);
    to = dedupedTo.join(', ');
    cc = dedupedCc.join(', ');
    if (dedupedCc.length > 0) showCcBcc = true;
  }

  const prefix = mode === 'forward' ? 'Fwd: ' : 'Re: ';
  const subj = m.subject || '';
  const subject = subj.toLowerCase().startsWith(prefix.toLowerCase()) ? subj : prefix + subj;

  let inReplyTo: string | null = null;
  let references: string | null = null;
  if (mode === 'reply' || mode === 'reply_all') {
    inReplyTo = m.message_id;
    // RFC 5322 §3.6.4: References should be parent's References list
    // with the parent's Message-ID appended. Even though Postern itself
    // no longer threads, receiving clients (Gmail, Outlook, Apple Mail)
    // still group by References — emitting it correctly keeps our
    // outbound mail looking right on the other end.
    const chain = [...(m.references ?? []), m.message_id].filter(
      (id, i, arr) => arr.indexOf(id) === i
    );
    references = chain.join(' ');
  }

  // Quote body — lazy plain-text fetch so we don't wait on the HTML path.
  let body = '';
  try {
    const r = await api.getMessagePlain(m.id);
    const quoted = r.text
      .split('\n')
      .map((line) => `> ${line}`)
      .join('\n');
    const header =
      mode === 'forward'
        ? `\n--- Forwarded message ---\nFrom: ${m.from_addr ?? ''}\nDate: ${new Date(m.date_utc * 1000).toLocaleString()}\nSubject: ${m.subject ?? ''}\nTo: ${m.to_addrs ?? ''}\n\n`
        : `On ${new Date(m.date_utc * 1000).toLocaleString()}, ${formatSender(m.from_addr)} wrote:\n`;
    body = `\n\n${header}${quoted}`;
  } catch {
    /* body-fetch optional */
  }

  return {
    fromAccountId: m.account_id,
    to,
    cc,
    showCcBcc,
    subject,
    body,
    inReplyTo,
    references,
  };
}

/// Inject the account signature into a draft body. Idempotent — if a
/// `-- ` marker is already present, returns the body unchanged.
///
/// For 'new' compose, signature appends with a blank delimiter line.
/// For reply/forward, only inserts when `includeOnReplies` is true, and
/// places the sig between the user's typing area and the quoted block.
export function insertSignature(
  body: string,
  signature: string | null | undefined,
  mode: 'new' | PrefillMode,
  includeOnReplies: boolean
): string {
  const sig = signature?.trim();
  if (!sig) return body;
  if (body.includes('\n-- \n') || body.startsWith('-- \n')) return body;
  const block = `-- \n${sig}\n`;
  if (mode === 'new') {
    return body ? `${body}\n\n${block}` : `\n\n${block}`;
  }
  if (!includeOnReplies) return body;
  if (body.startsWith('\n\n')) {
    return `\n\n${block}\n` + body.slice(2);
  }
  return `\n\n${block}\n` + body;
}
