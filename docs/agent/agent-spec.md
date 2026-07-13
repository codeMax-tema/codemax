# CodeMax Agent 规格书

> 面向开发者的技术规范，定义状态机、事件契约、Quality Gate、错误学习等核心机制。

---

## 一、状态机定义

### 1.1 任务阶段（TaskPhase）

```rust
enum TaskPhase {
    Created,           // 任务已创建，未开始
    Planned,           // 已生成 TodoPlan
    Editing,           // 正在生成/应用 EditingPlan
    Validating,        // 正在执行验证命令
    AnalyzingError,    // 正在分析验证错误
    Repairing,         // 正在生成修复计划
    WaitingApproval,   // 等待用户审批（高风险操作）
    NeedsIntervention, // 需要人工介入（修复轮次超限或验证超时）
    Completed,         // 任务完成（验证通过）
    Failed,            // 任务失败（无法继续）
}
```

**状态转换规则：**

```
Created → Planned → Editing → Validating
                              ↓
                        [验证通过] → Completed
                              ↓
                        [验证失败] → AnalyzingError → Repairing → Editing
                              ↓
                        [超时/超限] → NeedsIntervention
                              ↓
                        [需要审批] → WaitingApproval → Editing
```

### 1.2 Todo 状态（TodoStatus）

```rust
enum TodoStatus {
    Pending,     // 待执行
    InProgress,  // 执行中
    Completed,   // 已完成
    Failed,      // 失败
    Skipped,     // 跳过
}
```

### 1.3 审批状态（ApprovalStatus）

```rust
enum ApprovalStatus {
    Pending,    // 待审批
    Approved,   // 已批准
    Rejected,   // 已拒绝
    Cancelled,  // 已取消
}
```

### 1.4 验证状态（ValidationStatus）

```rust
enum ValidationStatus {
    Requested,  // 已请求（等待执行）
    Passed,     // 通过
    Failed,     // 失败
    Cancelled,  // 已取消
    TimedOut,   // 超时
}
```

---

## 二、事件契约

### 2.1 事件结构

```rust
struct AgentEvent {
    event_id: String,        // 唯一标识
    task_id: String,         // 任务 ID
    event_type: EventType,   // 事件类型
    phase: TaskPhase,        // 当前阶段
    message: String,         // 事件描述
    created_at: DateTime,    // 创建时间
    payload: Option<serde_json::Value>,  // 附加数据
}
```

### 2.2 事件类型

```rust
enum EventType {
    // 任务生命周期
    TaskCreated,
    TaskCompleted,
    TaskFailed,
    
    // Todo 管理
    TodoCreated,
    TodoUpdated,
    
    // 编辑操作
    EditPlanGenerated,
    EditPlanApplied,
    
    // 验证流程
    ValidationRequested,
    ValidationCompleted,
    
    // 错误修复
    ErrorAnalyzed,
    RepairPlanGenerated,
    
    // 审批流程
    ApprovalRequested,
    ApprovalResolved,
    
    // 人工介入
    InterventionRequired,
}
```

### 2.3 事件日志存储

事件日志存储在任务的 `logs` 字段中，使用 `AgentLogEntry` 结构：

```rust
struct AgentLogEntry {
    id: String,
    level: String,      // "info" | "warning" | "error"
    message: String,
    created_at: DateTime,
}
```

---

## 三、Quality Gate 标准

### 3.1 验证命令

验证命令由运行时配置，典型示例：

```yaml
validation_commands:
  - command: "npm run check"
    cwd: "{{worktree_path}}"
    timeout: 300s
  - command: "npm test"
    cwd: "{{worktree_path}}"
    timeout: 600s
```

### 3.2 通过标准

**代码质量：**
- 静态检查零错误（lint、type check）
- 无未解决的编译警告（除非项目明确允许）
- 代码风格符合项目规范（通过 format check）

**测试覆盖：**
- 所有单元测试通过
- 与修改模块相关的集成测试通过
- 新增功能必须有对应测试
- 测试覆盖率不低于项目基线

**架构合规：**
- 未违反模块边界
- 未引入未经审批的新依赖
- 未修改共享契约而未同步更新消费方

**安全与隐私：**
- 无硬编码密钥、Token 或密码
- 无明文存储敏感配置
- 用户输入已做校验和转义

### 3.3 验证结果判定

```rust
impl ValidationResult {
    fn passed(&self) -> bool {
        self.exit_code == 0 && !self.timed_out && !self.cancelled
    }
}
```

---

## 四、错误学习机制

### 4.1 错误记录格式

```json
{
  "error_id": "err_20260712_abc123",
  "task_id": "task_01J...",
  "phase": "repairing",
  "created_at": "2026-07-12T10:30:00Z",
  "error_type": "validation_failed",
  "symptom": "TypeScript compilation error: Property 'foo' does not exist",
  "failed_action": "Updated interface definition",
  "root_cause": "Missing property in implementation",
  "resolution": "Added missing property to implementation",
  "verification": "Validation passed after repair",
  "reusable_insight": "When updating interfaces, always update all implementations",
  "retention": "always",
  "tags": ["typescript", "interface", "compilation"]
}
```

### 4.2 跨任务复用规则

1. **标签匹配** — 新任务开始时，根据任务涉及的模块和错误类型检索相关经验
2. **时效性检查** — 超过 90 天的经验需验证是否仍然适用
3. **优先级排序** — 同一错误的最新解决方案优先于历史方案
4. **作用域限制** — 仓库级经验可跨任务复用，任务级经验仅限同类任务
5. **用户纠正优先** — 用户明确纠正过的错误，其经验永久保留

### 4.3 错误学习约束

1. 不记录 API Key、Token、密码、证书或隐私数据
2. 不保存完整大型日志，只保存必要摘要（单条记录不超过 500 字符）
3. 不把普通探索、无结果搜索或用户取消当成错误
4. 不重复犯已经记录且适用于当前任务的错误
5. 根因未确定时写明"正在调查"，不得编造原因

---

## 五、多任务并发协调

### 5.1 隔离原则

- 每个任务必须在独立的 worktree 中工作
- 禁止直接修改其他任务的 worktree 或目标分支
- 任务间不得共享未提交的修改

### 5.2 冲突检测

```rust
fn detect_conflicts(task: &Task, target_branch: &str) -> Vec<Conflict> {
    // 检查目标分支是否有其他任务的待合并修改
    // 发现潜在冲突时，标记任务状态为 NeedsIntervention
}
```

### 5.3 合并顺序

- 多个任务等待合并时，按 `readyToMerge` 时间戳排序
- 先合并的任务必须通过完整的 Quality Gate
- 后合并的任务必须 rebase 或 merge 最新目标分支后重新验证

### 5.4 共享资源协调

- 修改共享契约（API、数据库 schema、配置文件）时，必须通知用户协调其他任务
- 不得单方面修改被多个任务依赖的公共模块
- 涉及数据库迁移时，必须确保迁移脚本幂等且可回滚

---

## 六、降级策略

### 6.1 工具不可用

```rust
fn handle_tool_unavailable(tool: &str) -> Action {
    // 优先使用替代工具
    // 无替代方案时，标记任务为 NeedsIntervention
}
```

### 6.2 网络异常

```rust
fn handle_network_error(error: &NetworkError) -> Action {
    // 联网搜索失败时，回退到本地文档和项目源码
    // 无法确认外部信息时，标记为"尚未验证"
}
```

### 6.3 命令执行超时

```rust
fn handle_command_timeout(result: &ValidationResult) -> Action {
    if result.timed_out {
        // 取消命令并告知用户
        // 标记任务为 NeedsIntervention
    }
}
```

### 6.4 环境信息缺失

```rust
fn handle_missing_env_info(missing: &Vec<String>) -> Action {
    // 缺少关键环境信息时，立即询问用户
    // 不得凭猜测继续执行高风险操作
}
```

---

## 七、审批流程

### 7.1 高风险操作

以下操作需要用户审批：

- 删除文件（`delete` 操作）
- 修改仓库外路径
- 修改系统配置
- 使用管理员权限
- 访问未授权网络
- 执行数据库破坏性迁移
- 强制推送或覆盖分支

### 7.2 审批请求格式

```json
{
  "approval_id": "approval_task_01J_1",
  "approval_type": "model_delete",
  "risk_level": "high",
  "content": "Approve deletion of workspace files: src/old.ts [plan:abc123]",
  "reason": "Model-generated delete operations require explicit approval.",
  "status": "pending"
}
```

### 7.3 审批结果处理

```rust
fn handle_approval_result(approval: &Approval) -> Action {
    match approval.status {
        ApprovalStatus::Approved => continue_task(),
        ApprovalStatus::Rejected => fail_task(),
        ApprovalStatus::Cancelled => fail_task(),
        ApprovalStatus::Pending => wait_for_approval(),
    }
}
```

---

## 八、隐私与安全

### 8.1 敏感信息脱敏

```rust
fn redact_model_context(content: &str) -> String {
    // 脱敏 API Key、Token、密码等敏感信息
    // 使用 [REDACTED] 替换
}
```

### 8.2 路径安全验证

```rust
fn validate_path_safety(path: &str, worktree: &Path) -> Result<()> {
    // 确保路径在 worktree 内
    // 禁止使用 .. 逃逸
    // 禁止使用绝对路径
}
```

### 8.3 文件内容安全

```rust
fn validate_file_content_safety(content: &str) -> Result<()> {
    // 确保内容为 UTF-8 编码
    // 禁止修改二进制文件
    // 禁止包含敏感信息
}
```

---

## 九、性能优化

### 9.1 上下文裁剪

```rust
fn bounded_context_items(items: &Vec<String>, limit: usize) -> Vec<String> {
    // 保留最近的 N 条记录
    // 每条记录不超过指定长度
}
```

### 9.2 验证输出截断

```rust
fn bounded_validation_output(output: &str, limit: usize) -> String {
    if output.len() <= limit {
        return output.to_string();
    }
    let head_size = limit / 3;
    let tail_size = limit - head_size;
    format!("{}...[truncated]...{}", &output[..head_size], &output[output.len()-tail_size..])
}
```

---

## 十、扩展机制

### 10.1 自定义验证命令

```yaml
validation_commands:
  - command: "cargo check"
    cwd: "{{worktree_path}}"
    timeout: 300s
  - command: "cargo test"
    cwd: "{{worktree_path}}"
    timeout: 600s
```

### 10.2 自定义审批规则

```yaml
approval_rules:
  - operation: "delete"
    risk_level: "high"
    require_approval: true
  - operation: "update"
    path_pattern: "**/config/**"
    risk_level: "medium"
    require_approval: true
```

### 10.3 自定义错误学习策略

```yaml
error_learning:
  retention_policy: "always"  # ask | always | never
  max_age_days: 90
  tag_matching: true
  freshness_check: true
```
