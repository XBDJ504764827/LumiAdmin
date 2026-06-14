import React, { useCallback, useEffect, useRef, useState } from 'react';

export function SearchBar({ value, onChange, placeholder = '搜索...', statusOptions, statusValue, onStatusChange, debounceMs = 400 }) {
  const [local, setLocal] = useState(value ?? '');
  const timerRef = useRef(null);

  useEffect(() => {
    React.startTransition(() => { setLocal(value ?? ''); });
  }, [value]);

  useEffect(() => () => {
    if (timerRef.current) clearTimeout(timerRef.current);
  }, []);

  const handleChange = useCallback((e) => {
    const v = e.target.value;
    setLocal(v);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => onChange(v), debounceMs);
  }, [onChange, debounceMs]);

  return (
    <div className="search-bar" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
      <div className="search-bar-box">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" width="15" height="15" style={{ flexShrink: 0, color: 'var(--text3)' }}>
          <circle cx="7" cy="7" r="4.5" />
          <path d="M10.5 10.5L14 14" />
        </svg>
        <input
          type="text"
          placeholder={placeholder}
          value={local}
          onChange={handleChange}
        />
      </div>
      {statusOptions && statusOptions.length > 0 ? (
        <select
          className="filter-select"
          value={statusValue ?? ''}
          onChange={(e) => onStatusChange?.(e.target.value || undefined)}
        >
          <option value="">全部状态</option>
          {statusOptions.map((opt) => (
            <option key={opt.value ?? '__empty__'} value={opt.value}>{opt.label}</option>
          ))}
        </select>
      ) : null}
    </div>
  );
}
