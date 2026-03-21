# ArchiveFlow

本地桌面端加密压缩包访问辅助与密码恢复工具，基于 **Tauri 2 + React 19 + Rust** 构建。

> 仅应用于你有权访问、审计、恢复或测试的归档文件。请勿用于未授权目标。

## 功能特性

- **归档导入与检测** — 支持 ZIP / 7Z / RAR，自动检测加密状态、条目列表与常见元数据
- **文件树浏览** — 可展开/折叠的归档内容树，显示文件大小、加密标记
- **CPU 密码恢复引擎** — Rust 原生多线程实现，支持三种攻击模式：
  - **字典攻击**：逐个尝试候选密码列表，支持规则变换（大写/首字母大写/Leet/反转/重复/年份后缀/常见后缀）、分隔符组合、文件名常见模式
  - **暴力破解**：自定义字符集（小写/大写/数字/特殊字符）与长度范围穷举
  - **掩码攻击**：按位指定字符集，如 `?d?d?d?d`（支持 `?l ?u ?d ?s ?a ??`）
- **GPU 加速恢复（hashcat V1）** — 集成外部 hashcat，实现数量级性能提升：
  - 支持 ZIP AES（WinZip，hashcat mode 13600）与 PKZIP/ZipCrypto（mode 17200）
  - 自动发现本机 hashcat（PATH 或 `%LOCALAPPDATA%/ArchiveFlow/tools/hashcat/`）
  - 支持所有 hashcat 兼容 GPU（OpenCL / CUDA）；RTX 4080 实测约 1.2 亿次/秒（PKZIP）
  - 大文件保护：单条目压缩数据超过 10 MB 自动拒绝并引导使用 CPU 引擎
  - 启动时自动检测设备；初始化阶段（kernel 编译/autotune）显示脉冲动画与提示文字
  - V1 限制：仅支持 Windows + ZIP，不支持暂停/继续
- **并行恢复** — CPU 模式默认按 `max(1, num_cpus::get() - 1)` 启用多 worker，兼容 Intel 混合架构与 Apple Silicon
- **实时进度反馈** — 进度条 + 已尝试数 / 总数 / 速度（自动缩放：`118.8M 个/秒`）/ ETA / 设备数 / 最近断点时间
- **可取消 / 暂停 / 恢复** — 调度器支持排队、暂停、继续；GPU 模式支持取消
- **断点续跑** — CPU 恢复进度持久化到 SQLite，应用重启后可继续，并保留优先级上下文
- **任务管理** — 完整的任务生命周期状态机，8 种状态（就绪 / 处理中 / 成功 / 已穷尽 / 已取消 / 失败 / 不支持 / 已中断）
- **审计事件** — 操作日志完整记录，前端可按事件类型筛选浏览
- **结果导出** — 支持 CSV / JSON 导出，包含任务摘要；可按设置默认脱敏密码并选择是否附带审计记录
- **设置页面** — 语言切换、恢复默认配置、导出默认配置、结果保留策略、数据管理、hashcat 路径配置
- **全新桌面 UI** — 首页 / 任务管理 / 审计报告 / 设置 四个页面已统一为新的桌面端布局，强调信息层级、卡片分区与更清晰的操作动线
- **明暗双主题** — 内置浅色 / 深色主题切换，侧栏场景式主题开关支持动画过渡并持久化用户选择
- **响应式设置中心** — 设置页支持双列到单列自适应布局，在较窄窗口下仍可完整操作
- **启动画面** — 赛博朋克风格 boot screen，消除启动时白屏
- **本地 SQLite 持久化** — 所有任务数据与归档元信息存储在本地数据库，无需联网
- **国际化** — 支持简体中文（默认）与英文切换

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | [Tauri 2](https://tauri.app/) |
| 前端 | React 19 + TypeScript 5 + Vite |
| 样式 | Tailwind CSS v4 |
| 状态管理 | Zustand |
| 路由 | react-router-dom |
| 国际化 | react-i18next |
| 图标 | lucide-react |
| 后端 | Rust（tokio + thiserror） |
| 数据库 | SQLite（rusqlite） |
| ZIP 解析 | zip crate v2 |
| 7Z 解析 | sevenz-rust |
| RAR 解析 | unrar |
| GPU 加速 | [hashcat](https://hashcat.net/)（用户自行安装，自动检测） |

## 项目结构

```
ArchiveFlow/
├── src/                        # 前端源码
│   ├── components/
│   │   ├── BootScreen.tsx      # 赛博朋克启动画面
│   │   └── RecoveryPanel.tsx   # 密码恢复面板组件
│   ├── layouts/
│   │   └── MainLayout.tsx      # 主框架布局 + 侧栏导航 + 主题切换
│   ├── pages/
│   │   ├── HomePage.tsx        # 拖拽导入 + 最近任务
│   │   ├── TaskPage.tsx        # 任务列表
│   │   ├── TaskDetailPage.tsx  # 任务详情 + 归档内容 + 密码恢复
│   │   ├── ReportPage.tsx      # 审计日志浏览（按类型筛选）
│   │   └── SettingsPage.tsx    # 语言、恢复配置、数据管理、hashcat 配置
│   ├── stores/
│   │   ├── taskStore.ts        # 任务列表 Zustand 状态
│   │   └── appStore.ts         # 语言、恢复偏好、UI 状态
│   ├── lib/
│   │   ├── format.ts           # 工具函数（formatFileSize / formatElapsed / formatSpeed）
│   │   ├── ui.ts               # Badge / Button 等前端 UI 样式常量
│   │   ├── recoveryCandidates.ts  # 字典变换规则
│   │   └── recoveryObservability.ts  # ETA / 阶段推断
│   ├── services/api.ts         # Tauri invoke 封装
│   ├── router/index.tsx        # 路由（/, /tasks, /tasks/:id, /reports, /settings）
│   ├── types/index.ts          # TypeScript 类型定义
│   └── i18n/index.ts           # 中英文翻译
├── src-tauri/                  # Rust 后端
│   └── src/
│       ├── commands/           # Tauri 命令（task / archive / audit / recovery）
│       ├── services/
│       │   ├── archive_service/   # ZIP / 7Z / RAR 解析
│       │   ├── recovery_service/  # CPU 多线程恢复引擎
│       │   ├── hashcat_service/   # GPU hashcat 集成（检测 / hash 提取 / 运行 / 进度解析）
│       │   └── audit_service/     # 审计日志写入
│       ├── domain/             # 领域模型（TaskStatus、AttackMode、RecoveryStatus）
│       └── db/                 # SQLite 数据访问层
└── fixtures/zip/               # 测试用压缩包
```

## 本地开发

### 前置依赖

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77.2+
- [Tauri 开发依赖](https://tauri.app/start/prerequisites/)（WebView2 on Windows）
- （可选）[hashcat](https://hashcat.net/) — GPU 加速恢复，安装后自动检测

### 启动开发服务器

```bash
npm install
npm run tauri dev
```

### 运行测试

```bash
# 前端单元测试（Vitest）
npm run test:run

# 前端页面/UI 测试
npm run test:ui

# 代码检查
npm run lint

# Rust 单元测试（158 个）
cargo test --manifest-path src-tauri/Cargo.toml

# hashcat 真机集成测试（需本机安装 hashcat）
HASHCAT_PATH=/path/to/hashcat.exe cargo test --manifest-path src-tauri/Cargo.toml -- --ignored
```

### 仅构建前端

```bash
npm run build
```

### 打包桌面应用

```bash
npm run tauri build
```

## GPU 加速配置

GPU 恢复依赖 hashcat V1 集成，当前仅支持 **Windows + ZIP（AES/WinZip 或 PKZIP/ZipCrypto）**。

### 安装 hashcat

1. 从 [hashcat.net/hashcat](https://hashcat.net/hashcat/) 下载最新版本
2. 解压到以下任一位置（自动检测）：
   - 系统 PATH（如 `C:\Windows\System32\hashcat.exe`）
   - `%LOCALAPPDATA%\ArchiveFlow\tools\hashcat\hashcat-x.x.x\hashcat.exe`
3. 或在设置页面手动指定 hashcat.exe 路径

### 验证 GPU 检测

打开 **设置页面 → GPU 加速（hashcat）**，点击"检测 hashcat"，成功后会显示 hashcat 版本号与可用 GPU 设备列表。

### 已知限制（V1）

| 限制 | 说明 |
|---|---|
| 平台 | 仅 Windows |
| 格式 | 仅 ZIP（AES + PKZIP） |
| 文件大小 | 单条目压缩数据 ≤ 10 MB（超出引导使用 CPU） |
| 暂停/继续 | 不支持（取消后需重新启动） |
| CUDA | 推荐安装 CUDA SDK；未安装时自动回退 OpenCL |

## 任务状态机

任务在生命周期内流转以下 8 种状态：

```
就绪 (ready)
  └─→ 处理中 (processing)
        ├─→ 成功 (succeeded)       密码已找到
        ├─→ 已穷尽 (exhausted)     所有候选均已尝试，未找到
        ├─→ 已取消 (cancelled)     用户主动取消
        ├─→ 失败 (failed)          技术错误（如文件损坏）
        ├─→ 不支持 (unsupported)   归档格式无法处理
        └─→ 已中断 (interrupted)   程序异常退出后恢复检测
```

## 当前边界

- 归档恢复目标：ZIP / 7Z / RAR（GPU 加速当前仅 ZIP）
- 恢复模式：字典 / 暴力 / 掩码
- 结果导出：CSV / JSON
- CPU 默认并行度：`max(1, num_cpus::get() - 1)`
- GPU 单条目大小上限：10 MB（压缩后）
- 恢复结果与断点默认保存在本地 SQLite；不依赖云端
- 当前主题系统聚焦桌面端窗口体验，主要适配浅色 / 深色双主题与常见桌面分辨率

## 测试夹具

`fixtures/zip/` 目录包含以下测试文件：

| 文件 | 说明 |
|---|---|
| `normal.zip` | 5 个普通文件，无加密 |
| `encrypted-aes.zip` | AES 加密，密码：`test123` |
| `encrypted-pkzip.zip` | PKZIP/ZipCrypto 加密，密码：`test123` |
| `encrypted-strong.zip` | AES 加密，密码：`Str0ng!P@ss` |
| `unicode-names.zip` | 中文文件名 |
| `empty.zip` | 空归档 |
| `many-files.zip` | 100 个文件 |

## 开发路线图

- [x] 项目脚手架（Tauri 2 + React + Rust）
- [x] 归档导入与检测（ZIP / 7Z / RAR）
- [x] 任务管理 CRUD + SQLite 持久化
- [x] 文件树浏览界面
- [x] 密码恢复引擎（字典 + 暴力破解 + 掩码攻击，带并行、断点与取消/暂停支持）
- [x] 密码恢复前端 UI
- [x] 审计日志前端界面（按事件类型筛选）
- [x] 设置页面（语言切换、恢复/导出默认配置、数据管理）
- [x] 结果导出（CSV / JSON）
- [x] 恢复观测（ETA / worker / 最近断点 / 最近审计）
- [x] 启动画面（赛博朋克风格 boot screen）
- [x] GPU 加速（hashcat 集成，ZIP AES + PKZIP，Windows）
- [x] 速度显示格式化（`118.8M 个/秒`）
- [x] GPU 初始化过渡动画（kernel 编译期间脉冲进度条）
- [ ] GPU 支持扩展至 7Z / RAR
- [ ] 更强的规则文件 / 组合字典导入
- [ ] 基准细分与更深 profiling
- [ ] 调度器重试 / 公平性增强
- [ ] Tauri 真实端到端自动化测试

## 许可证

MIT
