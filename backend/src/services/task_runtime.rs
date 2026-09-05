//! 后台任务运行时工具。
//!
//! 所有后台循环任务统一通过 [`spawn_persistent`] 启动，获得两层保障：
//! - panic 隔离：单次任务体 panic 不会让整个循环静默消失，记录日志后延迟重启；
//! - 正常退出：任务体正常返回（如优雅关闭流程）后不再重启。

use std::future::Future;
use std::time::Duration;

/// 提取 panic payload 中的人类可读信息
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// 启动持久后台任务。
///
/// `factory` 每次被调用都应构造一个全新的任务体（内部自行 clone 所需依赖）。
/// 任务体 panic 时记录错误日志并延迟 5 秒重启；正常返回则停止。
pub fn spawn_persistent<F, Fut>(name: &'static str, factory: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(factory()).await {
                Ok(()) => break,
                Err(join_error) if join_error.is_panic() => {
                    let message = panic_message(join_error.into_panic().as_ref());
                    tracing::error!(
                        task = name,
                        panic = %message,
                        "后台任务发生 panic，5 秒后自动重启"
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(_) => break,
            }
        }
    });
}