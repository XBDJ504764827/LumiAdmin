import React, { useEffect, useState } from 'react';
import { api } from '../lib/api.js';
import { useAuth } from '../state/store.js';

/**
 * Hook to fetch a player's internal profile (notes + tags).
 * Returns { profile, loading }.
 * Only fetches if the user has admin/developer role.
 */
export function usePlayerInternalProfile(steamid64) {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const canView = session?.role === 'developer' || session?.role === 'admin';
  const [profile, setProfile] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!steamid64 || !token || !canView) {
      React.startTransition(() => { setProfile(null); });
      return;
    }
    let cancelled = false;
    React.startTransition(() => { setLoading(true); });
    api.getPlayerInternalProfile(token, steamid64)
      .then((result) => {
        if (!cancelled) setProfile(result.internal_profile ?? null);
      })
      .catch(() => {
        if (!cancelled) setProfile(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [steamid64, token, canView]);

  return { profile, loading };
}

/**
 * Compact display of a player's internal note.
 * Shows: tags as pills, note text, and "updated_by · updated_at" line.
 * Uses the usePlayerInternalProfile hook internally.
 */
export function InternalNoteBadge({ steamid64 }) {
  const { profile, loading } = usePlayerInternalProfile(steamid64);

  if (!steamid64 || loading) return null;
  if (!profile || (!profile.note && (!profile.tags || profile.tags.length === 0))) return null;

  return (
    <div style={{
      fontSize: 12,
      color: 'var(--text2)',
      padding: '8px 10px 8px 12px',
      background: 'var(--surface2)',
      borderRadius: 6,
      marginTop: 6,
      borderLeft: '3px solid var(--accent2)',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: profile.note ? 6 : 0 }}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" style={{ flexShrink: 0 }}>
          <rect x="3" y="1" width="9" height="13" rx="2" stroke="var(--accent2)" strokeWidth="1.5" fill="color-mix(in srgb, var(--accent2) 15%, transparent)" />
          <line x1="5.5" y1="5" x2="9.5" y2="5" stroke="var(--accent2)" strokeWidth="1.2" strokeLinecap="round" />
          <line x1="5.5" y1="7.5" x2="9.5" y2="7.5" stroke="var(--accent2)" strokeWidth="1.2" strokeLinecap="round" />
          <line x1="5.5" y1="10" x2="8" y2="10" stroke="var(--accent2)" strokeWidth="1.2" strokeLinecap="round" />
        </svg>
        <span style={{ fontWeight: 600, fontSize: 12, color: 'var(--text1)' }}>内部备注</span>
        {profile.tags && profile.tags.length > 0 && (
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
            {profile.tags.map((tag) => (
              <span key={tag} className="status-pill pill-idle" style={{ fontSize: 10, padding: '1px 6px' }}>{tag}</span>
            ))}
          </div>
        )}
      </div>
      {profile.note && (
        <div style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.5 }}>{profile.note}</div>
      )}
      {profile.updated_by && (
        <div style={{ fontSize: 11, color: 'var(--text3)', marginTop: 4 }}>
          {profile.updated_by} · {formatRelativeTime(profile.updated_at)}
        </div>
      )}
    </div>
  );
}

/**
 * Compact inline display of a player's internal note for table rows.
 * Shows only tags as small pills and a one-line truncated note preview.
 * Returns null when no note or tags exist — zero visual footprint.
 */
export function InternalNoteInline({ steamid64 }) {
  const { profile, loading } = usePlayerInternalProfile(steamid64);

  if (!steamid64 || loading || !profile) return null;
  if (!profile.note && (!profile.tags || profile.tags.length === 0)) return null;

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap', marginTop: 2 }}>
      <svg width="11" height="11" viewBox="0 0 16 16" fill="none" style={{ flexShrink: 0, opacity: 0.6 }}>
        <rect x="3" y="1" width="9" height="13" rx="2" stroke="var(--text2)" strokeWidth="1.5" fill="none" />
        <line x1="5.5" y1="5" x2="9.5" y2="5" stroke="var(--text2)" strokeWidth="1.2" strokeLinecap="round" />
        <line x1="5.5" y1="7.5" x2="9.5" y2="7.5" stroke="var(--text2)" strokeWidth="1.2" strokeLinecap="round" />
        <line x1="5.5" y1="10" x2="8" y2="10" stroke="var(--text2)" strokeWidth="1.2" strokeLinecap="round" />
      </svg>
      {profile.tags && profile.tags.length > 0 && profile.tags.map((tag) => (
        <span key={tag} className="status-pill pill-idle" style={{ fontSize: 10, padding: '1px 5px' }}>{tag}</span>
      ))}
      {profile.note && (
        <span style={{ fontSize: 11, color: 'var(--text3)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 200 }}>
          {profile.note.length > 40 ? profile.note.slice(0, 40) + '…' : profile.note}
        </span>
      )}
    </div>
  );
}

function formatRelativeTime(iso) {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  } catch { return iso; }
}
