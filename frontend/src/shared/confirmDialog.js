export function createConfirmOptions(options) {
  return {
    title: options.title ?? '确认操作',
    message: options.message,
    confirmText: options.confirmText ?? '确认删除',
    cancelText: options.cancelText ?? '取消',
    tone: options.tone ?? 'danger',
  };
}

export function createAlertOptions(options) {
  return {
    title: options.title ?? '提示',
    message: options.message,
    confirmText: options.confirmText ?? '知道了',
    tone: options.tone ?? 'info',
  };
}
