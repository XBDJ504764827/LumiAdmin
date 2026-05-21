export function buildDeleteGroupConfirmMessage(group) {
  const serverCount = group?.servers?.length ?? 0;

  if (serverCount === 0) {
    return `确定删除社区组“${group.name}”吗？`;
  }

  return `确定删除社区组“${group.name}”吗？删除后将同时删除其下 ${serverCount} 个服务器。`;
}

export function buildDeleteGroupSuccessMessage(groupName) {
  return `社区组“${groupName}”删除成功。`;
}

export function buildDeleteGroupFailureMessage(errorMessage) {
  return `社区组删除失败：${errorMessage}`;
}
