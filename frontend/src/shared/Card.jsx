import React from 'react';

export function Card({ title, subtitle, children }) {
  return (
    <article className="card">
      <div className="card-header"><div><div className="card-title">{title}</div><div className="card-sub">{subtitle}</div></div></div>
      <div className="card-body">{children}</div>
    </article>
  );
}
