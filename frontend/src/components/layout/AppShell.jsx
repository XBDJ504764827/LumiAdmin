import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '../../state/store.js';
import { ThemeToggle } from '../../shared/ThemeToggle.jsx';
import { PendingReviewProvider, usePendingReviewData } from '../../hooks/usePendingReviewIndicators.js';
import { sidebarSections } from './sidebarSections.jsx';

function hasActiveChild(children, pathname) {
  return children.some((child) => child.path === pathname || (child.children && hasActiveChild(child.children, pathname)));
}

function flattenNavItems(sections) {
  return sections.flatMap((section) => section.items.flatMap((item) => {
    if (!item.children) {
      return [{ ...item, sectionLabel: section.label }];
    }

    return item.children.map((child) => ({
      ...child,
      sectionLabel: section.label,
      parentLabel: item.label,
    }));
  }));
}

export function AppShell({ children }) {
  const [collapsed, setCollapsed] = useState(false);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [searchOpen, setSearchOpen] = useState(false);
  const [activeSearchIndex, setActiveSearchIndex] = useState(0);
  const [profileOpen, setProfileOpen] = useState(false);
  const searchRef = useRef(null);
  const searchInputRef = useRef(null);
  const profileRef = useRef(null);
  const { session, logout } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const sections = useMemo(() => sidebarSections(session?.role ?? 'guest'), [session?.role]);
  const searchItems = useMemo(() => flattenNavItems(sections), [sections]);
  const pendingReviews = usePendingReviewData();
  const pendingCounts = pendingReviews.counts;
  const avatar = (session?.displayName ?? 'AL').slice(0, 2).toUpperCase();
  const canOpenUserManagement = searchItems.some((item) => item.path === '/users');
  const normalizedSearch = searchTerm.trim().toLowerCase();
  const filteredSearchItems = useMemo(() => {
    const candidates = normalizedSearch
      ? searchItems.filter((item) => {
        const haystack = `${item.label} ${item.parentLabel ?? ''} ${item.sectionLabel} ${item.path}`.toLowerCase();
        return haystack.includes(normalizedSearch);
      })
      : searchItems;

    return candidates.slice(0, 8);
  }, [normalizedSearch, searchItems]);
  const activeSearchItem = filteredSearchItems[activeSearchIndex];
  const activeSearchOptionId = activeSearchItem ? `app-search-option-${activeSearchItem.path.replace(/[^a-zA-Z0-9_-]/g, '-')}` : undefined;

  // 跟踪展开的父级菜单（用 label 作为 key）
  const [expanded, setExpanded] = useState(() => {
    // 初始化时自动展开包含当前路径的父级
    const initial = new Set();
    sections.forEach((section) => {
      section.items.forEach((item) => {
        if (item.children && hasActiveChild(item.children, location.pathname)) {
          initial.add(item.label);
        }
      });
    });
    return initial;
  });

  useEffect(() => {
    function handleDocumentMouseDown(event) {
      if (searchRef.current && !searchRef.current.contains(event.target)) {
        setSearchOpen(false);
      }
      if (profileRef.current && !profileRef.current.contains(event.target)) {
        setProfileOpen(false);
      }
    }

    function handleGlobalShortcut(event) {
      const isSearchShortcut = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k';
      if (isSearchShortcut) {
        event.preventDefault();
        setSearchOpen(true);
        setActiveSearchIndex(0);
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }

      if (event.key === 'Escape') {
        setSearchOpen(false);
        setProfileOpen(false);
        setMobileNavOpen(false);
        searchInputRef.current?.blur();
      }
    }

    document.addEventListener('mousedown', handleDocumentMouseDown);
    window.addEventListener('keydown', handleGlobalShortcut);
    return () => {
      document.removeEventListener('mousedown', handleDocumentMouseDown);
      window.removeEventListener('keydown', handleGlobalShortcut);
    };
  }, []);

  const toggleExpand = useCallback((label) => {
    if (collapsed) {
      setCollapsed(false);
      // 延迟展开子菜单，等待侧边栏动画完成
      setTimeout(() => {
        setExpanded((prev) => {
          const next = new Set(prev);
          next.add(label);
          return next;
        });
      }, 150);
      return;
    }
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(label)) {
        next.delete(label);
      } else {
        next.add(label);
      }
      return next;
    });
  }, [collapsed]);

  const handleSearchNavigate = useCallback((path) => {
    navigate(path);
    setSearchOpen(false);
    setSearchTerm('');
    setMobileNavOpen(false);
  }, [navigate]);

  function handleSearchKeyDown(event) {
    if (event.key === 'ArrowDown' && filteredSearchItems.length > 0) {
      event.preventDefault();
      setSearchOpen(true);
      setActiveSearchIndex((index) => (index + 1) % filteredSearchItems.length);
      return;
    }

    if (event.key === 'ArrowUp' && filteredSearchItems.length > 0) {
      event.preventDefault();
      setSearchOpen(true);
      setActiveSearchIndex((index) => (index - 1 + filteredSearchItems.length) % filteredSearchItems.length);
      return;
    }

    if (event.key === 'Enter' && activeSearchItem) {
      event.preventDefault();
      handleSearchNavigate(activeSearchItem.path);
    }

    if (event.key === 'Escape') {
      setSearchOpen(false);
      event.currentTarget.blur();
    }
  }

  // 渲染单个导航项
  function renderNavItem(item, isSubItem = false) {
    const isActive = location.pathname === item.path;
    const pendingCount = item.pendingKey ? pendingCounts[item.pendingKey] ?? 0 : 0;
    const hasPending = pendingCount > 0;
    return (
      <button
        type="button"
        key={item.path}
        className={`nav-item ${isSubItem ? 'nav-sub-item' : ''} ${isActive ? 'active' : ''}`}
        onClick={() => {
          navigate(item.path);
          setMobileNavOpen(false);
        }}
        title={collapsed ? item.label : undefined}
        aria-current={isActive ? 'page' : undefined}
      >
        <span className="nav-icon">{item.icon}</span>
        <span className="nav-text-wrap">
          <span className="nav-text">{item.label}</span>
          {hasPending ? (
            <span
              className="nav-pending-dot"
              aria-label={`${item.label}有待审核内容`}
              title={`${item.label}有 ${pendingCount} 条待审核`}
            />
          ) : null}
        </span>
      </button>
    );
  }

  // 渲染可折叠的父级菜单
  function renderCollapsibleItem(item) {
    const isChildActive = item.children && hasActiveChild(item.children, location.pathname);
    const isExpanded = expanded.has(item.label) || isChildActive;

    return (
      <div key={item.label} className="nav-collapsible-group">
        <button
          type="button"
          className={`nav-item nav-parent-item ${isChildActive ? 'child-active' : ''}`}
          onClick={() => toggleExpand(item.label)}
          title={collapsed ? item.label : undefined}
          aria-expanded={isExpanded}
        >
          <span className="nav-icon">{item.icon}</span>
          <span className="nav-text">{item.label}</span>
          <span className={`nav-chevron ${isExpanded ? 'expanded' : ''}`}>
            <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d="M2.5 3.5L5 6L7.5 3.5" />
            </svg>
          </span>
        </button>
        {isExpanded && (
          <div className="nav-sub-menu">
            {item.children.map((child) => renderNavItem(child, true))}
          </div>
        )}
      </div>
    );
  }

  return (
    <PendingReviewProvider value={pendingReviews}>
      <div className={`app-shell ${collapsed ? 'sidebar-collapsed' : ''} ${mobileNavOpen ? 'mobile-nav-open' : ''}`}>
      <button
        type="button"
        className="mobile-nav-scrim"
        aria-label="关闭导航"
        onClick={() => setMobileNavOpen(false)}
      />
      <aside className={`sidebar ${collapsed ? 'collapsed' : ''}`} id="sidebar">
        <div className="sidebar-logo">
          <div className="logo-mark">
            <svg viewBox="0 0 24 24" fill="none">
              <path d="M12 3l8 5v8l-8 5-8-5V8l8-5z" stroke="white" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              <path d="M12 8v8M8 10v4M16 10v4" stroke="white" strokeWidth="1.4" strokeLinecap="round" opacity="0.7" />
            </svg>
          </div>
          {!collapsed && <div className="logo-text">Lumi<span>Admin</span></div>}
        </div>
        <button
          type="button"
          className="sidebar-collapse-btn"
          onClick={() => setCollapsed((v) => !v)}
          title={collapsed ? '展开侧边栏' : '折叠侧边栏'}
        >
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor"><path d="M6.5 2L3.5 5L6.5 8" strokeLinecap="round" strokeLinejoin="round" /></svg>
        </button>

        <nav className="nav-section">
          {sections.map((section) => (
            <div key={section.label} className="nav-group-block">
              <div className="nav-label">{section.label}</div>
              {section.items.map((item) => {
                if (item.children) {
                  return renderCollapsibleItem(item);
                }
                return renderNavItem(item);
              })}
            </div>
          ))}
        </nav>

        <div className="sidebar-footer">
          <div className="user-card">
            <div className="avatar avatar-accent" title={session?.displayName}>{avatar}</div>
            <div className="user-info">
              <div className="user-name">{session?.displayName ?? 'Alex 管理员'}</div>
              <div className="user-role">{session?.roleLabel ?? '系统管理员'}</div>
            </div>
            <button className="logout-btn" type="button" onClick={logout} title="退出登录">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="logout-icon">
                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                <polyline points="16 17 21 12 16 7" />
                <line x1="21" y1="12" x2="9" y2="12" />
              </svg>
              <span className="logout-text">退出</span>
            </button>
          </div>
        </div>
      </aside>

      <div className="main" id="main">
        <header className="topbar">
          <button
            className="icon-btn mobile-menu-btn"
            type="button"
            onClick={() => {
              setCollapsed(false);
              setMobileNavOpen(true);
            }}
            aria-label="打开导航"
            aria-expanded={mobileNavOpen}
            aria-controls="sidebar"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="3" y1="6" x2="21" y2="6" />
              <line x1="3" y1="12" x2="21" y2="12" />
              <line x1="3" y1="18" x2="21" y2="18" />
            </svg>
          </button>
          <div className={`search-box app-search ${searchOpen ? 'is-open' : ''}`} ref={searchRef}>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" /></svg>
            <input
              ref={searchInputRef}
              placeholder="搜索页面或功能..."
              value={searchTerm}
              onChange={(event) => {
                setSearchTerm(event.target.value);
                setSearchOpen(true);
                setActiveSearchIndex(0);
              }}
              onFocus={() => {
                setSearchOpen(true);
                setActiveSearchIndex(0);
              }}
              onKeyDown={handleSearchKeyDown}
              aria-label="全局搜索"
              aria-expanded={searchOpen}
              aria-controls="app-search-results"
              aria-activedescendant={activeSearchOptionId}
            />
            <span className="search-shortcut">⌘ K</span>
            {searchOpen ? (
              <div className="app-search-panel" id="app-search-results" role="listbox">
                <div className="app-search-panel-label">
                  <span>{normalizedSearch ? '搜索结果' : '快捷入口'}</span>
                  {filteredSearchItems.length > 0 ? <span>{filteredSearchItems.length} 项</span> : null}
                </div>
                {filteredSearchItems.length > 0 ? (
                  filteredSearchItems.map((item, index) => (
                    <button
                      id={`app-search-option-${item.path.replace(/[^a-zA-Z0-9_-]/g, '-')}`}
                      key={item.path}
                      type="button"
                      role="option"
                      aria-selected={index === activeSearchIndex}
                      className={`app-search-result ${location.pathname === item.path ? 'is-current' : ''} ${index === activeSearchIndex ? 'is-active' : ''}`}
                      onClick={() => handleSearchNavigate(item.path)}
                      onMouseEnter={() => setActiveSearchIndex(index)}
                    >
                      <span className="app-search-result-icon">{item.icon}</span>
                      <span className="app-search-result-main">
                        <span className="app-search-result-title">{item.label}</span>
                        <span className="app-search-result-sub">
                          {[item.sectionLabel, item.parentLabel].filter(Boolean).join(' / ') || item.path}
                        </span>
                      </span>
                      {index === activeSearchIndex ? <span className="app-search-result-key">Enter</span> : null}
                    </button>
                  ))
                ) : (
                  <div className="app-search-empty">没有匹配的页面</div>
                )}
              </div>
            ) : null}
          </div>
          <div className="topbar-actions">
            <ThemeToggle compact />
            <button className="icon-btn" type="button" onClick={() => navigate('/public/apply')} title="公开页面">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <polyline points="15 3 21 3 21 9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
            </button>
            <div className="topbar-divider" />
            <div className="topbar-profile" ref={profileRef}>
              <button
                className={`topbar-avatar ${profileOpen ? 'is-open' : ''}`}
                type="button"
                onClick={() => setProfileOpen((open) => !open)}
                title={session?.displayName ?? '账号菜单'}
                aria-label="账号菜单"
                aria-expanded={profileOpen}
              >
                {avatar}
              </button>
              {profileOpen ? (
                <div className="topbar-profile-menu">
                  <div className="topbar-profile-head">
                    <div className="avatar avatar-accent">{avatar}</div>
                    <div className="topbar-profile-meta">
                      <div className="topbar-profile-name">{session?.displayName ?? '管理员'}</div>
                      <div className="topbar-profile-role">{session?.roleLabel ?? session?.role ?? '管理账号'}</div>
                    </div>
                  </div>
                  {canOpenUserManagement ? (
                    <button
                      type="button"
                      className="topbar-profile-action"
                      onClick={() => {
                        setProfileOpen(false);
                        navigate('/users');
                      }}
                    >
                      <span>账号管理</span>
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="topbar-profile-action topbar-profile-action-danger"
                    onClick={() => {
                      setProfileOpen(false);
                      logout();
                    }}
                  >
                    <span>退出登录</span>
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        </header>
        {children}
      </div>
      </div>
    </PendingReviewProvider>
  );
}
