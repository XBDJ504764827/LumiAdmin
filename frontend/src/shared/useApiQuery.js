import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useAuth } from '../state/store.js';

/**
 * 封装 React Query 的 Hook，自动注入 token
 * @param {Array} queryKey - 查询键
 * @param {Function} queryFn - 查询函数，认证接口接收 token 参数；公开接口不接收参数
 * @param {Object} options - React Query 选项，auth=false 时不要求登录态
 */
export function useApiQuery(queryKey, queryFn, options = {}) {
  const { session } = useAuth();
  const token = session?.token ?? null;
  const { auth = true, enabled = true, ...queryOptions } = options;
  const requiresAuth = auth !== false;

  return useQuery({
    ...queryOptions,
    queryKey: requiresAuth ? [...queryKey, token] : queryKey,
    queryFn: () => (requiresAuth ? queryFn(token) : queryFn()),
    enabled: (requiresAuth ? !!token : true) && enabled,
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

  const { invalidateQueries, onSuccess: callerOnSuccess, ...mutationOptions } = options;

  return useMutation({
    ...mutationOptions,
    mutationFn: (params) => mutationFn({ token, ...params }),
    onSuccess: async (data, variables, context) => {
      if (invalidateQueries) {
        const queries = Array.isArray(invalidateQueries) ? invalidateQueries : [invalidateQueries];
        await Promise.all(queries.map((queryKey) => (
          queryClient.invalidateQueries({ queryKey: Array.isArray(queryKey) ? queryKey : [queryKey] })
        )));
      }
      await callerOnSuccess?.(data, variables, context);
    },
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
