# ADR-001: Awen — Terminal Intelligence Layer 架构设计

> 状态：草案
> 日期：2026-05-19
> 作者：Saonian

---

## 1. 背景与动机

Warp 终端提供了目前最好的命令行输入补全体验——历史匹配的 ghost text、参数级补全、AI 智能建议——但它是一个重量级闭源终端，且日益臃肿。

开发者社区正在大量迁移到轻量现代终端（Ghostty、Kitty、WezTerm、Alacritty），但这些终端在命令行智能体验上与 Warp 存在显著差距。现有的 shell 补全方案要么只覆盖历史匹配（zsh-autosuggestions），要么只覆盖参数补全（withfig/autocomplete），要么只覆盖 AI 建议（autocomplete.sh），没有一个开源方案将它们统一为一个完整的智能层。

本项目目标是构建 **Awen**——一个开源的终端智能层（Terminal Intelligence Layer），让任何现代终端都能获得超越 Warp 的智能输入体验。Awen 不仅仅是命令补全，而是基于你的上下文信号，在你需要的时候轻声给出恰好的建议。

---

## 2. 产品定位

Awen 是：

> **Terminal Intelligence Layer** —— 终端原生的智能输入增强层

核心一句话：

> **让输入变聪明，而不是让 AI 接管终端。**

Awen 的竞争力不在于"能补全 docker run"，而在于：

- 整体交互哲学：低语而非喊叫
- 输入流设计：三种 UI 模式无缝切换
- 多层推理结构：从确定性到概率性的 graceful degradation
- 延迟分层：每一层有自己的时间预算
- 上下文信号：轻量采集，提升建议相关性
- 建议仲裁：多维度、多信号源的智能排序
- 意图预测：不是补全文本，而是预判行为

### 产品哲学边界：永远只建议，永远不执行

这是 Awen 最重要的设计原则，不可违反。

**Awen 是"增强"，不是"接管"。** "增强"和"接管"是 AI 产品里最关键的边界之一。很多 AI terminal 产品正在追求 autonomous terminal agent，但终端最大的价值恰恰是开发者的直接控制感。Awen 选择站在"增强"这一侧——它像空气一样存在，超低延迟、极轻、永不打断、永不越权、永远只建议。

具体而言：

**Awen 永远不做的事**：

- 自动执行命令（任何命令都必须由用户按键确认）
- 修改文件
- 调用外部工具或 API（除了自身的 AI 补全 API）
- 自主访问 shell
- 长期任务规划
- 目标设定与自主工作流
- 深度 agent memory

**Awen 只做的事**：

- 建议（ghost text, dropdown, inline hint）
- 解释（命令解释）
- 警告（风险检测）
- 修复提示（失败修复建议）

**用户始终拥有完全控制权**。每一条建议都需要用户主动接受（按 →、按 Tab、按 Enter）。Awen 永远不会替用户做决定。这意味着：

1. **安全模型极简**：不需要 execution sandbox、permission escalation、capability runtime、agent memory isolation。Suggestion 的风险和 Action 的风险根本不是一个数量级。
2. **用户信任度高**：用户知道"它永远不会替我做决定"，所以敢一直开着它。这对形成使用习惯至关重要。
3. **上下文采集克制**：不需要完整 repo 理解、完整文件读取、完整 terminal capture。只需要当前输入、最近几条命令、cwd、repo type、git 状态、failure stderr 摘要——这已经足够做非常强的智能提示。
4. **产品复杂度可控**：不会从 zsh enhancement 一路膨胀到 full AI operating system。

---

## 3. 设计目标

- **轻量**：后台常驻 daemon 内存 < 50MB，不拖慢终端启动
- **低延迟**：本地补全 < 20ms，AI 补全异步到达不阻塞输入
- **低成本**：接入 DeepSeek 等便宜模型，单日正常使用费用可忽略（< ¥0.1）
- **上下文感知**：采集项目类型、git 状态、近期命令、失败历史等信号，提升建议相关性
- **可扩展**：用户可通过配置文件自定义命令规格、上下文规则、AI provider
- **渐进增强**：每一层独立工作，任意一层不可用不影响其他层

---

## 4. 架构概览

### 四层智能架构

```
┌──────────────────────────────────────────────────────────────┐
│                        用户终端                               │
│           (Ghostty / Kitty / WezTerm / Alacritty)            │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                   zsh (ZLE Widget)                     │  │
│  │  ┌──────────────────────────────────────────────────┐  │  │
│  │  │  Shell 插件 (.zsh)                               │  │  │
│  │  │  - 拦截按键事件 / 采集 shell 上下文信号           │  │  │
│  │  │  - 渲染 ghost text / dropdown / inline hint      │  │  │
│  │  │  - 接受/拒绝/部分接受建议                        │  │  │
│  │  └──────────────────┬───────────────────────────────┘  │  │
│  └─────────────────────┼──────────────────────────────────┘  │
│                        │ Unix Socket                         │
│  ┌─────────────────────┼──────────────────────────────────┐  │
│  │              Daemon (Rust + tokio)                     │  │
│  │                                                        │  │
│  │  ┌──────────────────────────────────────────────────┐  │  │
│  │  │           Suggestion Arbitrator                  │  │  │
│  │  │  多维仲裁：合并、排序、去重、冲突消解             │  │  │
│  │  └──────┬──────────┬───────────┬───────────┬────────┘  │  │
│  │         │          │           │           │           │  │
│  │  ┌──────┴───┐ ┌────┴────┐ ┌───┴────┐ ┌────┴────────┐ │  │
│  │  │ Layer 1  │ │ Layer 2 │ │ Layer 3│ │  Layer 3+   │ │  │
│  │  │ History  │ │ Specs   │ │ AI     │ │  Features   │ │  │
│  │  │ < 5ms    │ │ < 20ms  │ │ async  │ │  async      │ │  │
│  │  └──────────┘ └─────────┘ └────────┘ └─────────────┘ │  │
│  │         ▲          ▲           ▲           ▲          │  │
│  │         └──────────┴───────────┴───────────┘          │  │
│  │                        │                              │  │
│  │  ┌─────────────────────┴────────────────────────────┐ │  │
│  │  │              Layer 0: Context Engine              │ │  │
│  │  │                                                   │ │  │
│  │  │  Session │ Repo │ Intent │ Failure │ Env │ Git    │ │  │
│  │  └───────────────────────────────────────────────────┘ │  │
│  │         │          │           │          │            │  │
│  │  ┌──────┴───┐ ┌────┴────┐ ┌───┴────┐ ┌──┴──────┐     │  │
│  │  │ SQLite   │ │ TOML    │ │ LLM    │ │ Shell   │     │  │
│  │  │ History  │ │ Specs   │ │ API    │ │ Signals │     │  │
│  │  └──────────┘ └─────────┘ └────────┘ └─────────┘     │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

**核心设计原则**：Layer 0（Context Engine）是所有上层的数据基础。Layer 1 是确定性补全层（history + specs），Layer 2 是预测建议层（AI completion + recovery + explanation + warning）。Suggestion Arbitrator 是最终的决策层，决定用户看到什么。Awen 永远只建议，永远不执行。

---

## 5. 核心决策

### 5.1 架构模式：Daemon + Shell 插件

**决策**：采用后台 daemon 进程 + zsh 插件的分离架构，通过 Unix socket 通信。

**理由**：

- 不做成 Rust 库（只有 Rust CLI 开发者能用），也不做成独立终端（太重）
- daemon 常驻保持状态（上下文引擎、历史索引、specs 缓存、AI 连接池），shell 插件只负责 UI 和信号采集
- 与 atuin 同样的架构模式，已验证可行
- 未来扩展 fish/bash 只需新增 shell 插件，daemon 不变

**通信协议**：Unix socket + JSON 消息，请求/响应模式。

```json
// 请求（shell 插件 → daemon）
{
  "type": "suggest",
  "input": "docker run -",
  "cursor_pos": 12,
  "context": {
    "cwd": "/home/user/myapp",
    "last_command": "docker build -t myapp .",
    "last_exit_code": 0,
    "last_stderr": "",
    "git_branch": "feat/auth",
    "git_status": "ahead=2",
    "session_commands": ["npm run build", "docker build -t myapp ."],
    "env_hints": ["NODE_ENV=development"]
  },
  "timestamp": 1716100000
}

// 响应（daemon → shell 插件）
{
  "suggestions": [
    {
      "text": "it -p 3000:3000 myapp",
      "source": "intent",
      "confidence": 0.92,
      "reasoning": "刚构建了 myapp 镜像，大概率要运行"
    },
    {
      "text": "-p",
      "source": "specs",
      "confidence": 1.0,
      "desc": "映射端口",
      "meta": { "arg": "HOST:CONTAINER" }
    }
  ],
  "hint": null,
  "warning": null
}
```

### 5.2 Shell 层：zsh 优先，fish 次之

**决策**：首先支持 zsh，其次 fish。不支持 bash。

**补全 UI 三种模式**：

**模式 A：Ghost text（内联建议）**

排名第一的建议以灰色文本渲染在光标后方。

**模式 B：Dropdown 候选菜单（多候选列表）**

在输入行下方渲染浮动候选列表。

```
$ docker run --
  ┌──────────────────────────────────────────────────┐
  │  -p, --port HOST:CONTAINER    映射端口      specs │
  │  -v, --volume HOST:CONTAINER  挂载目录      specs │
  │  -e, --env KEY=VALUE          环境变量      specs │
  │  -it -p 3000:3000 myapp       运行刚构建镜像 intent│
  └──────────────────────────────────────────────────┘
```

**模式 C：Inline hint（行内提示）—— 极度克制**

在输入行上方显示辅助信息。**默认只在两种高价值场景触发**：

场景一：风险警告（本地检测，零延迟）

```
  ╭ ⚠ 这会删除当前目录下所有文件，包括隐藏文件
$ rm -rf ./*
```

场景二：失败修复提示（上一条命令失败时）

```
  ╭ ℹ cargo build 失败：cannot find crate `tokio`
$ cargo add tokio
```

命令解释**默认关闭**，仅在用户按快捷键主动触发时显示。因为终端用户最珍贵的是认知带宽——"少说话"比"更聪明"更重要。

**三种模式的触发逻辑**：

- 候选 ≤ 1 条：只显示 ghost text
- 候选 > 1 条且包含 specs 结果：显示 dropdown + ghost text
- 检测到高危命令：自动显示风险警告（inline hint）
- 上一条命令失败：自动显示修复建议（inline hint + ghost text）
- 用户按 `Ctrl+E`：主动触发命令解释（inline hint）
- Tab 手动展开/关闭 dropdown

**快捷键设计**：

- `→`：接受 ghost text（整条）
- `Ctrl+→`：接受 ghost text 中下一个单词
- `Tab`：打开/关闭 dropdown
- `↑` / `↓`：dropdown 导航
- `Enter`：确认 dropdown 选中项
- `Ctrl+E`：触发命令解释（inline hint）
- `Esc`：关闭所有 UI 层

---

### 5.3 Layer 0：Context Engine（上下文信号采集）

**定位**：不是"理解用户"，而是"提升建议相关性"。Context Engine 采集的都是 deterministic、shallow、signal-based 的信号——用来做权重加成，不做语义推理、不做行为建模、不做工作流认知。

#### Session Context（会话上下文）

shell 插件通过 `precmd` / `preexec` hook 持续向 daemon 上报：

- 最近 N 条命令（含时间戳、exit code）
- 最近一条命令的 stderr 输出（截断至 500 字符）
- 当前工作目录及 cd 轨迹
- session 持续时间

#### Repo Context（项目上下文）

daemon 在 cwd 变更时自动检测项目类型：

| 检测文件                          | 项目类型       | 进一步检测                                                     |
| --------------------------------- | -------------- | -------------------------------------------------------------- |
| package.json                      | Node.js        | pnpm-workspace.yaml → pnpm / turbo.json → turbo / nx.json → nx |
| Cargo.toml                        | Rust           | [workspace] → Cargo workspace                                  |
| pyproject.toml / requirements.txt | Python         | —                                                              |
| go.mod                            | Go             | —                                                              |
| docker-compose.yml                | Docker Compose | —                                                              |
| Dockerfile                        | Docker         | —                                                              |
| flake.nix                         | Nix            | —                                                              |
| Makefile                          | Make           | —                                                              |

结果缓存，cwd 变更时重新检测。影响 AI prompt 注入和历史匹配权重。

#### Git Context（Git 上下文）

采集信号：当前分支、ahead/behind、文件变更状态、最近 commit。

影响：`git push` 在 ahead > 0 时权重提升，`git pull` 在 behind > 0 时权重提升，`git add` 在有 unstaged changes 时权重提升，分支名注入 AI prompt。

#### Failure Context（失败上下文）

当 `last_exit_code != 0` 时激活"修复模式"：

1. 解析 stderr，匹配内置 failure patterns
2. 匹配成功 → 立即生成修复建议（ghost text + inline hint）
3. 同时异步发送 stderr 给 AI 获取更精准修复

**内置 failure patterns 示例**：

```toml
[[failure_patterns]]
pattern = "cannot find crate `(\\w+)`"
suggestion = "cargo add {1}"
description = "缺少 Rust 依赖"

[[failure_patterns]]
pattern = "Module not found.*'(\\S+)'"
suggestion = "npm install {1}"
description = "缺少 Node 依赖"

[[failure_patterns]]
pattern = "command not found: (\\w+)"
suggestion = "brew install {1}"
description = "命令未安装"

[[failure_patterns]]
pattern = "Permission denied"
suggestion = "sudo !!"
description = "权限不足"

[[failure_patterns]]
pattern = "port (\\d+) already in use"
suggestion = "lsof -i :{1}"
description = "端口占用"
```

#### Recency Signals（近期信号）

不做"理解用户意图"，只做"基于近期命令的相关性提升"。这是 deterministic 的信号加权，不是 semantic inference。

规则示例：

| 最近命令              | 信号                   | 加权                |
| --------------------- | ---------------------- | ------------------- |
| git add → git commit  | 下一条大概率 git 相关  | 同工具命令 ×1.5     |
| 上一条命令失败        | 下一条大概率修复       | 同工具修复命令 ×2.0 |
| 连续 3 条 docker 命令 | 当前在 docker 上下文中 | docker 相关 ×1.3    |

注意：这不是"推断用户在做什么"，只是"最近干了什么就近优先"。类似浏览器的"最近访问"排序，不是用户画像。

---

### 5.4 Layer 1：Deterministic Completion（确定性补全）

#### 历史匹配

- SQLite 存储，nucleo 模糊匹配
- 排序：`score = match_score × recency_decay × frequency_boost × directory_affinity`
- 延迟预算：< 5ms

#### 命令规格（Specs）

- TOML 格式，用户可手写扩展
- 触发：已知命令 + 光标在 flag/subcommand 位置
- 延迟预算：< 20ms

---

### 5.5 Layer 2：Predictive Suggestion Layer（预测建议层）

这一层只负责四件事：补全（completion）、修复（recovery）、警告（warning）、解释（explanation，用户主动触发）。不做规划（planning）、执行（execution）、自动化（automation）、工作流推断（workflow cognition）。

#### AI 补全

- 触发：停止输入 > 300ms（debounce）
- Context-Aware Prompt：注入 cwd、repo type、git context、recent commands、stderr
- 支持取消：继续打字时 abort 未完成请求

#### Failure Recovery（失败修复）— 核心特性

**这是 Awen 与其他补全工具的最大差异化特性，与 ghost text 并列为两大核心体验。**

前一条命令失败时自动激活：

1. 本地 pattern 即时匹配 → ghost text + inline hint 同时出现
2. AI 异步增强 → 如果比本地 pattern 更好则替换
3. 天然符合"suggestion not action"——用户本来就遇到了错误，系统轻声提示修复命令，风险极低、价值极高

#### Command Explanation（命令解释）— 用户主动触发

**默认关闭**。用户按快捷键（如 `Ctrl+E`）时，AI 异步生成当前命令的人类可读解释，显示在 inline hint 区域。结果缓存，同一命令不重复请求。不主动展示是因为：terminal 用户的认知带宽极其珍贵，AI 不停说话会变成干扰。

#### Risk Detection（风险检测）

高危命令（rm -rf、git push --force、chmod 777、curl | bash 等）在 inline hint 区域显示警告。本地正则匹配，零延迟，不依赖 AI。

---

### 5.6 Suggestion Arbitrator（建议仲裁系统）

各层独立产生候选，Arbitrator 负责最终决策。

#### 建议的语义维度

| 来源      | 语义                 | 典型置信度           | 延迟         |
| --------- | -------------------- | -------------------- | ------------ |
| specs     | "这个命令允许这样写" | 高（确定性）         | < 20ms       |
| history   | "你以前这样做过"     | 中-高                | < 5ms        |
| intent/ai | "推测你想做什么"     | 中（概率性）         | 200-800ms    |
| failure   | "这可能修复你的错误" | 高（匹配到 pattern） | < 5ms / 异步 |

#### 仲裁流程

```
Phase 1: 即时响应（< 20ms）
  - 收集 history + specs 结果
  - failure context 匹配 → 修复建议提升到第一位
  - 按 score 排序，输出给 UI

Phase 2: AI 增强（异步）
  - AI 结果到达 → 与已有结果合并去重（编辑距离 < 3 视为重复）
  - 优于当前 ghost text → 更新
  - 劣于当前 ghost text → 仅追加到 dropdown

Phase 3: 上下文加权
  - Git ahead > 0 且输入 git → push 权重 ×2
  - 最近命令失败 → 修复建议权重 ×3
  - 同目录命令 → 权重 ×1.5
  - 项目类型匹配 → 权重 ×1.3
```

#### Ghost text 更新策略

- 打字过程中：仅本地层驱动（无闪烁）
- 停止打字后：AI 可更新 ghost text
- 用户已部分接受（光标右移中）：不再更新
- 新旧 ghost text 前缀相同：平滑过渡

---

### 5.7 命令规格格式（Specs）

自定义 TOML 格式，不解析 Fig TypeScript specs。

```toml
# ~/.config/awen/specs/docker.toml
[command]
name = "docker"
description = "容器管理工具"

[[command.subcommands]]
name = "run"
description = "运行容器"

[[command.subcommands.flags]]
name = "--port"
short = "-p"
arg = "HOST:CONTAINER"
description = "映射端口"

# ... 更多 flags
```

**目录结构**：

```
~/.config/awen/
├── config.toml              # 全局配置
├── specs/                   # 用户自定义 specs（优先级高）
├── failure_patterns.toml    # 用户自定义失败修复映射
├── risk_patterns.toml       # 用户自定义风险规则
└── cache/
    ├── history.db           # SQLite 历史数据库
    └── ai_cache.db          # AI 响应缓存

/usr/share/awen/specs/       # 内置 specs（优先级低）
├── git.toml, docker.toml, kubectl.toml, npm.toml
├── cargo.toml, pip.toml, brew.toml, curl.toml
├── ssh.toml, claude.toml
```

---

### 5.8 AI Provider 设计

可插拔 provider 接口，默认 DeepSeek，支持 Ollama。

```toml
# ~/.config/awen/config.toml
[ai]
enabled = true
provider = "deepseek"         # deepseek | ollama | openai | anthropic
debounce_ms = 300
max_tokens = 60
cache_ttl_minutes = 30

[ai.deepseek]
api_key = "sk-xxx"            # 或 DEEPSEEK_API_KEY 环境变量
model = "deepseek-chat"

[ai.ollama]
model = "qwen2.5-coder:7b"
base_url = "http://localhost:11434"

[context]
session_history_size = 20
stderr_max_chars = 500
repo_detect = true
git_context = true

[ui]
ghost_text_color = 242
dropdown_max_items = 8
hint_style = "above"          # above | below | statusline
risk_detection = true
command_explanation = true
```

**成本估算**：~250 tokens/请求，每天 250 次 ≈ ¥0.07/天

---

### 5.9 Daemon 生命周期

按需启动 + 自动退出。

```bash
awen start       # 手动启动
awen stop        # 停止
awen status      # 状态 + context 摘要
awen config      # 打开配置
awen logs        # 查看日志
awen context     # 查看 context engine 状态（调试用）
```

---

## 6. 技术栈

| 组件        | 技术选型                 | 理由                         |
| ----------- | ------------------------ | ---------------------------- |
| Daemon      | Rust + tokio             | 性能、内存安全、单二进制分发 |
| 模糊匹配    | nucleo                   | Helix 验证过的匹配引擎       |
| 历史存储    | SQLite (rusqlite)        | 单文件嵌入式                 |
| HTTP 客户端 | reqwest                  | AI API 调用                  |
| 正则引擎    | regex                    | failure/risk pattern         |
| 序列化      | serde + toml             | 配置和 specs                 |
| IPC         | Unix socket + serde_json | 跨进程通信                   |
| Shell 插件  | zsh script (ZLE)         | 零依赖                       |

---

## 7. 完整功能矩阵

| 功能             | 描述                        | 数据来源                  | 延迟          | 默认                  |
| ---------------- | --------------------------- | ------------------------- | ------------- | --------------------- |
| 历史 ghost text  | 基于历史的内联补全          | SQLite + nucleo           | < 5ms         | ✅ 开                 |
| Specs 补全       | 命令参数确定性补全          | TOML specs                | < 20ms        | ✅ 开                 |
| AI 补全          | 上下文感知的智能补全        | LLM API                   | 200-800ms     | ✅ 开                 |
| Dropdown 菜单    | 多候选浮动列表              | 全部来源                  | 即时          | ✅ 开                 |
| **失败修复建议** | **失败后自动建议修复命令**  | **stderr + pattern + AI** | **即时/异步** | **✅ 开**             |
| **风险检测**     | **高危命令内联警告**        | **regex**                 | **< 1ms**     | **✅ 开**             |
| 项目类型检测     | 识别 Node/Rust/Python/Go 等 | 文件系统                  | < 10ms        | ✅ 开                 |
| Git 上下文       | 感知分支、ahead/behind      | git CLI                   | < 50ms        | ✅ 开                 |
| Recency 加权     | 近期命令的相关性提升        | session context           | < 5ms         | ✅ 开                 |
| 建议仲裁         | 多来源智能排序              | Arbitrator                | < 1ms         | ✅ 开                 |
| 命令解释         | 复杂命令可读化              | AI                        | 异步          | ❌ 关（用户主动触发） |

**两大核心体验**：智能 ghost text + 失败修复建议（Failure Recovery）。这两个特性最高频、最高价值、最低侵入，是 Awen 区别于所有现有工具的核心。

---

## 8. 实现阶段

> 从完整产品设计中切出实现顺序，不是功能边界。所有功能最终都会实现。

### Phase 1 — 核心管道（2 周）

- [ ] Rust daemon + Unix socket + JSON 协议
- [ ] zsh 插件：ZLE widget、ghost text 渲染、接受/拒绝
- [ ] 历史匹配 + directory-aware 排序
- [ ] Context Engine 基础：cwd、session commands、exit_code 采集

### Phase 2 — AI + Failure Recovery（2 周）

- [ ] DeepSeek + Ollama 接入
- [ ] Context-Aware Prompt（repo type、git、recent commands）
- [ ] Failure Context：stderr 采集 + pattern 匹配 + AI 修复 + inline hint
- [ ] Risk Detection（本地正则，零延迟）
- [ ] Suggestion Arbitrator：多来源合并、上下文加权

### Phase 3 — UI 完整体验（2 周）

- [ ] Dropdown 候选菜单 + 双模式切换
- [ ] Inline hint UI
- [ ] TOML specs 解析 + 内置 10+ 命令 + 热重载

### Phase 4 — 打磨（2 周）

- [ ] Command Explanation（opt-in，快捷键触发）
- [ ] Git Context 深度集成（ahead/behind 感知排序）
- [ ] Recency Signals 加权优化
- [ ] 边界 case 处理（tmux 降级、多行命令、terminal resize）

### Phase 5 — 发布（1 周）

- [ ] install.sh + daemon 管理 + 诊断命令
- [ ] 文档 + fish 插件

---

## 9. 架构边界

- **不做 bash 支持**：readline 无法优雅实现 ghost text
- **不做动态 generator**：动态候选由 AI 层处理
- **不做 Fig specs 兼容**：自定义 TOML，不承担 TS 解析
- **不做跨机器同步**：本地优先
- **不做终端适配**：标准 ANSI 序列，现代终端天然支持
- **不做终端替换**：Awen 是 shell 插件 + daemon

---

## 10. 长期演化方向

> 不影响当前架构决策，但当前架构应为这些方向留出扩展空间。

### Semantic History（语义历史）

从字符串匹配到语义匹配。"你上次部署 staging 的时候用了这些命令"。

### Session Memory（跨 Session 记忆）

持久化 session 摘要，新 session 中延续上下文。

### Workflow Macros（工作流宏）

输入 `deploy` 展开为项目特定的完整部署命令序列。

### Project Skills（项目级技能）

`.awen/` 目录放置项目特定配置、specs、工作流定义。

### Multi-line Intent（多行意图）

理解多行命令构建过程，建议 flag 组合而非单个 flag。

---

## 11. 风险与缓解

| 风险             | 影响            | 缓解策略                  |
| ---------------- | --------------- | ------------------------- |
| AI 延迟          | ghost text 闪烁 | debounce + 本地层即时兜底 |
| Dropdown 兼容性  | 错位、闪烁      | 标准 ANSI + 终端行数检测  |
| ZLE 插件冲突     | 补全失效        | widget 白名单             |
| Context 采集开销 | shell 变慢      | 缓存 + 异步采集           |
| API 不可用       | AI 失效         | 降级本地 + Ollama 备用    |
| Failure 误判     | 错误修复建议    | pattern 保守 + 标注来源   |
| 隐私             | 敏感信息泄露    | 过滤 + .awenignore        |

---

## 12. 名称由来与设计哲学

### 名称：Awen [AH-wen]

> **Smart when you need it. Silent when you don't.**

**Awen** 是威尔士语，意为"灵感"与"流动的精神"。在威尔士诗歌传统中，Awen 是诗人与吟游诗人的灵感缪斯——不是外力强加的指令，而是从内心自然涌现的创造力。获得 Awen 的人被称为 _awenydd_（被灵感所触的人）。

这个词与威尔士语 _awel_（微风）同源，共享印欧语根 \*-uel（吹）。

**第一层——灵感（Inspiration）**。Awen 的智能层不是机械的自动完成，而是在你还没想清楚自己要什么的时候，灵感般地浮现那条恰好的命令。它基于你的历史和上下文信号，以 ghost text 的形式轻声呈现——不强迫，不打断，只是在那里，等你选择接受或忽略。

**第二层——微风（Breeze）**。Awen 诞生于对 Warp 终端日益臃肿的不满。Warp 的补全体验是好的，但它把自己做成了一个沉重的闭源终端。Awen 选择成为微风——轻盈地吹过任何终端，带来超越 Warp 的智能体验，然后安静地退到后台。

### 产品人格

Awen 的气质是：**calm, quiet, lightweight, respectful, unobtrusive, competent。**

这不是功能描述，而是产品人格。它决定了 Awen 的每一个细节——从 ghost text 的灰色深度，到 inline hint 的措辞语气，到 dropdown 出现的时机。

### Hint 语气设计

Inline hint 的措辞应该像一个熟练的同事在旁边轻声提醒，而不是一个 AI 系统在分析你。

```
✗ 不要这样：
  ╭ ⚠ ERROR: Detected dangerous recursive deletion operation targeting current directory
  ╭ ℹ AI ANALYSIS: Build failure caused by missing dependency `tokio`

✓ 应该这样：
  ╭ ⚠ This will delete everything in the current directory
  ╭ ℹ Looks like `tokio` is missing
  ╭ ↳ Port 3000 is already in use
```

规则：

- 用自然语言，不用技术术语堆砌
- 不出现 "AI"、"Analysis"、"Detected" 这类机械化词汇
- 一句话说完，不解释推理过程
- 低调、温和、有用

### 设计哲学

**渐进增强，逐层显现。** 四层架构像水面的涟漪——Context Engine 在你打开终端的瞬间就开始感知；历史匹配在你按键的瞬间就到了；命令规格在本地毫秒级补上参数提示；AI 意图预测像远处传来的回声，异步到达，自然融入。每一层独立工作，任一层缺失不影响其他层。

**低语，不是喊叫。Silent Intelligence。** Ghost text 是灰色的、半透明的、短暂的。Dropdown 只在有多个有意义的候选时才出现。Inline hint 默认只在风险警告和失败修复两种高价值场景出现——不主动展示命令解释、不做 AI reasoning 展示、不做 workflow hint。很多 AI 产品在疯狂展示智能，Awen 选择相反的方向：默认不打扰，只在高价值时出现。"少说话"比"更聪明"更重要。

**在你卡住的时候出现。** Awen 最强的时刻不是日常补全——那是基本功。真正的"Awen 时刻"是：`cargo build` 失败了，你还没来得及去查错误信息，ghost text 已经轻声浮现 `cargo add tokio`。这不是"AI 理解了你的意图"，而是系统检测到一个信号（exit code ≠ 0 + stderr 匹配到已知模式），然后用最克制的方式递给你一个建议。

**永远只建议，永远不执行。** 这是 Awen 的铁律。每一条建议都必须由用户主动确认——按 →、按 Tab、按 Enter。Awen 永远不会自动执行命令、修改文件、调用工具。Suggestion 的风险和 Action 的风险根本不是一个数量级。"永远不替用户做决定"让用户敢一直开着它——这个信任是 Awen 成为日常工具而非偶尔惊艳的玩具的前提。

**像空气一样存在。** Awen 追求的最终形态不是"很酷的 AI terminal agent"，而是"像空气一样的存在"——超低延迟、极轻、永不打断、永不越权。你感知不到它的存在，直到你需要它；你不用思考它在不在，因为它永远在。这个方向比 agent 更高级。

**你的终端，你的选择。** Awen 不是终端，不是 shell，不试图替代任何东西。它是一层透明的增强——装上它，你的 zsh 多了灵感；卸掉它，一切照旧。没有任何锁定。

---

## 13. 相关项目参考

| 项目                 | 关系                | 参考价值             |
| -------------------- | ------------------- | -------------------- |
| zsh-autosuggestions  | ZLE ghost text      | widget 机制、兼容性  |
| atuin                | daemon + shell 插件 | 架构模式、历史存储   |
| withfig/autocomplete | 补全数据            | specs 命令范围       |
| inshellisense        | PTY wrapper         | 反面参考——太重       |
| autocomplete.sh      | AI shell 补全       | prompt 设计          |
| Kiro CLI             | 三层合一            | 体验标杆             |
| Warp                 | 终端级智能          | Context Engine 理念  |
| Claude Code          | AI coding agent     | Context-aware prompt |
