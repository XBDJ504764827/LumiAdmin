import React, { useCallback, useRef, useState } from 'react';

export function SearchBar({ value, onChange, placeholder = '搜索...', statusOptions, statusValue, onStatusChange, debounceMs = 400 }) {
  const [local, setLocal] = useState(value ?? '');
  const timerRef = useRef(null);

  const handleChange = useCallback((e) => {
    const v = e.target.value;
    setLocal(v);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => onChange(v), debounceMs);
  }, [onChange, debounceMs]);

  return (
    <div className="search-bar">
      <input
        type="text"
        className="form-control search-bar-input"
        placeholder={placeholder}
        value={local}
        onChange={handleChange}
      />
      {statusOptions && statusOptions.length > 0 ? (
        <select
          className="form-control search-bar-select"
          value={statusValue ?? ''}
          onChange={(e) => onStatusChange?.(e.target.value || undefined)}
        >
          <option value="">全部状态</option>
          {statusOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>{opt.label}</option>
          ))}
        </select>
      ) : null}
    </div>
  );
}
