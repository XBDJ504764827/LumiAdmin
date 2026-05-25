import React, { useCallback, useEffect, useRef, useState } from 'react';

/**
 * 无限滚动列表组件
 *
 * 用于大数据量列表的优化加载，支持：
 * - 滚动到底部自动加载更多
 * - 加载状态显示
 * - 错误重试
 */
export function InfiniteList({
  loadMore,
  renderItem,
  pageSize = 20,
  initialData = null,
  threshold = 200, // 距离底部多少像素时触发加载
}) {
  const [items, setItems] = useState([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [hasMore, setHasMore] = useState(true);
  const containerRef = useRef(null);
  const loadingRef = useRef(false);

  // 初始加载
  useEffect(() => {
    if (initialData) {
      setItems(initialData.items || []);
      setTotal(initialData.total || 0);
      setHasMore((initialData.items?.length || 0) < (initialData.total || 0));
    } else {
      loadData(1);
    }
  }, []);

  const loadData = useCallback(async (pageNum) => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    setLoading(true);
    setError(null);

    try {
      const result = await loadMore({ page: pageNum, page_size: pageSize });
      const newItems = result.items || [];

      if (pageNum === 1) {
        setItems(newItems);
      } else {
        setItems((prev) => [...prev, ...newItems]);
      }

      setTotal(result.total || 0);
      setPage(pageNum);
      setHasMore(newItems.length >= pageSize);
    } catch (err) {
      setError(err.message || '加载失败');
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, [loadMore, pageSize]);

  // 滚动检测
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !hasMore) return;

    const handleScroll = () => {
      if (loading || error) return;

      const { scrollTop, scrollHeight, clientHeight } = container;
      const distanceToBottom = scrollHeight - scrollTop - clientHeight;

      if (distanceToBottom < threshold) {
        loadData(page + 1);
      }
    };

    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, [loading, error, hasMore, page, loadData, threshold]);

  const retry = useCallback(() => {
    loadData(page);
  }, [page, loadData]);

  return {
    items,
    total,
    loading,
    error,
    hasMore,
    containerRef,
    retry,
    loadMore: () => loadData(page + 1),
    // 渲染辅助
    renderStatus: () => (
      <>
        {loading && <div className="infinite-loading">加载中...</div>}
        {error && (
          <div className="infinite-error">
            <span>{error}</span>
            <button onClick={retry}>重试</button>
          </div>
        )}
        {!hasMore && items.length > 0 && (
          <div className="infinite-end">已加载全部 {total} 条数据</div>
        )}
      </>
    ),
  };
}

/**
 * 使用 Intersection Observer 的无限滚动 Hook
 * 性能更好，不需要监听 scroll 事件
 */
export function useInfiniteScroll({
  loadMore,
  pageSize = 20,
  threshold = 0.1, // Intersection Observer threshold
}) {
  const [items, setItems] = useState([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [hasMore, setHasMore] = useState(true);
  const sentinelRef = useRef(null);
  const loadingRef = useRef(false);

  const loadData = useCallback(async (pageNum) => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    setLoading(true);
    setError(null);

    try {
      const result = await loadMore({ page: pageNum, page_size: pageSize });
      const newItems = result.items || [];

      if (pageNum === 1) {
        setItems(newItems);
      } else {
        setItems((prev) => [...prev, ...newItems]);
      }

      setTotal(result.total || 0);
      setPage(pageNum);
      setHasMore(newItems.length >= pageSize);
    } catch (err) {
      setError(err.message || '加载失败');
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, [loadMore, pageSize]);

  // 初始加载
  useEffect(() => {
    loadData(1);
  }, []);

  // Intersection Observer
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting && !loading && !error) {
          loadData(page + 1);
        }
      },
      { threshold }
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [loading, error, hasMore, page, loadData, threshold]);

  const retry = useCallback(() => {
    loadData(page);
  }, [page, loadData]);

  const refresh = useCallback(() => {
    setItems([]);
    setPage(1);
    setHasMore(true);
    loadData(1);
  }, [loadData]);

  return {
    items,
    total,
    loading,
    error,
    hasMore,
    sentinelRef,
    retry,
    refresh,
    loadMore: () => loadData(page + 1),
  };
}