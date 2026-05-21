import React from 'react';

export function StatusPill({ kind = 'muted', children }) {
  return <span className={`pill ${kind}`}>{children}</span>;
}
