use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct EndpointDoc {
    pub module: &'static str,
    pub tone: &'static str,
    pub name: &'static str,
    pub method: &'static str,
    pub endpoint: &'static str,
    pub description: &'static str,
    pub auth_required: bool,
    pub roles: &'static [&'static str],
}

pub fn list_endpoints() -> Vec<EndpointDoc> {
    vec![
        EndpointDoc { module: "认证", tone: "info", name: "管理员登录", method: "POST", endpoint: "/api/auth/login", description: "网站管理员使用用户名和密码登录", auth_required: false, roles: &["guest"] },
        EndpointDoc { module: "认证", tone: "info", name: "获取当前会话", method: "GET", endpoint: "/api/auth/me", description: "读取当前登录会话信息", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "仪表盘", tone: "info", name: "获取仪表盘数据", method: "GET", endpoint: "/api/dashboard", description: "获取服务器、社区、在线玩家和管理员统计", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "获取服务器列表", method: "GET", endpoint: "/api/community/servers", description: "获取当前所有社区组及下属服务器配置", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "创建社区组", method: "POST", endpoint: "/api/community/groups", description: "创建新的社区分组", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "删除社区组", method: "DELETE", endpoint: "/api/community/groups/:group_id", description: "删除社区组并级联删除下属服务器", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "添加社区服务器", method: "POST", endpoint: "/api/community/groups/:group_id/servers", description: "在指定社区组下添加服务器", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "测试 RCON 连接", method: "POST", endpoint: "/api/community/servers/test-rcon", description: "在保存服务器前验证 RCON 密码和端口的连通性", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "更新社区服务器", method: "PUT", endpoint: "/api/community/servers/:server_id", description: "修改指定服务器的连接配置", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "删除社区服务器", method: "DELETE", endpoint: "/api/community/servers/:server_id", description: "删除指定服务器配置", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "获取在线玩家", method: "GET", endpoint: "/api/community/servers/:server_id/players", description: "获取指定服务器由插件上报的在线玩家详情", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "社区配置", tone: "info", name: "查看服务器上报 Token", method: "GET", endpoint: "/api/community/servers/:server_id/report-token", description: "查看指定服务器插件上报使用的 Token", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "社区配置", tone: "warning", name: "重置服务器上报 Token", method: "POST", endpoint: "/api/community/servers/:server_id/report-token/reset", description: "重置指定服务器插件上报 Token，旧 Token 会立即失效", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "插件上报", tone: "success", name: "上报在线玩家", method: "POST", endpoint: "/api/plugin/online-players/report", description: "CS:GO 插件使用服务器 Token 和端口上报当前在线玩家详情", auth_required: false, roles: &[] },
        EndpointDoc { module: "插件准入", tone: "warning", name: "插件进服准入校验", method: "POST", endpoint: "/api/plugin/access/check", description: "游戏服务器插件在玩家进服时校验封禁、白名单和进入限制", auth_required: false, roles: &["game-server"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "后台白名单列表", method: "GET", endpoint: "/api/whitelist", description: "获取后台白名单审核列表", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "手动添加白名单", method: "POST", endpoint: "/api/whitelist/manual", description: "管理员手动创建已通过的白名单记录", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "通过白名单申请", method: "POST", endpoint: "/api/whitelist/:id/approve", description: "管理员通过玩家白名单申请", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "拒绝白名单申请", method: "POST", endpoint: "/api/whitelist/:id/reject", description: "管理员拒绝玩家白名单申请", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "恢复白名单通过", method: "POST", endpoint: "/api/whitelist/:id/restore", description: "将被拒绝的白名单申请恢复为通过", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "白名单管理", tone: "online", name: "删除白名单审核", method: "POST", endpoint: "/api/whitelist/:id/revoke", description: "撤销已通过白名单记录", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "封禁管理", tone: "danger", name: "封禁列表", method: "GET", endpoint: "/api/bans", description: "获取后台封禁记录列表", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "封禁管理", tone: "danger", name: "新增玩家封禁", method: "POST", endpoint: "/api/bans", description: "向后台写入账号或 IP 封禁记录", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "封禁管理", tone: "danger", name: "解除玩家封禁", method: "POST", endpoint: "/api/bans/:id/unban", description: "解除处于封禁中状态的账号或 IP 限制", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "插件封禁", tone: "danger", name: "插件创建封禁", method: "POST", endpoint: "/api/plugin/bans", description: "游戏服务器插件上传 Steam 或 IP 封禁记录", auth_required: false, roles: &["game-server"] },
        EndpointDoc { module: "插件封禁", tone: "danger", name: "插件解除封禁", method: "POST", endpoint: "/api/plugin/bans/unban", description: "游戏服务器插件按 SteamID 或 IP 解除有效封禁", auth_required: false, roles: &["game-server"] },
        EndpointDoc { module: "插件封禁", tone: "danger", name: "插件封禁校验", method: "POST", endpoint: "/api/plugin/bans/check", description: "游戏服务器插件在玩家进服时校验封禁状态", auth_required: false, roles: &["game-server"] },
        EndpointDoc { module: "网站用户", tone: "warning", name: "网站用户列表", method: "GET", endpoint: "/api/users", description: "获取当前可管理的网站管理员账号", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "网站用户", tone: "warning", name: "创建网站用户", method: "POST", endpoint: "/api/users", description: "创建新的后台管理员账号", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "网站用户", tone: "warning", name: "更新网站用户", method: "PUT", endpoint: "/api/users/:id", description: "修改指定后台管理员账号", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "网站用户", tone: "warning", name: "删除网站用户", method: "DELETE", endpoint: "/api/users/:id", description: "删除指定后台管理员账号", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "网站用户", tone: "warning", name: "修改网站用户密码", method: "PUT", endpoint: "/api/users/:id/password", description: "修改指定后台管理员账号密码", auth_required: true, roles: &["admin", "developer", "normal"] },
        EndpointDoc { module: "操作日志", tone: "info", name: "操作日志列表", method: "GET", endpoint: "/api/logs", description: "记录网站管理员的关键操作追踪", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "API 文档", tone: "info", name: "接口元数据列表", method: "GET", endpoint: "/api/docs/endpoints", description: "获取后台 API 接口列表页使用的接口元数据", auth_required: true, roles: &["admin", "developer"] },
        EndpointDoc { module: "公共展示", tone: "online", name: "白名单公示", method: "GET", endpoint: "/api/public/whitelist", description: "公共页面获取已通过白名单记录", auth_required: false, roles: &["guest"] },
        EndpointDoc { module: "公共展示", tone: "online", name: "提交白名单申请", method: "POST", endpoint: "/api/public/whitelist", description: "公共页面接口：玩家提交 Steam 标识符及游戏昵称", auth_required: false, roles: &["guest"] },
        EndpointDoc { module: "公共展示", tone: "danger", name: "封禁公示", method: "GET", endpoint: "/api/public/bans", description: "公共页面获取封禁公示记录", auth_required: false, roles: &["guest"] },
    ]
}

#[cfg(test)]
mod tests {
    use super::list_endpoints;

    #[test]
    fn list_endpoints_includes_docs_and_ban_endpoints() {
        let endpoints = list_endpoints();

        assert!(endpoints.iter().any(|item| item.endpoint == "/api/docs/endpoints"));
        assert!(endpoints.iter().any(|item| item.endpoint == "/api/bans" && item.method == "POST"));
        assert!(endpoints.iter().any(|item| item.endpoint == "/api/plugin/bans" && item.method == "POST"));
        assert!(endpoints.iter().any(|item| item.endpoint == "/api/plugin/bans/unban" && item.method == "POST"));
        assert!(endpoints.iter().any(|item| item.endpoint == "/api/plugin/bans/check" && item.method == "POST"));
    }

    #[test]
    fn endpoint_docs_include_plugin_access_check() {
        let endpoints = list_endpoints();
        assert!(endpoints.iter().any(|item| {
            item.endpoint == "/api/plugin/access/check"
                && item.method == "POST"
                && item.module == "插件准入"
                && item.roles == ["game-server"]
        }));
    }
}