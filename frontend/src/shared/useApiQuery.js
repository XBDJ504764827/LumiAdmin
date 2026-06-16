import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useAuth } from '../state/store.js';

/**
 * 封装 React Query 的 Hook，自动注入 token
 * @param {Array} queryKey - 查询键
 * @param {Function} queryFn - 查询函数，接收 token 参数
 * @param {Object} options - React Query 选项
 */
export function useApiQuery(queryKey, queryFn, options = {}) {
  const { session } = useAuth();
  const token = session?.token ?? null;

  return useQuery({
    queryKey: [...queryKey, token],
    queryFn: () => queryFn(token),
    enabled: !!token && (options.enabled !== false),
    ...options,
  });
}

/**
 * 封装 React Query 的 Mutation Hook
 * @param {Function} mutationFn - 变更函数，接收 { token, ...params } 参数
 * @param {Object} options - React Query 选项
 */
export function useApiMutation(mutationFn, options = {}) {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params) => mutationFn({ token, ...params }),
    onSuccess: (data, variables, context) => {
      if (options.invalidateQueries) {
        const queries = Array.isArray(options.invalidateQueries) 
          ? options.invalidateQueries 
          : [options.invalidateQueries];
        queries.forEach(queryKey => {
          queryClient.invalidateQueries({ queryKey: [queryKey] });
        });
      }
      if (options.onSuccess) {
        options.onSuccess(data, variables, context);
      }
    },
    ...options,
  });
}

/**
 * 创建一个带有预配置选项的查询 Hook 工厂
 * @param {Array} baseQueryKey - 基础查询键
 * @param {Function} queryFn - 查询函数
 */
export function createApiQueryHook(baseQueryKey, queryFn) {
  return function useQueryHook(options = {}) {
    return useApiQuery(baseQueryKey, queryFn, options);
  };
}

/**
 * 创建一个带有预配置选项的变更 Hook 工厂
 * @param {Function} mutationFn - 变更函数
 * @param {Array|Function} invalidateQueries - 需要失效的查询键
 */
export function createApiMutationHook(mutationFn, invalidateQueries) {
  return function useMutationHook(options = {}) {
    return useApiMutation(mutationFn, {
      invalidateQueries,
      ...options,
    });
  };
}
