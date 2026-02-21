# 生命周期管理修复记录

## 问题描述

在 U（更新）、S（安装）、R（卸载）页面执行操作期间按 Esc 返回首页后，
`/var/lib/pacman/db.lck` 锁文件不会被清理，导致后续任何 pacman 操作均报错：

```
error: could not lock database: File exists
```

根因是多个相互叠加的缺陷共同导致这一现象，以下逐一说明。

---

## Bug 1 — P0 根本原因：`try_remove_db_lock` 无法删除 root 拥有的文件

**文件**：`src/package_manager/streaming.rs`

### 旧代码
```rust
let _ = std::fs::remove_file(path);
```

### 问题
`/var/lib/pacman/` 目录权限为 `drwxr-xr-x root root`，普通用户没有写权限。
`remove_file` 以 `EACCES` 静默失败，`let _` 直接丢弃错误——
这是"清理函数运行了但什么都没发生"的直接原因。

### 修复
改用 `sudo -n rm -f`（`-n` 为 non-interactive，凭证已缓存时成功，过期时静默失败不阻塞 TUI）：
```rust
let status = std::process::Command::new("sudo")
    .args(["-n", "rm", "-f", lock_path])
    .status();
if let Ok(s) = status {
    if !s.success() {
        log::warn!("try_remove_db_lock: sudo rm -f {} 失败（exit={:?}）", lock_path, s.code());
    }
}
```

---

## Bug 2 — P1 竞争条件：`reset_cancel()` 在工作线程内覆盖用户的取消请求

**文件**：`src/package_manager/streaming.rs`，`src/tui/update.rs`，  
`src/tui/install.rs`，`src/tui/remove.rs`，`src/tui/shell.rs`，  
`src/package_manager/mod.rs`

### 旧行为时序
```
UI线程:    spawn_update_task() → std::thread::spawn(worker)
                                    worker: get_explicit_packages()  ← 耗时阻塞
用户: Esc → cancel_update() → SHOULD_CANCEL = true（PID此时为0，SIGTERM未发）
                                    worker: run_streaming_command()
                                              └─ reset_cancel() → SHOULD_CANCEL = false ← 覆盖！
                                              └─ 进程正常启动，Esc 被彻底忽略
```

因为 `reset_cancel()` 曾在 `run_streaming_command()` 内部调用，
工作线程启动后的重置会覆盖 UI 线程已设置的取消标志。

### 修复
将 `reset_cancel()` 的调用点**移到 UI 线程**，在 `std::thread::spawn` 之前执行，
并从 `run_streaming_command` / `run_custom_command_streaming` 内部移除该调用。
同时从 `package_manager/mod.rs` 导出 `reset_cancel`：

```rust
// mod.rs
pub use streaming::reset_cancel;

// spawn_update_task / spawn_install_task / spawn_remove_task / spawn_shell_task
crate::package_manager::reset_cancel();  // UI 线程调用
std::thread::spawn(move || { ... });
```

---

## Bug 3 — P1 竞争条件：`CHILD_PID = 0` 时 `cancel_update` 不发信号

**文件**：`src/package_manager/streaming.rs`

### 旧行为
`cancel_update()` 仅在 `CHILD_PID != 0` 时才发 SIGTERM。
但从 `reset_cancel()` → `Command::spawn()` → `CHILD_PID.store(pid)` 三步之间存在窗口——
若用户在此期间按 Esc，PID 为 0，SIGTERM 未发，进程照常运行并持有锁文件。

### 修复
在 `cancel_update()` 中设置标志后，最多等待 500 ms（10×50 ms）让 PID 就绪：

```rust
let mut pid = CHILD_PID.load(Ordering::SeqCst);
if pid == 0 {
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        pid = CHILD_PID.load(Ordering::SeqCst);
        if pid != 0 { break; }
    }
}
```

同时，在 `run_streaming_command` 中于 `CHILD_PID.store()` 之后立即检查：

```rust
CHILD_PID.store(child_pid, Ordering::SeqCst);
// 处理竞争窗口：若用户在 PID 存入前已请求取消，补发 SIGINT
if should_cancel() {
    unsafe { libc::kill(-(child_pid as i32), libc::SIGINT); }
}
```

---

## Bug 4 — P2 信号选择错误：直接发 SIGTERM/SIGKILL，pacman 无法自清理

**文件**：`src/package_manager/streaming.rs`

### 旧行为
`cancel_update()` 和 `cleanup_child_processes()` 均直接发 **SIGTERM**，
超时后发 **SIGKILL**。

### 问题
- 用户在终端直接 `Ctrl+C` 时发送的是 **SIGINT**，pacman 的信号处理器响应 SIGINT，
  在退出前会自动删除 `/var/lib/pacman/db.lck`。
- SIGTERM 不属于 pacman 的默认处理范围，行为因版本而异。
- SIGKILL 直接杀死进程，任何清理代码都不会执行，锁文件必然残留。

### 修复
改为三段式信号阶梯，优先给 pacman 机会自我清理：

```
SIGINT（模拟 Ctrl+C）
  → 等待进程自然退出（最多 5 秒）
  → 若超时：SIGTERM
    → 再等待 1 秒
    → 若仍在运行：SIGKILL（最后手段）
      → try_remove_db_lock() 作为最终兜底
```

此修复同时应用于 `cancel_update()` 和 `cleanup_child_processes()`，
以及 cancelled 分支的等待超时（从 3 秒扩展至 6 秒）。

---

## Bug 5 — P2 Update 页面 Esc 不区分阶段

**文件**：`src/tui/mod.rs`

### 旧行为
`AppMode::Update` 的 Esc 对所有 `UpdatePhase` 均无条件调用 `cancel_update()`——
包括 `PreviewingUpdates`（无进程运行）、`AnalysisComplete`（已完成）等阶段，
会错误地置位 `SHOULD_CANCEL`，污染下一次操作的初始状态。

相比之下，Install/Remove 的 Esc 在各自 handler 中已按 phase 分发处理。

### 修复
Update Esc 也改为按 phase 分发：

```rust
AppMode::Update => {
    match app.update.phase {
        UpdatePhase::Updating | UpdatePhase::Analyzing => {
            // 仅在有子进程运行时才发取消信号
            crate::package_manager::cancel_update();
        }
        _ => {
            // 无运行中的子进程，直接返回，不污染 SHOULD_CANCEL
        }
    }
    app.mode = AppMode::Dashboard;
    app.update.reset_scroll();
}
```

---

## 修改文件汇总

| 文件 | 变更内容 |
|---|---|
| `src/package_manager/streaming.rs` | `try_remove_db_lock` 改用 `sudo -n rm -f`；`cancel_update` 增加 PID 等待窗口和 SIGINT→SIGTERM→SIGKILL 阶梯；`cleanup_child_processes` 同样改为 SIGINT 优先；`run_streaming_command` 移除内部 `reset_cancel()` 调用，增加 post-PID 竞争检查；`run_custom_command_streaming` 同理 |
| `src/package_manager/mod.rs` | 新增 `pub use streaming::reset_cancel` 导出 |
| `src/tui/update.rs` | `spawn_update_task` 在 spawn 前于 UI 线程调用 `reset_cancel()` |
| `src/tui/install.rs` | `spawn_install_task` 在 spawn 前于 UI 线程调用 `reset_cancel()` |
| `src/tui/remove.rs` | `spawn_remove_task` 在 spawn 前于 UI 线程调用 `reset_cancel()` |
| `src/tui/shell.rs` | `spawn_shell_task` 在 spawn 前于 UI 线程调用 `reset_cancel()` |
| `src/tui/mod.rs` | Update 页面 Esc 按 `UpdatePhase` 分发，仅在 `Updating`/`Analyzing` 时调用 `cancel_update()` |
