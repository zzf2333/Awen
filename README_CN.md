<div align="center">
  <img src="docs/logo.png" alt="Awen Logo" />
  <h1>Awen</h1>
  <p><b>终端智能层 — 需要时出现，不需要时隐身。</b></p>
  <a href="https://github.com/zzf2333/Awen/releases"><img src="https://img.shields.io/github/v/tag/zzf2333/Awen?label=version&style=flat-square" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/zzf2333/Awen/stargazers"><img src="https://img.shields.io/github/stars/zzf2333/Awen?style=flat-square" alt="Stars"></a>
  <br/>
  <a href="README.md">English</a>
</div>

<br/>

<div align="center">
  <img src="docs/Ghostty.png" width="1000" />
</div>

<br/>

## 为什么做 Awen

现代终端很快，但不聪明。你反复敲同样的命令、忘记参数、打错路径、面对晦涩的报错无从下手。Warp 之类的工具把智能塞进了一个全新终端 — 重、封闭、有自己的一套规矩。

Awen 反其道而行：一个轻量 daemon 挂在你现有的 zsh 上。你打字时浮现 ghost text；命令失败时弹出修复建议；敲了危险命令时闪烁警告。全部在 **20ms** 内完成。AI 作为异步后备可用但从不强制 — 本地优先，始终离线可用。

**只建议，不执行。** Awen 不是 shell agent，不是自动化工具，不是终端模拟器。卡住时它低语一句，顺畅时它消失不见。

名字来自威尔士语，意为"诗意的灵感" — 不请自来的微风，不留痕迹。

## 功能

| 功能 | 做什么 | 速度 |
| :--- | :--- | :--- |
| **Ghost Text** | 基于历史 + 命令规格的行内补全 | <5ms |
| **失败修复** | 识别 18 种报错模式，建议修复命令 | 即时 |
| **风险检测** | 24 种危险命令模式，回车前警告 | 即时 |
| **命令规格** | 77 条内置规格 — 子命令、参数、说明 | <20ms |
| **AI 补全** | DeepSeek / Ollama 异步后备，本地不够时兜底 | 异步 |
| **自然语言** | 输入 `# 查找大文件` → 得到 shell 命令 | 异步 |
| **上下文感知** | 跟踪 git 状态、项目类型、会话历史、上次退出码 | 持续 |

## 安装

### Homebrew（macOS / Linux）

```bash
brew install zzf2333/tap/awen
```

然后在 `~/.zshrc` 中添加：

```bash
source $(brew --prefix)/share/awen/awen.zsh
```

### 一键脚本

```bash
curl -sSL https://raw.githubusercontent.com/zzf2333/Awen/main/install-remote.sh | sh
```

### 从源码构建

**前置条件：** Rust 1.85+、zsh

```bash
git clone https://github.com/zzf2333/Awen.git
cd Awen
./install.sh
```

---

重启 shell 即可。Awen 首次启动会自动导入 zsh 历史。

**可选：** 安装 `jq` 和 `socat` 以获得最佳性能（`brew install jq socat`）。

## 使用

### 快捷键

| 按键 | 操作 |
| :--- | :--- |
| `→` | 接受完整 ghost text |
| `Ctrl+→` | 接受下一个词 |
| `↑↓` | 在建议菜单中导航 |
| `Enter` | 接受选中的建议 |
| `Esc` | 关闭 |

### 自然语言

输入 `#` 加一句描述，Awen 翻译成命令：

```
# 找到所有今天修改过的 go 文件
```

### CLI

```bash
awen start              # 启动 daemon
awen stop               # 停止 daemon
awen status             # 查看状态（pid、运行时间、历史条数）
awen logs               # 查看最近日志
awen config             # 查看配置
awen context            # 查看当前上下文状态
awen history import     # 导入 zsh 历史
```

## 配置

配置文件在 `~/.config/awen/config.toml`，所有字段都有合理默认值。

<details>
<summary>完整配置参考</summary>

```toml
[ai]
enabled = true                  # 开关 AI 补全
provider = "deepseek"           # deepseek | ollama
debounce_ms = 300               # 触发 AI 前的延迟
timeout_ms = 30000              # AI 请求超时（异步，不阻塞输入）
max_tokens = 1024               # AI 最大生成 token 数
min_local_candidates = 2        # 本地候选数低于此值且置信度也低时才触发 AI
min_local_confidence = 0.6      # 本地最高置信度低于此值且候选数也少时才触发 AI
cache_ttl_minutes = 30          # AI 响应缓存有效期

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
git_context = true              # 采集 git 上下文
capture_stderr = true           # 捕获 stderr 用于失败修复

[ui]
ghost_text_color = 242          # ghost text 颜色（ANSI 256）
hint_style = "above"            # 提示位置："above" 或 "below"
dropdown_max_items = 8          # 建议菜单最大条目
risk_detection = true           # 高危命令警告
```
</details>

### 自定义规格

用户规格放在 `~/.config/awen/specs/`，会覆盖同名内置规格。TOML 格式：

```toml
[command]
name = "mycli"
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

### 自定义失败 & 风险模式

在 `~/.config/awen/failure_patterns.toml` 和 `~/.config/awen/risk_patterns.toml` 中添加自定义规则。

<details>
<summary>内置命令规格（77 条）</summary>

| 类别 | 命令 |
| :--- | :--- |
| VCS & 开发生态 | `git`, `docker`, `npm`, `cargo`, `brew`, `curl`, `ssh` |
| 云 & 基础设施 | `gh`, `kubectl`, `terraform`, `aws`, `gcloud`, `az`, `helm` |
| 语言 & 运行时 | `python`, `go`, `node` |
| 包管理 | `pip`, `pnpm`, `yarn`, `bun`, `uv`, `poetry`, `cmake`, `make` |
| AI 工具 | `claude`, `codex`, `opencode`, `antigravity` |
| 文件操作 | `ls`, `rm`, `cp`, `mv`, `mkdir`, `touch`, `ln`, `chmod`, `chown` |
| 文本处理 | `cat`, `head`, `tail`, `grep`, `sed`, `awk`, `sort`, `uniq`, `wc`, `diff`, `cut`, `tr`, `tee`, `xargs` |
| 搜索 & 归档 | `find`, `tar` |
| 进程 & 系统 | `ps`, `kill`, `df`, `du`, `lsof`, `htop` |
| 网络 | `ping`, `dig`, `wget`, `ss`, `nmap` |
| 系统管理 | `systemctl`, `journalctl` |
| 终端复用 | `tmux`, `screen` |
| 测试 & Lint | `pytest`, `ruff` |
| 任务运行 | `just` |
| 数据库 CLI | `psql`, `mysql`, `redis-cli`, `mongosh`, `sqlite3` |
</details>

## 安全边界

- **从不执行** — 所有建议都需要用户明确操作
- **从不读取敏感文件** — .env、.ssh、kubeconfig、凭证文件一律不碰
- **隐私过滤** — 含 key/token/secret/password 的环境变量被脱敏；stderr 中的 token 被遮蔽
- **离线可用** — 所有本地功能无需网络；AI 完全可选
- **不是 agent** — 不修改文件、不后台执行、无副作用

## 开发

```bash
make dev              # Debug 构建 + 同步 + 重启 daemon
make release          # Release 构建 + 同步 + 重启
make test             # cargo test + shellcheck + zsh 冒烟测试
make lint             # clippy + fmt check + shellcheck
make sync             # 只复制 specs/plugin（不重新编译）
make status           # 查看 daemon 状态
make logs             # 查看 daemon 日志
```

<details>
<summary>项目结构</summary>

```
src/
├── main.rs              # CLI 入口（clap）
├── daemon.rs            # Unix socket 服务，请求分发
├── protocol.rs          # JSON 请求/响应类型
├── config.rs            # TOML 配置，serde 默认值
├── pipeline.rs          # AI 触发策略，合并逻辑
├── arbitrator.rs        # 去重、上下文加权、排序、取前 8
├── sanitize.rs          # 隐私过滤
├── context/
│   ├── mod.rs           # 上下文引擎
│   ├── session.rs       # 会话历史环
│   ├── git.rs           # Git 分支/状态
│   └── repo.rs          # 项目类型检测
└── layer/
    ├── history.rs       # SQLite + nucleo 模糊匹配
    ├── specs.rs         # TOML 命令规格
    ├── ai.rs            # DeepSeek / Ollama 提供方
    ├── failure.rs       # stderr → 修复建议
    ├── risk.rs          # 输入 → 危险警告
    └── history_import.rs
plugin/
└── awen.zsh             # zsh widget（ghost text、菜单、提示）
specs/
└── *.toml               # 77 条内置命令规格
```
</details>

## 设计哲学

- **低语，而非喊叫。** 建议以 ghost text 呈现 — 可见但不打扰。没有弹窗、没有声音、不抢注意力。
- **卡住时才出现。** 最好的工具是你忘记它在运行的那种。Awen 在你犹豫、失败或触碰危险时才现身。
- **本地优先，AI 其次。** 历史和规格在 5ms 内响应。AI 是后备而非依赖 — 完全可选。
- **如空气般存在。** 随 shell 启动的终端 daemon，占用极少资源，关掉终端就消失。

## License

MIT
