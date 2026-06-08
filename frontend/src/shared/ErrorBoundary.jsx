import React from 'react';

/**
 * 错误边界组件
 * 捕获子组件树中的 JavaScript 错误，防止整个应用白屏
 */
export class ErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  componentDidCatch(error, errorInfo) {
    console.error('[ErrorBoundary]', error, errorInfo);
  }

  handleRetry() {
    this.setState({ hasError: false, error: null });
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, () => this.handleRetry());
      }

      return (
        <div style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          minHeight: 300,
          padding: 40,
          textAlign: 'center',
        }}>
          <div style={{
            width: 48, height: 48, borderRadius: 12,
            background: 'var(--danger-bg, rgba(220,38,38,0.1))',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            marginBottom: 16,
          }}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="var(--danger, #dc2626)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
          </div>
          <div style={{ fontSize: 16, fontWeight: 600, color: 'var(--text, #111)', marginBottom: 8 }}>
            页面加载出错
          </div>
          <div style={{ fontSize: 13, color: 'var(--text2, #666)', maxWidth: 400, marginBottom: 20 }}>
            此页面遇到了一个意外错误。请尝试刷新，如果问题持续请联机管理员。
          </div>
          <button
            onClick={() => this.handleRetry()}
            style={{
              padding: '8px 20px',
              border: '1px solid var(--border, #ddd)',
              borderRadius: 8,
              background: 'var(--surface, #fff)',
              color: 'var(--text, #111)',
              fontSize: 13,
              cursor: 'pointer',
              transition: 'all 0.15s',
            }}
          >
            重试
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
