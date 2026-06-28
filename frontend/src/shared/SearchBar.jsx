import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { IconSearch, IconX } from './Icons.jsx';

function isFilled(value) {
  return value !== undefined && value !== null && String(value).trim() !== '';
}

function optionValue(value) {
  return value ?? '';
}

export function FilterToolbar({
  search,
  onSearchChange,
  searchPlaceholder = '搜索...',
  searchDebounceMs = 400,
  filters = [],
  actions = null,
  onSubmit,
  onReset,
  submitText = '查询',
  resetText = '清除',
  autoSubmit = true,
  activeCount,
  className = '',
}) {
  const [localSearch, setLocalSearch] = useState(search ?? '');
  const timerRef = useRef(null);

  useEffect(() => {
    React.startTransition(() => {
      setLocalSearch(search ?? '');
    });
  }, [search]);

  useEffect(() => () => {
    if (timerRef.current) clearTimeout(timerRef.current);
  }, []);

  const computedActiveCount = useMemo(() => {
    if (typeof activeCount === 'number') return activeCount;
    const searchCount = isFilled(localSearch) ? 1 : 0;
    const filterCount = filters.reduce((count, filter) => count + (isFilled(filter.value) ? 1 : 0), 0);
    return searchCount + filterCount;
  }, [activeCount, filters, localSearch]);

  const handleSearchChange = useCallback((event) => {
    const next = event.target.value;
    setLocalSearch(next);
    if (!onSearchChange) return;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => onSearchChange(next), searchDebounceMs);
  }, [onSearchChange, searchDebounceMs]);

  const commitSearch = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    onSearchChange?.(localSearch);
  }, [localSearch, onSearchChange]);

  const handleSubmit = useCallback((event) => {
    event?.preventDefault();
    commitSearch();
    onSubmit?.();
  }, [commitSearch, onSubmit]);

  const handleReset = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setLocalSearch('');
    onSearchChange?.('');
    filters.forEach((filter) => filter.onChange?.(''));
    onReset?.();
  }, [filters, onReset, onSearchChange]);

  return (
    <form className={`filter-toolbar ${className}`.trim()} onSubmit={handleSubmit}>
      <div className="filter-toolbar-main">
        {onSearchChange ? (
          <label className="filter-search">
            <IconSearch size={15} />
            <input
              type="search"
              placeholder={searchPlaceholder}
              value={localSearch}
              onChange={handleSearchChange}
              onBlur={commitSearch}
            />
          </label>
        ) : null}

        {filters.map((filter) => {
          if (filter.type === 'select') {
            return (
              <label key={filter.key} className="filter-field">
                {filter.label ? <span className="filter-field-label">{filter.label}</span> : null}
                <select
                  className="filter-select"
                  value={optionValue(filter.value)}
                  onChange={(event) => filter.onChange?.(event.target.value || undefined)}
                >
                  {filter.options.map((opt) => (
                    <option key={String(optionValue(opt.value)) || '__empty__'} value={optionValue(opt.value)}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </label>
            );
          }

          return (
            <label key={filter.key} className="filter-field">
              {filter.label ? <span className="filter-field-label">{filter.label}</span> : null}
              <input
                className="filter-input"
                type={filter.type || 'text'}
                placeholder={filter.placeholder}
                value={filter.value ?? ''}
                onChange={(event) => filter.onChange?.(event.target.value)}
              />
            </label>
          );
        })}
      </div>

      <div className="filter-toolbar-actions">
        {computedActiveCount > 0 ? (
          <span className="filter-active-count">{computedActiveCount} 项筛选</span>
        ) : null}
        {!autoSubmit || onSubmit ? (
          <button className="btn btn-primary btn-sm" type="submit">{submitText}</button>
        ) : null}
        {computedActiveCount > 0 ? (
          <button className="btn btn-outline btn-sm" type="button" onClick={handleReset}>
            <IconX size={14} />
            {resetText}
          </button>
        ) : null}
        {actions}
      </div>
    </form>
  );
}

export function SearchBar({
  value,
  onChange,
  placeholder = '搜索...',
  statusOptions,
  statusValue,
  onStatusChange,
  debounceMs = 400,
  onReset,
  ...props
}) {
  const filters = statusOptions && statusOptions.length > 0
    ? [{
      key: 'status',
      type: 'select',
      value: statusValue,
      onChange: onStatusChange,
      options: [
        { value: '', label: '全部状态' },
        ...statusOptions.filter((opt) => opt.value !== '' && opt.value !== undefined),
      ],
    }]
    : [];

  return (
    <FilterToolbar
      search={value}
      onSearchChange={onChange}
      searchPlaceholder={placeholder}
      searchDebounceMs={debounceMs}
      filters={filters}
      onReset={onReset}
      {...props}
    />
  );
}
