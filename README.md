# ArchiveFlow

本地桌面端加密压缩包访问辅助与密码恢复工具，基于 **Tauri 2 + React 19 + Rust** 构建。

## 功能特性

- **归档导入与检测** — 支持 ZIP（完整解析）、7Z / RAR（格式识别），自动检测加密状态与文件条目
- **文件树浏览** — 可展开/折叠的归档内容树，显示文件大小、加密标记
- **密码恢复引擎** — Rust 原生实现，支持两种攻击模式：
  - **字典攻击**：逐个尝试候选密码列表
  - **暴力破解**：自定义字符集（小写/大写/数字/特殊字符）与长度范围穷举
- **实时进度反馈** — 进度条 + 已尝试数 / 总数 / 速度（个/秒）/ 已用时间
- **可取消恢复** — 任意时刻发送取消信号，立即停止后台线程
- **任务管理** — 完整的任务生命周期状态机（导入 → 检查 → 就绪 → 处理中 → 成功/失败）
- **审计事件** — 后端已实现操作记录存储
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

## 项目结构

```
ArchiveFlow/
├── src/                        # 前端源码
│   ├── components/
│   │   └── RecoveryPanel.tsx   # 密码恢复面板组件
│   ├── pages/
│   │   ├── HomePage.tsx        # 拖拽导入 + 最近任务
│   │   ├── TaskPage.tsx        # 任务列表
│   │   ├── TaskDetailPage.tsx  # 任务详情 + 归档内容 + 密码恢复
│   │   ├── ReportPage.tsx      # 审计报告（开发中）
│   │   └── SettingsPage.tsx    # 设置（开发中）
│   ├── services/api.ts         # Tauri invoke 封装
│   ├── stores/taskStore.ts     # Zustand 状态
│   ├── types/index.ts          # TypeScript 类型定义
│   └── i18n/index.ts           # 中英文翻译
├── src-tauri/                  # Rust 后端
│   └── src/
│       ├── commands/           # Tauri 命令（task / archive / audit / recovery）
│       ├── services/           # 业务逻辑（archive_service / recovery_service / audit_service）
│       ├── domain/             # 领域模型
│       └── db/                 # SQLite 数据访问层
└── fixtures/zip/               # 测试用压缩包
```

## 本地开发

### 前置依赖

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77+
- [Tauri 开发依赖](https://tauri.app/start/prerequisites/)（WebView2 on Windows）

### 启动开发服务器

```bash
npm install
npm run tauri dev
```

### 仅构建前端

```bash
npm run build
```

### 打包桌面应用

```bash
npm run tauri build
```

## 任务状态机

```
导入 → 检查中 → 等待授权 → 就绪 → 处理中 → 校验中 → 成功
                                              ↓
                                            失败 → 已清理
```

## 测试夹具

`fixtures/zip/` 目录包含以下测试文件：

| 文件 | 说明 |
|---|---|
| `normal.zip` | 5 个普通文件，无加密 |
| `encrypted-aes.zip` | AES 加密，密码：`test123` |
| `encrypted-strong.zip` | AES 加密，密码：`Str0ng!P@ss` |
| `unicode-names.zip` | 中文文件名 |
| `empty.zip` | 空归档 |
| `many-files.zip` | 100 个文件 |

## 开发路线图

- [x] 项目脚手架（Tauri 2 + React + Rust）
- [x] 归档导入与检测（ZIP 完整解析，7Z/RAR 格式识别）
- [x] 任务管理 CRUD + SQLite 持久化
- [x] 文件树浏览界面
- [x] 密码恢复引擎（字典 + 暴力破解，带进度事件与取消支持）
- [x] 密码恢复前端 UI
- [ ] 审计日志前端界面
- [ ] 设置页面（语言切换、缓存管理）
- [ ] 7Z / RAR 完整条目解析
- [ ] 测试覆盖

## 许可证

MIT
