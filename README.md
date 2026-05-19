# Awen

> **Terminal Intelligence Layer — 让输入变聪明，而不是让 AI 接管终端。**

> Smart when you need it. Silent when you don't.

Awen 是一个开源的终端智能输入增强层，让 Ghostty、Kitty、WezTerm、Alacritty 等任何现代终端都能获得超越 Warp 的智能输入体验。

Awen **永远只建议，永远不执行**。每一条建议都需要你主动接受（按 →、Tab、Enter）。它不会替你做决定、不会执行命令、不会修改文件。

## 核心特性

- **Ghost Text 补全** — 基于历史、命令规格、AI 的内联灰色建议文本
- **失败修复建议** — 上一条命令失败后，自动建议修复命令（只提示不执行）
- **风险检测** — 对 `rm -rf`、`git push --force`、`chmod 777` 等高危命令显示内联警告
- **命令规格补全** — TOML 格式的确定性参数补全，内置 git/docker/npm/cargo/brew/curl/ssh
- **AI 智能补全** — 支持 DeepSeek、Ollama，异步不阻塞，可关闭
- **上下文感知** — 项目类型检测、Git 状态、近期命令、失败历史

## 架构

```
用户终端 (Ghostty / Kitty / WezTerm / Alacritty)
  └─ zsh (ZLE Widget)
       └─ Shell 插件 (awen.zsh)
            │ Unix Socket
            └─ Daemon (Rust + tokio)
                 ├─ Context Engine (session / repo / git)
                 ├─ Layer 1: History (SQLite + nucleo) — < 5ms
                 ├─ Layer 1: Specs (TOML) — < 20ms
                 ├─ Layer 2: AI (DeepSeek / Ollama) — async
                 ├─ Layer 2: Failure Recovery (pattern + AI)
                 ├─ Layer 2: Risk Detection (regex)
                 └─ Suggestion Arbitrator
```

## 安装

### 依赖

- Rust 工具链（1.85+）
- zsh
- socat（可选，用于 shell 与 daemon 通信；如无则使用 zsh 内置 zsocket）

### 从源码安装

```bash
git clone https://github.com/SaoNian/awen.git
cd awen
./install.sh
```

安装脚本会：
1. 编译 release 版本
2. 安装到 `~/.local/bin/awen`
3. 复制 specs 和 zsh 插件到 `~/.config/awen/`
4. 生成默认配置文件

然后在 `~/.zshrc` 中添加：

```bash
source ~/.config/awen/awen.zsh
```

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
| `Esc` | 清除建议 |

### CLI 命令

```bash
awen start     # 启动 daemon
awen stop      # 停止 daemon
awen status    # 查看状态
awen logs      # 查看日志
awen config    # 查看配置
awen context   # 查看当前上下文
```

## 配置

配置文件位于 `~/.config/awen/config.toml`：

```toml
[ai]
enabled = true                  # 开关 AI 补全
provider = "deepseek"           # deepseek | ollama
debounce_ms = 300               # 停止输入后触发 AI 的延迟
max_tokens = 60                 # AI 最大生成 token 数

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

[ui]
ghost_text_color = 242          # ghost text 颜色 (ANSI 256)
dropdown_max_items = 8          # 候选菜单最大条目
risk_detection = true           # 高危命令警告
command_explanation = true      # 命令解释功能
```

### 自定义 Specs

在 `~/.config/awen/specs/` 下创建 TOML 文件：

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

### 构建

```bash
cargo build
```

### 测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
cargo fmt --check
```

### 项目结构

```
src/
├── main.rs           # CLI 入口
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
