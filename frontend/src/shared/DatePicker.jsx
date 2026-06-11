import { useState, useRef, useEffect } from 'react';

const WEEKDAYS = ['一', '二', '三', '四', '五', '六', '日'];

function pad(n) {
  return n < 10 ? `0${n}` : `${n}`;
}

function toLocalISO(date) {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function isSameDay(a, b) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

function getDaysInMonth(year, month) {
  return new Date(year, month + 1, 0).getDate();
}

/**
 * 日期时间选择器（日历弹窗）
 * @param {Object} props
 * @param {string|null} props.value - 本地 ISO 格式的日期时间字符串（如 "2026-06-15T14:30"）
 * @param {Function} props.onChange - 值变化回调
 * @param {string} [props.placeholder] - 占位文本
 * @param {boolean} [props.disabled]
 * @param {Date} [props.minDate] - 最小可选日期
 */
export function DatePicker({ value, onChange, placeholder = '选择到期时间', disabled = false, minDate }) {
  const [open, setOpen] = useState(false);
  const [openCount, setOpenCount] = useState(0);
  const containerRef = useRef(null);

  const selectedDate = value ? new Date(value) : null;
  const now = new Date();
  const effectiveMin = minDate ?? now;

  const initYear = selectedDate?.getFullYear() ?? now.getFullYear();
  const initMonth = selectedDate?.getMonth() ?? now.getMonth();
  const initHours = selectedDate ? selectedDate.getHours() : 23;
  const initMinutes = selectedDate ? selectedDate.getMinutes() : 59;

  function toggleOpen() {
    if (disabled) return;
    setOpen((prev) => {
      if (!prev) {
        // 从关闭到打开，递增计数以重置日历面板
        setOpenCount((c) => c + 1);
      }
      return !prev;
    });
  }

  // 点击外部关闭
  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e) {
      if (containerRef.current && !containerRef.current.contains(e.target)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  const displayValue = value ? (() => {
    const d = new Date(value);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  })() : '';

  return (
    <div className="datepicker-container" ref={containerRef}>
      <div className="datepicker-input-wrap" onClick={toggleOpen}>
        <input
          type="text"
          className="datepicker-input form-control"
          value={displayValue}
          placeholder={placeholder}
          readOnly
          disabled={disabled}
        />
        <span className="datepicker-icon">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
            <rect x="1" y="3" width="14" height="12" rx="2" stroke="currentColor" strokeWidth="1.4" />
            <path d="M1 7h14M5 1v4M11 1v4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
        </span>
      </div>
      {open && !disabled ? (
        <CalendarPanel
          key={openCount}
          selectedDate={selectedDate}
          effectiveMin={effectiveMin}
          now={now}
          initYear={initYear}
          initMonth={initMonth}
          initHours={initHours}
          initMinutes={initMinutes}
          onChange={onChange}
        />
      ) : null}
    </div>
  );
}

function CalendarPanel({ selectedDate, effectiveMin, now, initYear, initMonth, initHours, initMinutes, onChange }) {
  const [viewYear, setViewYear] = useState(initYear);
  const [viewMonth, setViewMonth] = useState(initMonth);
  const [hours, setHours] = useState(initHours);
  const [minutes, setMinutes] = useState(initMinutes);

  function prevMonth() {
    if (viewMonth === 0) {
      setViewMonth(11);
      setViewYear((y) => y - 1);
    } else {
      setViewMonth((m) => m - 1);
    }
  }

  function nextMonth() {
    if (viewMonth === 11) {
      setViewMonth(0);
      setViewYear((y) => y + 1);
    } else {
      setViewMonth((m) => m + 1);
    }
  }

  function handleDayClick(day) {
    const date = new Date(viewYear, viewMonth, day, hours, minutes);
    if (date < effectiveMin) return;
    onChange(toLocalISO(date));
  }

  function handleTimeChange(newHours, newMinutes) {
    setHours(newHours);
    setMinutes(newMinutes);
    if (selectedDate) {
      const date = new Date(viewYear, viewMonth, selectedDate.getDate(), newHours, newMinutes);
      if (date >= effectiveMin) {
        onChange(toLocalISO(date));
      }
    }
  }

  const daysInMonth = getDaysInMonth(viewYear, viewMonth);
  let firstDayOffset = new Date(viewYear, viewMonth, 1).getDay();
  firstDayOffset = firstDayOffset === 0 ? 6 : firstDayOffset - 1;

  const calendarCells = [];
  const prevMonthDays = getDaysInMonth(viewMonth === 0 ? viewYear - 1 : viewYear, viewMonth === 0 ? 11 : viewMonth - 1);
  for (let i = firstDayOffset - 1; i >= 0; i--) {
    calendarCells.push({ day: prevMonthDays - i, current: false, disabled: true });
  }
  for (let d = 1; d <= daysInMonth; d++) {
    const cellDate = new Date(viewYear, viewMonth, d, hours, minutes);
    calendarCells.push({
      day: d,
      current: true,
      disabled: cellDate < effectiveMin && !isSameDay(cellDate, effectiveMin),
      selected: selectedDate && isSameDay(cellDate, selectedDate),
      isToday: isSameDay(cellDate, now),
    });
  }
  const remaining = 42 - calendarCells.length;
  for (let i = 1; i <= remaining; i++) {
    calendarCells.push({ day: i, current: false, disabled: true });
  }

  const monthLabel = `${viewYear} 年 ${viewMonth + 1} 月`;

  return (
    <div className="datepicker-popup">
      <div className="datepicker-nav">
        <button type="button" className="datepicker-nav-btn" onClick={prevMonth}>&#8249;</button>
        <span className="datepicker-nav-label">{monthLabel}</span>
        <button type="button" className="datepicker-nav-btn" onClick={nextMonth}>&#8250;</button>
      </div>
      <div className="datepicker-weekdays">
        {WEEKDAYS.map((w) => <span key={w} className="datepicker-weekday">{w}</span>)}
      </div>
      <div className="datepicker-grid">
        {calendarCells.map((cell, idx) => (
          <button
            key={idx}
            type="button"
            className={[
              'datepicker-day',
              !cell.current && 'datepicker-day-other',
              cell.selected && 'datepicker-day-selected',
              cell.isToday && 'datepicker-day-today',
              cell.disabled && 'datepicker-day-disabled',
            ].filter(Boolean).join(' ')}
            onClick={() => !cell.disabled && handleDayClick(cell.day)}
            disabled={cell.disabled}
          >
            {cell.day}
          </button>
        ))}
      </div>
      <div className="datepicker-time">
        <span className="datepicker-time-label">时间</span>
        <div className="datepicker-time-inputs">
          <input
            type="number"
            className="datepicker-time-input"
            min={0}
            max={23}
            value={hours}
            onChange={(e) => {
              const v = Math.max(0, Math.min(23, Number(e.target.value) || 0));
              handleTimeChange(v, minutes);
            }}
          />
          <span className="datepicker-time-sep">:</span>
          <input
            type="number"
            className="datepicker-time-input"
            min={0}
            max={59}
            value={minutes}
            onChange={(e) => {
              const v = Math.max(0, Math.min(59, Number(e.target.value) || 0));
              handleTimeChange(hours, v);
            }}
          />
        </div>
      </div>
    </div>
  );
}
