# Awen

[English](README.md)

> **Terminal Intelligence Layer — 让输入变聪明，而不是让 AI 接管终端。**

Awen 是一个开源的终端智能输入增强层，让 Ghostty、Kitty、WezTerm、Alacritty 等任何现代终端都能获得超越 Warp 的智能输入体验。

Awen **永远只建议，永远不执行**。每一条建议都需要你主动接受（按 →、Tab、Enter）。它不会替你做决定、不会执行命令、不会修改文件。

## 核心特性

- **Ghost Text 补全** — 内联灰色建议文本；本地建议（历史 + specs）按键即出，AI 建议在停止输入后异步刷新
- **失败修复建议** — 上一条命令失败后，自动建议修复命令（只提示不执行）
- **风险检测** — 对 `rm -rf`、`git push --force`、`chmod 777` 等高危命令显示内联警告
- **命令规格补全** — TOML 格式的确定性参数补全，内置 77 条命令规格，覆盖 git、docker、npm、cargo、aws、gcloud、kubectl、helm、claude、codex 等
- **AI 智能补全** — 支持 DeepSeek、Ollama，超时受限可选补全，可关闭
- **上下文感知** — 项目类型检测、Git 状态、近期命令、失败历史

## 功能成熟度

| 功能 | 状态 |
|------|------|
| Ghost Text（历史 + 规格） | **稳定** |
| 风险检测 | **稳定** |
| 失败修复（本地模式） | 实验性（依赖 stderr 捕获） |
| AI 补全（DeepSeek / Ollama） | 实验性 |
| stderr 捕获 | 实验性（默认关闭） |
| 命令解释 | 计划中 |
| 下拉菜单 | 实验性 |

## 架构

```
用户终端 (Ghostty / Kitty / WezTerm / Alacritty)
  └─ zsh (ZLE Widget)
       └─ Shell 插件 (awen.zsh)
            │  阶段 1 (同步): skip_ai=true  → 本地结果 <20ms
            │  阶段 2 (异步, 条件触发): skip_ai=false → 本地不足时 AI 兜底
            │ Unix Socket
            └─ Daemon (Rust + tokio)
                 ├─ Context Engine (session / repo / git)
                 ├─ Layer 1: History (SQLite + nucleo) — < 5ms
                 ├─ Layer 1: Specs (TOML) — < 20ms
                 ├─ Layer 2: AI (DeepSeek / Ollama) — 异步，永不阻塞输入
                 ├─ Layer 2: Failure Recovery (pattern + AI)
                 ├─ Layer 2: Risk Detection (regex)
                 └─ Suggestion Arbitrator
```

## 安装

### 依赖

- Rust 工具链（1.85+）
- zsh
- jq（推荐，用于更稳定的 JSON 处理）
- socat（可选，用于 shell 与 daemon 通信；如无则使用 zsh 内置 zsocket）

### 从源码安装

```bash
git clone https://github.com/zzf2333/Awen.git
cd awen
./install.sh
```

安装脚本会：
1. 编译 release 版本
2. 安装到 `~/.local/bin/awen`
3. 复制 specs 和 zsh 插件到 `~/.config/awen/`
4. 生成默认配置文件
5. 自动将 `source ~/.config/awen/awen.zsh` 添加到 `~/.zshrc`（交互式确认，默认 yes）
6. 如 `~/.local/bin` 不在 PATH 中，自动添加

打开新终端即可使用 — Awen 会自动启动，首次运行时自动导入 zsh 历史记录。

### 手动安装

```bash
cargo build --release
cp target/release/awen ~/.local/bin/
cp plugin/awen.zsh ~/.config/awen/
cp specs/*.toml ~/.config/awen/specs/
# Add to .zshrc: source ~/.config/awen/awen.zsh
```

## 使用

### 快捷键

| 按键 | 功能 |
|------|------|
| `→` | 接受整条 ghost text |
| `Ctrl+→` | 接受下一个单词 |
| `Shift+Tab` | 清除建议 |

### CLI 命令

```bash
awen start              # 启动 daemon（zsh 插件会自动启动）
awen stop               # 停止 daemon
awen status             # 查看状态
awen logs               # 查看日志
awen config             # 查看配置
awen context            # 查看当前上下文
awen history import     # 从 zsh 历史导入（首次启动自动执行）
```

`history import` 支持 `--file <路径>` 指定自定义历史文件，`--force` 强制重新导入。

## 配置

配置文件位于 `~/.config/awen/config.toml`：

```toml
[ai]
enabled = true                  # 开关 AI 补全
provider = "deepseek"           # deepseek | ollama
debounce_ms = 300               # 停止输入后触发 AI 的延迟
timeout_ms = 30000              # AI 请求超时（毫秒，异步通道，不阻塞输入）
max_tokens = 1024               # AI 最大生成 token 数（推理模型需要更多）
min_local_candidates = 2        # 本地候选数低于此值且置信度也低时才触发 AI
min_local_confidence = 0.6      # 本地最高置信度低于此值且候选数也少时才触发 AI
cache_ttl_minutes = 30          # AI 响应缓存有效期（分钟）

[ai.deepseek]
api_key = ""                    # 或设置 DEEPSEEK_API_KEY 环境变量
model = "deepseek-chat"
base_url = "https://api.deepseek.com"

[ai.ollama]
model = "qwen2.5-coder:7b"
base_url = "http://localhost:11434"

[context]
session_history_size = 20       # 会话记忆的命令数量
stderr_max_chars = 500          # stderr 截断长度
repo_detect = true              # 自动检测项目类型
git_context = true              # 采集 Git 上下文
capture_stderr = true           # 捕获 stderr 用于失败修复

[ui]
ghost_text_color = 242          # ghost text 颜色 (ANSI 256)
hint_style = "above"            # 提示显示位置："above" 或 "below"
dropdown_max_items = 8          # 候选菜单最大条目（计划中）
risk_detection = true           # 高危命令警告
command_explanation = false     # 命令解释功能（计划中，尚未实现）
```

### 内置 Specs

Awen 内置 77 条命令规格，按类别分组：

<details>
<summary>完整列表（点击展开）</summary>

| 类别 | 命令 |
|------|------|
| VCS & 开发生态 | `git`, `docker`, `npm`, `cargo`, `brew`, `curl`, `ssh` |
| 云 & 基础设施 | `gh`, `kubectl`, `terraform`, `aws`, `gcloud`, `az`, `helm` |
| 语言 & 运行时 | `python`, `go`, `node` |
| 包管理 & 构建工具 | `pip`, `pnpm`, `yarn`, `bun`, `uv`, `poetry`, `cmake`, `make` |
| AI 工具 | `claude`, `codex`, `opencode`, `antigravity` |
| 文件操作 | `ls`, `rm`, `cp`, `mv`, `mkdir`, `touch`, `ln`, `chmod`, `chown` |
| 文本处理 | `cat`, `head`, `tail`, `grep`, `sed`, `awk`, `sort`, `uniq`, `wc`, `diff`, `cut`, `tr`, `tee`, `xargs` |
| 搜索、归档 & 进程 | `find`, `tar`, `ps`, `kill`, `df`, `du`, `lsof` |
| 网络 & 诊断 | `ping`, `dig`, `wget`, `ss`, `nmap` |
| 系统管理 | `systemctl`, `journalctl`, `htop` |
| 终端复用 | `tmux`, `screen` |
| 测试 & Lint | `pytest`, `ruff` |
| 任务运行器 | `just` |
| 数据库 CLI | `psql`, `mysql`, `redis-cli`, `mongosh`, `sqlite3` |

</details>

### 自定义 Specs

在 `~/.config/awen/specs/` 下创建 TOML 文件，可以添加新命令或覆盖内置规格：

```toml
[command]
name = "my-tool"
description = "My custom tool"

[[command.subcommands]]
name = "deploy"
description = "Deploy to production"

[[command.subcommands.flags]]
name = "--env"
short = "-e"
arg = "ENV"
description = "Target environment"
```

### 贡献 Specs

贡献内置 spec 的步骤：

1. 在 `specs/<command>.toml` 按上述格式创建文件
2. 在 `src/layer/specs.rs` 的 `builtin_specs!` 宏中注册
3. 运行 `cargo test` 验证解析正确

规范：
- 命令和子命令名小写
- Flag 使用 `--kebab-case`，短 flag 使用 `-x`
- 参数占位符大写（`FILE`、`NUM`、`DIR`）
- 描述简洁，不加句号
- 危险 flag（自动执行、绕过安全）应写入 risk pattern，不写入 spec

### 自定义 Failure Patterns

在 `~/.config/awen/failure_patterns.toml`：

```toml
[[failure_patterns]]
pattern = "my custom error: (\\w+)"
suggestion = "my-tool fix {1}"
description = "Fix the custom error"
```

### 自定义 Risk Patterns

在 `~/.config/awen/risk_patterns.toml`：

```toml
[[risk_patterns]]
pattern = "my-dangerous-cmd --force"
warning = "This will force-execute, are you sure?"
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `AWEN_AI_DELAY` | `1` | 停止输入后等待多少秒再发起 AI 请求 |
| `AWEN_LOCAL_THROTTLE_MS` | `20` | 本地建议请求的最小间隔（毫秒，按键节流） |
| `AWEN_CAPTURE_STDERR` | `1` | 设为 `0` 禁用 stderr 捕获 |
| `AWEN_STDERR_MAX_CHARS` | `500` | 发送给 daemon 的最大 stderr 字节数 |
| `AWEN_ENABLE_KEYBIND_OVERRIDE` | `1` | 设为 `0` 禁用 Awen 快捷键覆盖 |
| `AWEN_GHOST_STYLE` | `fg=244` | Ghost text 样式（zsh highlight 格式） |
| `AWEN_STYLE_DIM` | `fg=244` | 暗淡文本样式 |
| `AWEN_STYLE_MUTED` | `fg=250` | 柔和文本样式 |
| `AWEN_STYLE_TEXT` | `fg=255` | 正常文本样式 |
| `AWEN_STYLE_SELECTED` | `fg=255,bold,bg=236` | 选中项样式 |
| `AWEN_STYLE_PANEL` | `fg=240` | 面板边框样式 |
| `AWEN_STYLE_PANEL_BG` | `bg=234` | 面板背景样式 |
| `AWEN_STYLE_HISTORY` | `fg=146` | History 来源标签颜色 |
| `AWEN_STYLE_SPEC` | `fg=69` | Spec 来源标签颜色 |
| `AWEN_STYLE_AI` | `fg=177` | AI 来源标签颜色 |
| `AWEN_STYLE_RISK` | `fg=220` | 风险警告颜色 |
| `AWEN_STYLE_FIX` | `fg=108` | 修复建议颜色 |
| `DEEPSEEK_API_KEY` | — | DeepSeek API 密钥（替代配置文件） |

## 安全边界

Awen 的安全模型极简，因为它**永远只建议，永远不执行**：

- **不自动执行**任何命令 — 每条建议都需用户按键确认
- **不修改文件** — 不读取、不写入用户项目文件
- **不读取敏感文件** — 不访问 `.env`、`.ssh`、`kubeconfig`、AWS credentials 等
- **不泄露隐私** — 发送给 AI 的上下文经过净化（过滤 API key、token、password 等）
- **AI 可关闭** — 设置 `ai.enabled = false`，所有本地功能正常工作
- **无网络也可用** — 历史匹配、specs 补全、风险检测、失败修复 pattern 全部本地运行
- **不做 agent** — 不规划、不执行、不自动化、不做工作流推断

## 开发

### 快速迭代

```bash
make dev       # Debug 构建 + 同步插件 + 重启 daemon（最快）
make release   # Release 构建 + 同步 + 重启
make sync      # 仅同步 plugin/specs（不重新编译，改 zsh 时用）
make test      # cargo test + shellcheck + zsh 冒烟测试
make lint      # clippy + fmt + shellcheck
make status    # 查看 daemon 状态
make logs      # 查看最近的 daemon 日志
```

`make dev` 是主要的开发循环 — 一条命令完成构建、部署、重启，改动立即生效。

### 手动构建

```bash
cargo build
cargo test
cargo clippy
cargo fmt --check
```

### 项目结构

```
src/
├── main.rs           # CLI 入口
├── lib.rs            # 模块导出
├── daemon.rs         # Unix socket server
├── protocol.rs       # JSON 协议定义
├── config.rs         # 配置加载
├── arbitrator.rs     # 建议仲裁
├── sanitize.rs       # 敏感信息过滤
├── context/          # 上下文引擎
│   ├── session.rs    # 会话上下文
│   ├── repo.rs       # 项目类型检测
│   └── git.rs        # Git 上下文
└── layer/            # 补全层
    ├── history.rs    # 历史匹配
    ├── specs.rs      # 命令规格
    ├── ai.rs         # AI 补全
    ├── failure.rs    # 失败修复
    └── risk.rs       # 风险检测
```

## 设计哲学

**低语，不是喊叫。** Ghost text 是灰色的、半透明的。Inline hint 只在高价值场景出现。"少说话"比"更聪明"更重要。

**在你卡住的时候出现。** `cargo build` 失败了，ghost text 已经轻声浮现 `cargo add tokio`。

**像空气一样存在。** 超低延迟、极轻、永不打断、永不越权。装上它，你的 zsh 多了灵感；卸掉它，一切照旧。

## 名称

**Awen** [AH-wen] 是威尔士语，意为"灵感"与"流动的精神"。与 _awel_（微风）同源。

## License

MIT
