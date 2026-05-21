export function normalizeAdminPreviewRows(items = []) {
  return items.map((item) => {
    const displayName = item.display_name ?? '';

    return {
      displayName,
      role: item.role,
      roleLabel: item.role_label,
      status: item.status,
      initials: displayName.trim().charAt(0).toUpperCase() || '?',
    };
  });
}
