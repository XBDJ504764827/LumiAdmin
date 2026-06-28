import React from 'react';
import { PageState } from './PageState.jsx';

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
        <PageState
          tone="danger"
          title="页面加载出错"
          message="此页面遇到了一个意外错误。请尝试刷新，如果问题持续请联系管理员。"
          action={() => this.handleRetry()}
        />
      );
    }

    return this.props.children;
  }
}
